use crate::config::{Bind, Config, TlsConfig};
use crate::connection::{handle_user_socket, ActiveConnections};
use crate::event::{
    Activity, Custom, Event, GroupUpdate, Notification, PreAuth, ShareCreate, StorageUpdate,
};
use crate::message::MessageType;
use crate::metrics::METRICS;
use crate::redis::Redis;
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use ahash::RandomState;
use color_eyre::{eyre::WrapErr, Result};
use dashmap::DashMap;
use flexi_logger::LoggerHandle;
use futures::future::{select, Either};
use futures::StreamExt;
use futures::{pin_mut, FutureExt};
use smallvec::alloc::sync::Arc;
use sqlx::AnyPool;
use std::convert::Infallible;
use std::fs;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, oneshot};
use tokio::time::sleep;
use tokio_stream::wrappers::UnixListenerStream;
use warp::filters::addr::remote;
use warp::{Filter, Reply};
use warp_real_ip::get_forwarded_for;

pub mod config;
pub mod connection;
pub mod event;
pub mod message;
pub mod metrics;
pub mod nc;
pub mod redis;
pub mod storage_mapping;
pub mod user;

pub struct App {
    connections: ActiveConnections,
    nc_client: nc::Client,
    storage_mapping: StorageMapping,
    pre_auth: DashMap<String, (Instant, UserId), RandomState>,
    test_cookie: AtomicU32,
    redis: Redis,
    log_handle: Mutex<LoggerHandle>,
    reset_tx: broadcast::Sender<()>,
    _reset_rx: broadcast::Receiver<()>,
}

impl App {
    pub async fn new(config: Config, log_handle: LoggerHandle) -> Result<Self> {
        let connections = ActiveConnections::default();
        let nc_client = nc::Client::new(&config.nextcloud_url, config.allow_self_signed)?;
        let test_cookie = AtomicU32::new(0);

        let storage_mapping = StorageMapping::new(config.database, config.database_prefix).await?;
        let pre_auth = DashMap::default();

        let redis = Redis::new(config.redis)?;

        let (reset_tx, reset_rx) = broadcast::channel(1);

        Ok(App {
            connections,
            nc_client,
            test_cookie,
            pre_auth,
            storage_mapping,
            redis,
            log_handle: Mutex::new(log_handle),
            reset_tx,
            _reset_rx: reset_rx,
        })
    }

    pub async fn with_connection(
        connection: AnyPool,
        config: Config,
        log_handle: LoggerHandle,
        allow_self_signed: bool,
    ) -> Result<Self> {
        let connections = ActiveConnections::default();
        let nc_client = nc::Client::new(&config.nextcloud_url, allow_self_signed)?;
        let test_cookie = AtomicU32::new(0);

        let storage_mapping =
            StorageMapping::from_connection(connection, config.database_prefix).await?;
        let pre_auth = DashMap::default();

        let redis = Redis::new(config.redis)?;

        let (reset_tx, reset_rx) = broadcast::channel(1);

        Ok(App {
            connections,
            nc_client,
            test_cookie,
            pre_auth,
            storage_mapping,
            redis,
            log_handle: Mutex::new(log_handle),
            reset_tx,
            _reset_rx: reset_rx,
        })
    }

    pub async fn self_test(&self) -> Result<()> {
        let _ = self
            .storage_mapping
            .get_users_for_storage_path(1, "")
            .await
            .wrap_err("Failed to test database access")?;
        let mut redis = self
            .redis
            .connect()
            .await
            .wrap_err("Failed to connect to redis")?;
        redis
            .del("notify_push_app_version")
            .await
            .wrap_err("Failed to clear app version")?;
        self.nc_client
            .request_app_version()
            .await
            .wrap_err("Failed to request app version")?;
        match redis.get("notify_push_app_version").await {
            Ok(version) if version == env!("NOTIFY_PUSH_VERSION") => {}
            Ok(version) => {
                log::warn!(
                    "push server (version {}) is not the same version as the app (version {})",
                    env!("NOTIFY_PUSH_VERSION"),
                    version
                );
            }
            Err(_) => {}
        }

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
            Event::Custom(Custom {
                user,
                message,
                body,
            }) => {
                self.connections
                    .send_to_user(&user, MessageType::Custom(message, body))
                    .await;
            }
            Event::Config(event::Config::LogSpec(spec)) => {
                match self.log_handle.lock().await.parse_and_push_temp_spec(&spec) {
                    Ok(()) => log::info!("Set log level to {}", spec),
                    Err(e) => log::error!("Failed to set log level: {:?}", e),
                }
            }
            Event::Config(event::Config::LogRestore) => {
                self.log_handle.lock().await.pop_temp_spec();
                log::info!("Restored log level");
            }
            Event::Query(event::Query::Metrics) => match self.redis.connect().await {
                Ok(mut redis) => {
                    if let Err(e) = redis
                        .set(
                            "notify_push_metrics",
                            &serde_json::to_string(&METRICS).unwrap(),
                        )
                        .await
                    {
                        log::warn!("Failed to set metrics: {}", e);
                    }
                }
                Err(e) => log::warn!("Failed to set metrics: {}", e),
            },
            Event::Signal(event::Signal::Reset) => {
                log::info!("Stopping all open connections");
                if let Err(e) = self.reset_tx.send(()) {
                    log::warn!("Failed to send reset command to all connections: {}", e);
                }
            }
        }
    }

    pub fn reset_rx(&self) -> broadcast::Receiver<()> {
        self.reset_tx.subscribe()
    }
}

