use crate::config::Config;
use crate::connection::{handle_user_socket, ActiveConnections};
use crate::event::{
    Activity, Custom, Event, GroupUpdate, Notification, PreAuth, ShareCreate, StorageUpdate,
};
use crate::message::MessageType;
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use ahash::RandomState;
use color_eyre::{eyre::WrapErr, Result};
use dashmap::DashMap;
use futures::StreamExt;
use smallvec::alloc::sync::Arc;
use sqlx::AnyPool;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use warp::filters::addr::remote;
use warp::Filter;
use warp_real_ip::get_forwarded_for;

pub mod config;
pub mod connection;
pub mod event;
pub mod message;
pub mod metrics;
pub mod nc;
pub mod storage_mapping;
pub mod user;

pub struct App {
    connections: ActiveConnections,
    nc_client: nc::Client,
    storage_mapping: StorageMapping,
    pre_auth: DashMap<String, (Instant, UserId), RandomState>,
    test_cookie: AtomicU32,
    redis_url: String,
}

impl App {
    pub async fn new(config: Config) -> Result<Self> {
        let connections = ActiveConnections::default();
        let nc_client = nc::Client::new(&config.nextcloud_url)?;
        let test_cookie = AtomicU32::new(0);

        let storage_mapping =
            StorageMapping::new(&config.database_url, config.database_prefix).await?;
        let pre_auth = DashMap::default();

        let redis_url = config.redis_url;

        Ok(App {
            connections,
            nc_client,
            test_cookie,
            pre_auth,
            storage_mapping,
            redis_url,
        })
    }

    pub async fn with_connection(connection: AnyPool, config: Config) -> Result<Self> {
        let connections = ActiveConnections::default();
        let nc_client = nc::Client::new(&config.nextcloud_url)?;
        let test_cookie = AtomicU32::new(0);

        let storage_mapping =
            StorageMapping::from_connection(connection, config.database_prefix).await?;
        let pre_auth = DashMap::default();

        let redis_url = config.redis_url;

        Ok(App {
            connections,
            nc_client,
            test_cookie,
            pre_auth,
            storage_mapping,
            redis_url,
        })
    }

    pub async fn self_test(&self) -> Result<()> {
        let _ = self
            .storage_mapping
            .get_users_for_storage_path(1, "")
            .await
            .wrap_err("Failed to test database access")?;
        Ok(())
    }

    async fn handle_event(&self, event: Event) {
        match event {
            Event::StorageUpdate(StorageUpdate { storage, path }) => {
                match self
                    .storage_mapping
                    .get_users_for_storage_path(storage, &path)
                    .await
                {
                    Ok(users) => {
                        for user in users {
                            self.connections
                                .send_to_user(&user, MessageType::File)
                                .await;
                        }
                    }
                    Err(e) => log::error!("{:#}", e),
                }
            }
            Event::GroupUpdate(GroupUpdate { user, .. }) => {
                self.connections
                    .send_to_user(&user, MessageType::File)
                    .await;
            }
            Event::ShareCreate(ShareCreate { user }) => {
                self.connections
                    .send_to_user(&user, MessageType::File)
                    .await;
            }
            Event::TestCookie(cookie) => {
                self.test_cookie.store(cookie, Ordering::SeqCst);
            }
            Event::Activity(Activity { user }) => {
                self.connections
                    .send_to_user(&user, MessageType::Activity)
                    .await;
            }
            Event::Notification(Notification { user }) => {
                self.connections
                    .send_to_user(&user, MessageType::Notification)
                    .await;
            }
            Event::PreAuth(PreAuth { user, token }) => {
                self.pre_auth.insert(token, (Instant::now(), user));
            }
            Event::Custom(Custom { user, message }) => {
                self.connections
                    .send_to_user(&user, MessageType::Custom(message))
                    .await;
            }
        }
    }
}

pub async fn serve(app: Arc<App>, port: u16) {
    let app = warp::any().map(move || app.clone());

    let cors = warp::cors().allow_any_origin();

    // GET /ws -> websocket upgrade
    let socket = warp::path!("ws")
        // The `ws()` filter will prepare Websocket handshake...
        .and(warp::ws())
        .and(app.clone())
        .and(remote())
        .and(get_forwarded_for())
        .map(
            |ws: warp::ws::Ws, app, remote: Option<SocketAddr>, mut forwarded_for: Vec<IpAddr>| {
                if let Some(remote) = remote {
                    forwarded_for.push(remote.ip());
                }
                log::debug!("new websocket connection from {:?}", forwarded_for.first());
                ws.on_upgrade(move |socket| handle_user_socket(socket, app, forwarded_for))
            },
        )
        .with(cors);

    let cookie_test = warp::path!("test" / "cookie")
        .and(app.clone())
        .map(|app: Arc<App>| {
            let cookie = app.test_cookie.load(Ordering::SeqCst);
            log::debug!("current test cookie is {}", cookie);
            cookie.to_string()
        });

    let reverse_cookie_test = warp::path!("test" / "reverse_cookie")
        .and(app.clone())
        .and_then(|app: Arc<App>| async move {
            let cookie = app.nc_client.get_test_cookie().await.unwrap_or(0);
            log::debug!("got remote test cookie {}", cookie);
            Result::<_, Infallible>::Ok(cookie.to_string())
        });

    let mapping_test = warp::path!("test" / "mapping" / u32)
        .and(app.clone())
        .and_then(|storage_id: u32, app: Arc<App>| async move {
            let access = app
                .storage_mapping
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
        });

    let remote_test = warp::path!("test" / "remote" / IpAddr)
        .and(app.clone())
        .and_then(|remote: IpAddr, app: Arc<App>| async move {
            let result = app
                .nc_client
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

    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}

pub async fn listen(app: Arc<App>) -> Result<()> {
    let client = redis::Client::open(app.redis_url.clone())?;
    let mut event_stream = event::subscribe(client).await?;

    let handle = move |event: Event| {
        // todo: any way to do this without cloning the arc every event (scoped?)
        let app = app.clone();
        async move {
            app.handle_event(event).await;
        }
    };

    while let Some(event) = event_stream.next().await {
        match event {
            Ok(event) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received {}",
                    event
                );
                tokio::spawn(handle(event));
            }
            Err(e) => log::warn!("{:#}", e),
        }
    }
    Ok(())
}
