use crate::config::Config;
use crate::connection::ActiveConnections;
use crate::event::{Event, GroupUpdate, ShareCreate, StorageUpdate};
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use color_eyre::{eyre::WrapErr, Report, Result};
use futures::stream::SplitStream;
use futures::{FutureExt, StreamExt};
use once_cell::sync::OnceCell;
use redis::Client;
use smallvec::alloc::sync::Arc;
use std::convert::Infallible;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;
use warp::filters::ws::Message;
use warp::ws::WebSocket;
use warp::Filter;
use warp_real_ip::real_ip;

mod config;
mod connection;
mod event;
mod nc;
mod storage_mapping;
mod user;

static NC_CLIENT: OnceCell<nc::Client> = OnceCell::new();

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();

    let args = std::env::args();
    let config = match args.skip(1).next() {
        Some(file) => {
            Config::from_file(&file).wrap_err("Failed to load config from nextcloud config file")?
        }
        None => Config::from_env().wrap_err("Failed to load config from environment variables")?,
    };

    let connections = ActiveConnections::default();
    let nc_client = nc::Client::new(&config.nextcloud_url)?;
    let test_cookie = Arc::new(AtomicU32::new(0));
    let _ = NC_CLIENT.set(nc_client);

    let mapping =
        Arc::new(StorageMapping::new(&config.database_url, config.database_prefix).await?);
    let client = redis::Client::open(config.redis_url)?;

    tokio::task::spawn(
        listen(
            client,
            connections.clone(),
            mapping.clone(),
            test_cookie.clone(),
        )
        .map(|res| match res {
            Err(e) => {
                eprintln!("{:#}", e);
                std::process::exit(1);
            }
            _ => {}
        }),
    );

    let connections = warp::any().map(move || connections.clone());
    let test_cookie = warp::any().map(move || test_cookie.clone());
    let mapping = warp::any().map(move || mapping.clone());

    let cors = warp::cors().allow_any_origin();

    // GET /ws -> websocket upgrade
    let socket = warp::path!("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(connections)
        .and(real_ip(config.trusted_proxies))
        .map(|ws: warp::ws::Ws, users, addr: Option<IpAddr>| {
            ws.on_upgrade(move |socket| user_connected(socket, users, addr))
        })
        .with(cors);

    let cookie_test =
        warp::path!("test" / "cookie")
            .and(test_cookie)
            .map(|test_cookie: Arc<AtomicU32>| {
                let cookie = test_cookie.load(Ordering::SeqCst);
                cookie.to_string()
            });

    let reverse_cookie_test = warp::path!("test" / "reverse_cookie").and_then(|| async move {
        let client = NC_CLIENT.get().unwrap();
        let cookie = client.get_test_cookie().await.unwrap_or(0);
        Result::<_, Infallible>::Ok(cookie.to_string())
    });

    let mapping_test = warp::path!("test" / "mapping" / u32).and(mapping).and_then(
        |storage_id: u32, mapping: Arc<StorageMapping>| async move {
            let access = mapping
                .get_users_for_storage_path(storage_id, "")
                .await
                .map(|access| access.count())
                .unwrap_or(0);
            Result::<_, Infallible>::Ok(access.to_string())
        },
    );

    let remote_test =
        warp::path!("test" / "remote" / IpAddr).and_then(|remote: IpAddr| async move {
            let client = NC_CLIENT.get().unwrap();
            let result = client
                .test_set_remote(remote)
                .await
                .map(|remote| remote.to_string())
                .unwrap_or_else(|e| e.to_string());
            Result::<_, Infallible>::Ok(result)
        });

    let routes = socket
        .or(cookie_test)
        .or(reverse_cookie_test)
        .or(mapping_test)
        .or(remote_test);

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
    Ok(())
}

async fn user_connected(ws: WebSocket, connections: ActiveConnections, remote: Option<IpAddr>) {
    let (user_ws_tx, mut user_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::task::spawn(rx.forward(user_ws_tx).map(|result| {
        if let Err(e) = result {
            eprintln!("websocket send error: {}", e);
        }
    }));

    let user_id = match socket_auth(&mut user_ws_rx, remote).await {
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

async fn socket_auth(rx: &mut SplitStream<WebSocket>, remote: Option<IpAddr>) -> Result<UserId> {
    let username_msg = read_socket_auth_message(rx).await?;
    let username = username_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;
    let password_msg = read_socket_auth_message(rx).await?;
    let password = password_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;

    let client = NC_CLIENT.get().unwrap();
    if client
        .verify_credentials(username, password, remote)
        .await?
    {
        log::info!("Authenticated socket for {}", username);
        Ok(UserId::from(username))
    } else {
        Err(Report::msg("Invalid credentials"))
    }
}

async fn listen(
    client: Client,
    connections: ActiveConnections,
    mapping: Arc<StorageMapping>,
    test_cookie: Arc<AtomicU32>,
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
                            connections.send_to_user(&user, "notify_file").await;
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
                connections.send_to_user(&user, "notify_file").await;
            }
            Ok(Event::ShareCreate(ShareCreate { user, .. })) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received share create notification for user {}",
                    user
                );
                connections.send_to_user(&user, "notify_file").await;
            }
            Ok(Event::TestCookie(cookie)) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received test cookie {}",
                    cookie
                );
                test_cookie.store(cookie, Ordering::SeqCst);
            }
            Err(e) => log::warn!("{:#}", e),
        }
    }
    Ok(())
}
