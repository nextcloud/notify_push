use crate::message::{DebounceMap, MessageType};
use crate::metrics::METRICS;
use crate::{App, UserId};
use ahash::RandomState;
use color_eyre::{Report, Result};
use dashmap::DashMap;
use futures::{future::select, pin_mut, SinkExt, StreamExt};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::{channel, Receiver, Sender};
use tokio::time::timeout;
use warp::filters::ws::{Message, WebSocket};

const USER_CONNECTION_LIMIT: usize = 64;

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, Sender<MessageType>, RandomState>);

impl ActiveConnections {
    pub async fn add(&self, user: UserId) -> Result<Receiver<MessageType>> {
        if let Some(sender) = self.0.get(&user) {
            // stop a single user from trying to eat all the resources
            if sender.receiver_count() > USER_CONNECTION_LIMIT {
                Err(Report::msg("connection limit exceeded"))
            } else {
                Ok(sender.subscribe())
            }
        } else {
            let (tx, rx) = channel(4);
            self.0.insert(user, tx);
            Ok(rx)
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: MessageType) {
        if let Some(tx) = self.0.get(user) {
            log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);

            tx.send(msg).ok();
        }
    }
}

pub async fn handle_user_socket(mut ws: WebSocket, app: Arc<App>, forwarded_for: Vec<IpAddr>) {
    let user_id = match timeout(
        Duration::from_secs(1),
        socket_auth(&mut ws, forwarded_for, &app),
    )
    .await
    {
        Ok(Ok(user_id)) => user_id,
        Ok(Err(e)) => {
            log::warn!("{}", e);
            ws.send(Message::text(format!("err: {}", e))).await.ok();
            return;
        }
        Err(_) => {
            ws.send(Message::text("Authentication timeout".to_string()))
                .await
                .ok();
            return;
        }
    };

    log::debug!("new websocket authenticated as {}", user_id);
    ws.send(Message::text("authenticated")).await.ok();

    let mut rx = match app.connections.add(user_id.clone()).await {
        Ok(rx) => rx,
        Err(e) => {
            ws.send(Message::text(e.to_string())).await.ok();
            return;
        }
    };

    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    METRICS.add_connection();

    let transmit = async move {
        let mut debounce = DebounceMap::default();
        loop {
            // we dont care about dropped messages
            if let Ok(msg) = rx.recv().await {
                if debounce.should_send(&msg) {
                    METRICS.add_message();
                    user_ws_tx.send(Message::text(msg.to_string())).await.ok();
                }
            }
        }
    };

    let receive = async move {
        // handle messages until the client closes the connection
        while let Some(result) = user_ws_rx.next().await {
            let _msg = match result {
                Ok(msg) => msg,
                Err(e) => {
                    log::warn!("websocket error: {}", e);
                    break;
                }
            };
        }
    };

    pin_mut!(transmit);
    pin_mut!(receive);

    select(transmit, receive).await;

    METRICS.remove_connection();
}

async fn read_socket_auth_message(rx: &mut WebSocket) -> Result<Message> {
    match rx.next().await {
        Some(Ok(msg)) => Ok(msg),
        Some(Err(e)) => Err(Report::from(e).wrap_err("Socket error during authentication")),
        None => Err(Report::msg("Client disconnected during authentication")),
    }
}

async fn socket_auth(rx: &mut WebSocket, forwarded_for: Vec<IpAddr>, app: &App) -> Result<UserId> {
    let username_msg = read_socket_auth_message(rx).await?;
    let username = username_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;
    let password_msg = read_socket_auth_message(rx).await?;
    let password = password_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;

    // cleanup all pre_auth tokens older than 15s
    let cutoff = Instant::now() - Duration::from_secs(15);
    app.pre_auth.retain(|_, (time, _)| *time > cutoff);

    if let Some((_, (_, user))) = app.pre_auth.remove(password) {
        log::info!(
            "Authenticated socket for {} using pre authenticated token",
            user
        );
        return Ok(user);
    }

    if !username.is_empty() {
        app.nc_client
            .verify_credentials(username, password, forwarded_for)
            .await
    } else {
        Err(Report::msg("Invalid credentials"))
    }
}
