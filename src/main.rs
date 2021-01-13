use crate::config::Config;
use crate::connection::ActiveConnections;
use crate::event::{
    Activity, Event, GroupUpdate, Notification, PreAuth, ShareCreate, StorageUpdate,
};
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use color_eyre::{eyre::WrapErr, Report, Result};
use dashmap::DashMap;
use futures::stream::SplitStream;
use futures::{FutureExt, StreamExt};
use once_cell::sync::OnceCell;
use redis::Client;
use smallvec::alloc::sync::Arc;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::timeout;
use warp::filters::addr::remote;
use warp::filters::ws::Message;
use warp::ws::WebSocket;
use warp::Filter;
use warp_real_ip::get_forwarded_for;

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
    let _ = dotenv::dotenv();

    let args = std::env::args();
    let config = match args.skip(1).next() {
        Some(file) => {
            Config::from_file(&file).wrap_err("Failed to load config from nextcloud config file")?
        }
        None => Config::from_env().wrap_err("Failed to load config from environment variables")?,
    };

    log::trace!("Running with config: {:?}", config);

    let connections = ActiveConnections::default();
    let nc_client = nc::Client::new(&config.nextcloud_url)?;
    let test_cookie = Arc::new(AtomicU32::new(0));
    let port = dotenv::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(80u16);
    let _ = NC_CLIENT.set(nc_client);

    let mapping =
        Arc::new(StorageMapping::new(&config.database_url, config.database_prefix).await?);
    let pre_auth: Arc<DashMap<String, (Instant, UserId)>> = Arc::default();

    let _ = mapping
        .get_users_for_storage_path(1, "")
        .await
        .wrap_err("Failed to test database access")?;

    let client = redis::Client::open(config.redis_url)?;

    tokio::task::spawn(
        listen(
            client,
            connections.clone(),
            mapping.clone(),
            test_cookie.clone(),
            pre_auth.clone(),
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
    let pre_auth = warp::any().map(move || pre_auth.clone());

    let cors = warp::cors().allow_any_origin();

    // GET /ws -> websocket upgrade
    let socket = warp::path!("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(connections)
        .and(pre_auth)
        .and(remote())
        .and(get_forwarded_for())
        .map(
            |ws: warp::ws::Ws,
             users,
             pre_auth,
             remote: Option<SocketAddr>,
             mut forwarded_for: Vec<IpAddr>| {
                if let Some(remote) = remote {
                    forwarded_for.push(remote.ip());
                }
                log::debug!("new websocket connection from {:?}", forwarded_for.first());
                ws.on_upgrade(move |socket| user_connected(socket, users, forwarded_for, pre_auth))
            },
        )
        .with(cors);

    let cookie_test =
        warp::path!("test" / "cookie")
            .and(test_cookie)
            .map(|test_cookie: Arc<AtomicU32>| {
                let cookie = test_cookie.load(Ordering::SeqCst);
                log::debug!("current test cookie is {}", cookie);
                cookie.to_string()
            });

    let reverse_cookie_test = warp::path!("test" / "reverse_cookie").and_then(|| async move {
        let client = NC_CLIENT.get().unwrap();
        let cookie = client.get_test_cookie().await.unwrap_or(0);
        log::debug!("got remote test cookie {}", cookie);
        Result::<_, Infallible>::Ok(cookie.to_string())
    });

    let mapping_test = warp::path!("test" / "mapping" / u32).and(mapping).and_then(
        |storage_id: u32, mapping: Arc<StorageMapping>| async move {
            let access = mapping
                .get_users_for_storage_path(storage_id, "")
                .await
                .map(|access| {
                    let count = access.count();
                    log::debug!("storage mapping count for {} = {}", storage_id, count);
                    count
                })
                .map_err(|err| {
                    log::error!(
                        "error while getting mapping count for {}: {:#}",
                        storage_id,
                        err
                    );
                    err
                })
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
            log::debug!("got remote {} when trying to set remote {}", result, remote);
            Result::<_, Infallible>::Ok(result)
        });

    let routes = socket
        .or(cookie_test)
        .or(reverse_cookie_test)
        .or(mapping_test)
        .or(remote_test);

    warp::serve(routes).run(([127, 0, 0, 1], port)).await;
    Ok(())
}

async fn user_connected(
    ws: WebSocket,
    connections: ActiveConnections,
    forwarded_for: Vec<IpAddr>,
    pre_auth: Arc<DashMap<String, (Instant, UserId)>>,
) {
    let (user_ws_tx, mut user_ws_rx) = ws.split();

    // Use an unbounded channel to handle buffering and flushing of messages
    // to the websocket...
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::task::spawn(rx.forward(user_ws_tx).map(|result| {
        if let Err(e) = result {
            eprintln!("websocket send error: {}", e);
        }
    }));

    let user_id = match socket_auth(&mut user_ws_rx, forwarded_for, pre_auth).await {
        Ok(user_id) => user_id,
        Err(e) => {
            log::warn!("{}", e);
            let _ = tx.send(Ok(Message::text(format!("err: {}", e))));
            return;
        }
    };

    log::debug!("new websocket authenticated as {}", user_id);
    let _ = tx.send(Ok(Message::text("authenticated")));

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

async fn socket_auth(
    rx: &mut SplitStream<WebSocket>,
    forwarded_for: Vec<IpAddr>,
    pre_auth: Arc<DashMap<String, (Instant, UserId)>>,
) -> Result<UserId> {
    let username_msg = read_socket_auth_message(rx).await?;
    let username = username_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;
    let password_msg = read_socket_auth_message(rx).await?;
    let password = password_msg
        .to_str()
        .map_err(|_| Report::msg("Invalid authentication message"))?;

    // cleanup all pre_auth tokens older than 15s
    let now = Instant::now();
    pre_auth.retain(|_, (time, _)| now.duration_since(*time) < Duration::from_secs(15));

    if let Some((_, (_, user))) = pre_auth.remove(password) {
        log::info!(
            "Authenticated socket for {} using pre authenticated token",
            user
        );
        return Ok(user);
    }

    let client = NC_CLIENT.get().unwrap();
    if client
        .verify_credentials(username, password, forwarded_for)
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
    pre_auth: Arc<DashMap<String, (Instant, UserId)>>,
) -> Result<()> {
    let mut event_stream = event::subscribe(client).await?;
    while let Some(event) = event_stream.next().await {
        if let Ok(event) = &event {
            log::debug!(
                target: "notify_push::receive",
                "Received {}",
                event
            );
        }
        match event {
            Ok(Event::StorageUpdate(StorageUpdate { storage, path })) => {
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
                connections.send_to_user(&user, "notify_file").await;
            }
            Ok(Event::ShareCreate(ShareCreate { user, .. })) => {
                connections.send_to_user(&user, "notify_file").await;
            }
            Ok(Event::TestCookie(cookie)) => {
                test_cookie.store(cookie, Ordering::SeqCst);
            }
            Ok(Event::Activity(Activity { user, .. })) => {
                connections.send_to_user(&user, "notify_activity").await;
            }
            Ok(Event::Notification(Notification { user, .. })) => {
                connections.send_to_user(&user, "notify_notification").await;
            }
            Ok(Event::PreAuth(PreAuth { user, token })) => {
                pre_auth.insert(token, (Instant::now(), user));
            }
            Err(e) => log::warn!("{:#}", e),
        }
    }
    Ok(())
}
