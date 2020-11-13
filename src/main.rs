use crate::connection::ActiveConnections;
use crate::event::StorageUpdate;
use crate::storage_mapping::StorageMapping;
use futures::{FutureExt, StreamExt};
use main_error::MainError;
use redis::{Client, RedisError};
use tokio::sync::mpsc;
use warp::ws::WebSocket;
use warp::Filter;

mod connection;
mod event;
mod storage_mapping;

#[tokio::main]
async fn main() -> Result<(), MainError> {
    pretty_env_logger::init();
    let _ = dotenv::dotenv();

    let connections = ActiveConnections::default();
    let mapping = StorageMapping::new(&std::env::var("DATABASE_URL")?).await?;
    let client = redis::Client::open(std::env::var("REDIS_URL")?)?;
    let active_connections = connections.clone();
    let connections = warp::any().map(move || connections.clone());

    // GET /ws -> websocket upgrade
    let socket = warp::path("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(connections)
        .map(|ws: warp::ws::Ws, users| ws.on_upgrade(move |socket| user_connected(socket, users)));

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

    // Save the sender in our list of connected users.
    let mut connection_id = None;
    let mut user_id = None;

    let connections2 = connections.clone();

    // Every time the user sends a message, broadcast it to
    // all other users...
    while let Some(result) = user_ws_rx.next().await {
        let msg = match result {
            Ok(msg) => msg,
            Err(e) => {
                eprintln!("websocket error(uid={:?}): {}", connection_id, e);
                break;
            }
        };
        if let (Ok(message), None) = (msg.to_str(), connection_id) {
            println!("listing to changes for {}", message);
            user_id = Some(message.to_string());
            connection_id = Some(connections.add(message.to_string(), tx.clone()).await);
        }
    }

    if let (Some(connection_id), Some(user_id)) = (connection_id, user_id) {
        // user_ws_rx stream will keep processing as long as the user stays connected
        connections2.remove(&user_id, connection_id).await;
    }
}

async fn listen(
    client: Client,
    connections: ActiveConnections,
    mapping: StorageMapping,
) -> Result<(), RedisError> {
    let mut event_stream = event::subscribe(client).await?;
    while let Some(event) = event_stream.next().await {
        match event {
            event::Event::StorageUpdate(StorageUpdate { storage }) => {
                match mapping.get_users_for_storage(storage).await {
                    Ok(users) => {
                        for user in users {
                            connections
                                .send_to_user(&user, "notify_storage_update")
                                .await;
                        }
                    }
                    Err(e) => eprintln!("{:?}", e),
                }
            }
        }
    }
    Ok(())
}
