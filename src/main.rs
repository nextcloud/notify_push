use crate::config::Config;
use crate::connection::ActiveConnections;
use crate::event::{Event, GroupUpdate, ShareCreate, StorageUpdate};
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use color_eyre::{eyre::WrapErr, Report, Result};
use futures::stream::SplitStream;
use futures::{FutureExt, StreamExt};
use redis::Client;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use warp::filters::ws::Message;
use warp::ws::WebSocket;
use warp::Filter;

mod config;
mod connection;
mod event;
mod storage_mapping;
mod user;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let config = Config::from_env().wrap_err("Failed to load config")?;

    let connections = ActiveConnections::default();

    let mapping = StorageMapping::new(&config.database_url, config.database_prefix).await?;
    let client = redis::Client::open(config.redis_url)?;
    let active_connections = connections.clone();
    let connections = warp::any().map(move || connections.clone());

    let cors = warp::cors().allow_any_origin();

    // GET /ws -> websocket upgrade
    let socket = warp::path("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(connections)
        .map(|ws: warp::ws::Ws, users| ws.on_upgrade(move |socket| user_connected(socket, users)))
        .with(cors);

    let routes = socket;

    tokio::task::spawn(listen(client, active_connections, mapping));

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
    Ok(())
}

async fn user_connected(ws: WebSocket, connections: ActiveConnections) {
    let (user_ws_tx, mut user_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::task::spawn(rx.forward(user_ws_tx).map(|result| {
        if let Err(e) = result {
            eprintln!("websocket send error: {}", e);
        }
    }));

    let user_id = match socket_auth(&mut user_ws_rx).await {
        Ok(user_id) => user_id,
        Err(e) => {
            log::warn!("{}", e);
            let _ = tx.send(Ok(Message::text(format!("err: {}", e))));
            return;
        }
    };

    let connection_id = connections.add(user_id.clone(), tx.clone());

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

    connections.remove(&user_id, connection_id);
}

async fn read_socket_auth_message(rx: &mut SplitStream<WebSocket>) -> Result<Message> {
    match timeout(Duration::from_secs(1), rx.next()).await {
        Ok(Some(Ok(msg))) => Ok(msg),
        Ok(Some(Err(e))) => Err(Report::from(e).wrap_err("Socket error during authentication")),
        Ok(None) => Err(Report::msg("Client disconnected during authentication")),
        Err(_) => Err(Report::msg("Authentication timeout")),
    }
}

async fn socket_auth(rx: &mut SplitStream<WebSocket>) -> Result<UserId> {
    let username_msg = read_socket_auth_message(rx).await?;
    let username = username_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;
    let password_msg = read_socket_auth_message(rx).await?;
    let _password = password_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;

    // todo: actually authenticate
    log::debug!("Authenticated socket for {}", username);
    Ok(UserId::from(username))
}

async fn listen(
    client: Client,
    connections: ActiveConnections,
    mapping: StorageMapping,
) -> Result<()> {
    let mut event_stream = event::subscribe(client).await?;
    while let Some(event) = event_stream.next().await {
        match event {
            Ok(Event::StorageUpdate(StorageUpdate { storage, path })) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received storage update notification for storage {} and path {}",
                    storage,
                    path
                );
                match mapping.get_users_for_storage_path(storage, &path).await {
                    Ok(users) => {
                        for user in users {
                            connections
                                .send_to_user(&user, "notify_storage_update")
                                .await;
                        }
                    }
                    Err(e) => log::error!("{:#}", e),
                }
            }
            Ok(Event::GroupUpdate(GroupUpdate { user, .. })) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received group update notification for user {}",
                    user
                );
                connections
                    .send_to_user(&user, "notify_storage_update")
                    .await;
            }
            Ok(Event::ShareCreate(ShareCreate { user, .. })) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received share create notification for user {}",
                    user
                );
                connections
                    .send_to_user(&user, "notify_storage_update")
                    .await;
            }
            Err(e) => log::warn!("{:#}", e),
        }
    }
    Ok(())
}