pub fn serve(
    app: Arc<App>,
    bind: Bind,
    cancel: oneshot::Receiver<()>,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = ()> + Send> {
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
            let response = match app.nc_client.get_test_cookie().await {
                Ok(cookie) => {
                    log::debug!("got remote test cookie {}", cookie);
                    cookie.to_string()
                }
                Err(e) => {
                    log::warn!("Error while trying to get cookie from Nextcloud {:#}", e);
                    format!("{:#}", e)
                }
            };

            Result::<_, Infallible>::Ok(response)
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

    let version = warp::path!("test" / "version")
        .and(warp::post())
        .and(app)
        .and_then(|app: Arc<App>| async move {
            Result::<_, Infallible>::Ok(match app.redis.connect().await {
                Ok(mut client) => {
                    client
                        .set("notify_push_version", env!("NOTIFY_PUSH_VERSION"))
                        .await
                        .ok();
                    "set"
                }
                Err(e) => {
                    log::warn!("Failed to get redis connection for version set: {:#}", e);
                    "error"
                }
            })
        });

    let routes = socket
        .or(cookie_test)
        .or(reverse_cookie_test)
        .or(mapping_test)
        .or(remote_test)
        .or(version);

    let routes = routes.clone().or(warp::path!("push" / ..).and(routes));

    serve_at(routes, bind, cancel, tls)
}

fn serve_at<F, C>(
    filter: F,
    bind: Bind,
    cancel: C,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = ()> + Send>
where
    C: Future + Send + Sync + 'static,
    F: Filter + Clone + Send + Sync + 'static,
    F::Extract: Reply,
{
    let cancel = cancel.map(|_| ());
    let server = warp::serve(filter);
    match (bind, tls) {
        (Bind::Tcp(addr), Some(tls)) => {
            let (_, server) = server
                .tls()
                .cert_path(&tls.cert)
                .key_path(&tls.key)
                .bind_with_graceful_shutdown(addr, cancel);
            Ok(Either::Left(Either::Left(server)))
        }
        (Bind::Tcp(addr), None) => {
            let (_, server) = server.bind_with_graceful_shutdown(addr, cancel);
            Ok(Either::Left(Either::Right(server)))
        }
        (Bind::Unix(socket_path, permissions), tls) => {
            if tls.is_some() {
                log::warn!("Serving with TLS over a unix socket is not supported");
            }
            fs::remove_file(&socket_path).ok();

            let listener = UnixListener::bind(&socket_path).wrap_err_with(|| {
                format!(
                    "Failed to setup socket at {}",
                    socket_path.to_string_lossy()
                )
            })?;
            fs::set_permissions(&socket_path, PermissionsExt::from_mode(permissions))?;

            let stream = UnixListenerStream::new(listener);
            Ok(Either::Right(
                server
                    .serve_incoming_with_graceful_shutdown(stream, cancel)
                    .map(move |_| {
                        fs::remove_file(&socket_path).ok();
                    }),
            ))
        }
    }
}

pub async fn listen_loop(app: Arc<App>, cancel: oneshot::Receiver<()>) {
    let loop_ = async move {
        loop {
            if let Err(e) = listen(app.clone()).await {
                eprintln!("Failed to setup redis subscription: {:#}", e);
            }
            log::warn!("Redis server disconnected, reconnecting in 1s");
            sleep(Duration::from_secs(1)).await;
        }
    };
    pin_mut!(loop_);
    select(cancel, loop_).await;
}

pub async fn listen(app: Arc<App>) -> Result<()> {
    let mut event_stream = event::subscribe(&app.redis).await?;

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
