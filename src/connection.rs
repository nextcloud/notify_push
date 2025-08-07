/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::error::{AuthenticationError, WebSocketError};
use crate::message::{PushMessage, SendQueue};
use crate::metrics::METRICS;
use crate::passthru_hasher::PassthruHasher;
use crate::Result;
use crate::{App, UserId};
use dashmap::mapref::entry::Entry;
use dashmap::DashMap;
use futures::{future::select, pin_mut, SinkExt, StreamExt};
use rand::{thread_rng, Rng, SeedableRng};
use std::net::IpAddr;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::time::timeout;
use warp::filters::ws::{Message, WebSocket};

const USER_CONNECTION_LIMIT: usize = 64;
const PING_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, broadcast::Sender<PushMessage>, PassthruHasher>);

impl ActiveConnections {
    pub fn add(&self, user: UserId) -> Result<broadcast::Receiver<PushMessage>> {
        match self.0.entry(user) {
            Entry::Occupied(entry) => {
                let sender = entry.get();
                if sender.receiver_count() > USER_CONNECTION_LIMIT {
                    Err(AuthenticationError::LimitExceeded.into())
                } else {
                    Ok(sender.subscribe())
                }
            }
            Entry::Vacant(entry) => {
                METRICS.add_user();
                let (tx, rx) = broadcast::channel(4);
                entry.insert(tx);
                Ok(rx)
            }
        }
    }

    pub fn send_to_user(&self, user: &UserId, msg: PushMessage) {
        if let Some(tx) = self.0.get(user) {
            tx.send(msg).ok();
        }
    }

    pub fn remove(&self, user: &UserId) {
        if let Entry::Occupied(e) = self.0.entry(user.clone()) {
            if e.get().receiver_count() == 1 {
                log::debug!("Removing {user} from active connections");
                METRICS.remove_user();
                e.remove();
            }
        }
    }
}

#[derive(Default)]
pub struct ConnectionOptions {
    pub listen_file_id: AtomicBool,
    pub max_debounce_time: usize,
    pub max_connection_time: Duration,
}

impl ConnectionOptions {
    pub fn new(max_debounce_time: usize, max_connection_time: usize) -> Self {
        ConnectionOptions {
            max_debounce_time,
            max_connection_time: Duration::from_secs(max_connection_time as u64),
            ..ConnectionOptions::default()
        }
    }
}

pub async fn handle_user_socket(
    mut ws: WebSocket,
    app: Arc<App>,
    forwarded_for: Vec<IpAddr>,
    opts: ConnectionOptions,
) {
    let user_id = match timeout(
        Duration::from_secs(15),
        socket_auth(&mut ws, forwarded_for, &app),
    )
    .await
    {
        Ok(Ok(user_id)) => user_id,
        Ok(Err(e)) => {
            log::warn!("{e:#}");
            ws.send(Message::text(format!("err: {e:#}"))).await.ok();
            return;
        }
        Err(_) => {
            ws.send(Message::text("Authentication timeout".to_string()))
                .await
                .ok();
            return;
        }
    };

    log::info!("new websocket authenticated as {user_id}");
    ws.send(Message::text("authenticated")).await.ok();

    let mut rx = match app.connections.add(user_id.clone()) {
        Ok(rx) => rx,
        Err(e) => {
            ws.send(Message::text(e.to_string())).await.ok();
            return;
        }
    };

    let (mut user_ws_tx, mut user_ws_rx) = ws.split();

    METRICS.add_connection();

    // Every time we send a ping, we set this to a random non-zero value
    // when a pong is returned, we check it against the expected value and reset this to 0
    // If we get the wrong pong back, or the expected value hasn't been cleared
    // when we send the next ping, we close the connection
    let expect_pong = AtomicUsize::default();
    let expect_pong = &expect_pong;

    let transmit = async {
        // Use faster random generator for generating ping messages and time smearthey dont need to be
        // cryptographically secure.
        let mut rng =
            rand::rngs::SmallRng::from_rng(thread_rng()).expect("Failed to initialize rng");

        // for each connection we randomize the max debounce time to remove the chance that many connections
        // get messages at the same time and cause load peaks
        let debounce_factor = rng.gen_range(0.5..1.5);
        let mut send_queue = SendQueue::new(opts.max_debounce_time, debounce_factor);

        let mut reset = app.reset_rx();

        let connection_start_time = Instant::now();
        let mut last_send = connection_start_time - PING_INTERVAL;

        'tx_loop: loop {
            tokio::select! {
                msg = timeout(Duration::from_millis(500), rx.recv()) => {
                    let now = Instant::now();
                    match msg {
                        Ok(Ok(msg)) => {
                            if let Some(msg) = send_queue.push(msg, now) {
                                log::debug!(target: "notify_push::send", "Sending {msg} to {user_id}");
                                METRICS.add_message(msg.message_type());
                                last_send = now;
                                user_ws_tx.send(msg.into_message(&opts)).await.ok();
                            }
                        }
                        Err(_timout) => {
                            if opts.max_connection_time != Duration::ZERO && now - connection_start_time > opts.max_connection_time {
                                user_ws_tx.close().await.ok();
                                log::debug!("Connection closed by exceeding maximum connection time");
                                break 'tx_loop;
                            }

                            for msg in send_queue.drain(now, METRICS.active_connection_count()) {
                                last_send = now;
                                METRICS.add_message(msg.message_type());
                                log::debug!(target: "notify_push::send", "Sending debounced {msg} to {user_id}");
                                user_ws_tx.feed(msg.into_message(&opts)).await.ok();
                            }

                            if now.duration_since(last_send) > PING_INTERVAL {
                                let data = rng.gen::<NonZeroUsize>().into();
                                let last_ping = expect_pong.swap(data, Ordering::SeqCst);
                                if last_ping > 0 {
                                    log::info!("{user_id} didn't reply to ping, closing");
                                    break;
                                }
                                log::debug!(target: "notify_push::send", "Sending ping to {user_id}");
                                last_send = now;
                                user_ws_tx
                                    .feed(Message::ping(data.to_le_bytes()))
                                    .await
                                    .ok();
                            }
                            user_ws_tx.flush().await.ok();
                        }
                        Ok(Err(_)) => {
                            // we dont care about dropped messages
                        }
                    }
                },
                _ = reset.recv() => {
                    user_ws_tx.close().await.ok();
                    log::debug!("Connection closed by reset request");
                    break 'tx_loop;
                },
            };
        }
    };

    let receive = async {
        // handle messages until the client closes the connection
        while let Some(result) = user_ws_rx.next().await {
            match result {
                Ok(msg) if msg.is_pong() => {
                    let expected = expect_pong.swap(0, Ordering::SeqCst);
                    if msg.as_bytes() != expected.to_le_bytes() {
                        log::info!("received wrong pong, closing");
                        break;
                    }
                }
                Ok(msg) if msg.is_text() => {
                    let text = msg.to_str().unwrap_or_default();
                    if text == "listen notify_file_id" {
                        opts.listen_file_id.store(true, Ordering::Relaxed);
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let formatted = e.to_string();
                    // hack while warp only has opaque error types
                    match formatted.as_str() {
                        "WebSocket protocol error: Connection reset without closing handshake"
                        | "IO error: Connection reset by peer (os error 104)" => {
                            log::debug!("websocket error: {e:#}")
                        }
                        _ => log::warn!("websocket error: {e:#}"),
                    };
                    break;
                }
            };
        }
    };

    pin_mut!(transmit);
    pin_mut!(receive);

    select(transmit, receive).await;

    METRICS.remove_connection();
    app.connections.remove(&user_id);
}

async fn read_socket_auth_message(rx: &mut WebSocket) -> Result<Message, WebSocketError> {
    match rx.next().await {
        Some(Ok(msg)) => Ok(msg),
        Some(Err(e)) => Err(e.into()),
        None => Err(WebSocketError::Disconnected),
    }
}

async fn socket_auth(
    rx: &mut WebSocket,
    forwarded_for: Vec<IpAddr>,
    app: &App,
) -> Result<UserId, AuthenticationError> {
    let username_msg = read_socket_auth_message(rx).await?;
    let username = username_msg
        .to_str()
        .map_err(|_| AuthenticationError::InvalidMessage)?;
    let password_msg = read_socket_auth_message(rx).await?;
    let password = password_msg
        .to_str()
        .map_err(|_| AuthenticationError::InvalidMessage)?;

    // cleanup all pre_auth tokens older than 15s
    let cutoff = Instant::now() - Duration::from_secs(15);
    app.pre_auth.retain(|_, (time, _)| *time > cutoff);

    if let Some((_, (_, user))) = app.pre_auth.remove(password) {
        log::debug!("Authenticated socket for {user} using pre authenticated token");
        return Ok(user);
    }

    if !username.is_empty() {
        Ok(app
            .nc_client
            .verify_credentials(username, password, forwarded_for)
            .await?)
    } else {
        Err(AuthenticationError::Invalid)
    }
}
