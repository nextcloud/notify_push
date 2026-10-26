/*
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */
use crate::config::{Bind, Config, TlsConfig};
use crate::connection::{
    handle_user_socket, ActiveConnections, ConnectionOptions, PRE_AUTH_VALIDITY,
};
pub use crate::error::Error;
use crate::error::{SelfTestError, SocketError};
use crate::event::{
    Activity, Custom, Event, GroupUpdate, Notification, PreAuth, ShareCreate, StorageUpdate,
};
use crate::extractors::{ForwardedFor, ValidatedToken};
use crate::message::{PushMessage, UpdatedFiles};
use crate::metrics::METRICS;
use crate::redis::Redis;
use crate::storage_mapping::StorageMapping;
pub use crate::user::UserId;
use ahash::RandomState;
use axum::extract::{ConnectInfo, FromRequestParts, Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::any;
use axum::{debug_handler, Router};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use dashmap::DashMap;
use flexi_logger::LoggerHandle;
use futures::future::{select, Either};
use futures::StreamExt;
use futures::{pin_mut, FutureExt};
use smallvec::alloc::sync::Arc;
use sqlx::AnyPool;
use std::fs;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use tokio::net::{TcpListener, UnixListener};
use tokio::spawn;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, oneshot};
use tokio::time::sleep;
use tower_http::cors::{Any, CorsLayer};

pub mod config;
pub mod connection;
pub mod error;
pub mod event;
mod extractors;
pub mod message;
pub mod metrics;
pub mod nc;
mod passthru_hasher;
pub mod redis;
pub mod storage_mapping;
pub mod user;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Number of outstanding pre-auth tokens after which expired tokens get cleaned up
const PRE_AUTH_CLEANUP_THRESHOLD: usize = 1024;

pub struct App {
    connections: ActiveConnections,
    nc_client: nc::Client,
    storage_mapping: StorageMapping,
    pre_auth: DashMap<String, (Instant, UserId), RandomState>,
    test_cookie: AtomicU32,
    redis: Redis,
    log_handle: Mutex<LoggerHandle>,
    reset_tx: broadcast::Sender<()>,
    max_debounce_time: usize,
    max_connection_time: usize,
    _reset_rx: broadcast::Receiver<()>,
}

impl App {
    pub async fn new(config: Config, log_handle: LoggerHandle) -> Result<Self> {
        let connections = ActiveConnections::default();
        let nc_client = nc::Client::new(
            &config.nextcloud_url,
            config.allow_self_signed,
            config.user_agent.as_deref(),
        )?;
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
            max_debounce_time: config.max_debounce_time,
            max_connection_time: config.max_connection_time,
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
        let nc_client = nc::Client::new(
            &config.nextcloud_url,
            allow_self_signed,
            config.user_agent.as_deref(),
        )?;
        let test_cookie = AtomicU32::new(0);

        let storage_mapping = StorageMapping::from_connection(connection, config.database_prefix);
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
            max_debounce_time: config.max_debounce_time,
            max_connection_time: config.max_connection_time,
            _reset_rx: reset_rx,
        })
    }

    pub async fn self_test(&self) -> Result<(), SelfTestError> {
        let _ = self
            .storage_mapping
            .get_users_for_storage_path(1, "")
            .await?;
        let mut redis = self.redis.connect().await?;
        redis.del("notify_push_app_version").await?;
        self.nc_client.request_app_version().await?;
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
            Event::StorageUpdate(StorageUpdate {
                storage,
                path,
                file_id,
            }) => {
                match self
                    .storage_mapping
                    .get_users_for_storage_path(storage, &path)
                    .await
                {
                    Ok(users) => {
                        for user in users {
                            self.connections
                                .send_to_user(&user, PushMessage::File(file_id.into()));
                        }
                    }
                    Err(e) => log::error!("{e:#}"),
                }
            }
            Event::GroupUpdate(GroupUpdate { user, .. }) => {
                self.connections
                    .send_to_user(&user, PushMessage::File(UpdatedFiles::Unknown));
            }
            Event::ShareCreate(ShareCreate { user }) => {
                self.connections
                    .send_to_user(&user, PushMessage::File(UpdatedFiles::Unknown));
            }
            Event::TestCookie(cookie) => {
                self.test_cookie.store(cookie, Ordering::SeqCst);
            }
            Event::Activity(Activity { user }) => {
                self.connections.send_to_user(&user, PushMessage::Activity);
            }
            Event::Notification(Notification { user }) => {
                self.connections
                    .send_to_user(&user, PushMessage::Notification);
            }
            Event::PreAuth(PreAuth { target, token }) => {
                // tokens are normally cleaned up when a socket authenticates, requested tokens
                // that are never used would linger, so clean up once the map starts growing
                if self.pre_auth.len() > PRE_AUTH_CLEANUP_THRESHOLD {
                    let cutoff = Instant::now() - PRE_AUTH_VALIDITY;
                    self.pre_auth.retain(|_, (time, _)| *time > cutoff);
                }
                self.pre_auth.insert(token, (Instant::now(), target.id()));
            }
            Event::Custom(Custom {
                target,
                message,
                body,
            }) => {
                self.connections
                    .send_to_user(&target.id(), PushMessage::Custom(message, body));
            }
            Event::Config(event::Config::LogSpec(spec)) => {
                match self.log_handle.lock().await.parse_and_push_temp_spec(&spec) {
                    Ok(()) => log::info!("Set log level to {spec}"),
                    Err(e) => log::error!("Failed to set log level: {e:#}"),
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
                        log::warn!("Failed to set metrics: {e:#}");
                    }
                }
                Err(e) => log::warn!("Failed to set metrics: {e:#}"),
            },
            Event::Signal(event::Signal::Reset) => {
                log::info!("Stopping all open connections");
                if let Err(e) = self.reset_tx.send(()) {
                    log::warn!("Failed to send reset command to all connections: {e:#}");
                }
            }
        }
    }

    pub fn reset_rx(&self) -> broadcast::Receiver<()> {
        self.reset_tx.subscribe()
    }
}

pub async fn serve(
    app: Arc<App>,
    bind: Bind,
    cancel: oneshot::Receiver<()>,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = std::io::Result<()>> + Send> {
    let cors = CorsLayer::new().allow_methods(Any).allow_origin(Any);

    let test_router = Router::new()
        .route("/cookie", any(test_cookie))
        .route("/reverse_cookie", any(test_reverse_cookie))
        .route("/mapping/{storage_id}", any(test_mapping))
        .route("/remote/{remote}", any(test_remote))
        .route("/version", any(test_version));

    let router = Router::new()
        .route("/ws", any(ws_handler))
        .nest("/test", test_router);

    // also match all routes with an added "push/" prefix
    let router = router
        .clone()
        .nest("/push", router)
        .layer(cors)
        .with_state(app);

    serve_at(router, bind, cancel, tls).await
}

async fn serve_at<C>(
    router: Router,
    bind: Bind,
    cancel: C,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = std::io::Result<()>> + Send>
where
    C: Future + Send + Sync + 'static,
{
    let cancel = cancel.map(|_| ());

    match (bind, tls) {
        (Bind::Tcp(addr), Some(tls)) => {
            let handle = Handle::new();

            let handle_ = handle.clone();
            spawn(async move {
                let _ = cancel.await;
                handle_.graceful_shutdown(Some(Duration::from_secs(5)));
            });

            let config = RustlsConfig::from_pem_file(&tls.cert, &tls.key).await?;
            let server = axum_server::bind_rustls(addr, config)
                .handle(handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>());
            Ok(Either::Left(Either::Left(server)))
        }
        (Bind::Tcp(addr), None) => {
            let listener = TcpListener::bind(addr).await?;
            let server = async move {
                axum::serve(
                    listener,
                    router.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(cancel)
                .await?;
                Ok(())
            };
            Ok(Either::Left(Either::Right(server)))
        }
        (Bind::Unix(socket_path, permissions), tls) => {
            if tls.is_some() {
                log::warn!("Serving with TLS over a unix socket is not supported");
            }
            fs::remove_file(&socket_path).ok();

            let listener = UnixListener::bind(&socket_path)
                .map_err(|e| SocketError::Bind(e, socket_path.to_string_lossy().to_string()))?;
            fs::set_permissions(&socket_path, PermissionsExt::from_mode(permissions))
                .map_err(SocketError::SocketPermissions)?;

            let server = async move {
                axum::serve(listener, router.into_make_service())
                    .with_graceful_shutdown(cancel)
                    .await?;
                fs::remove_file(socket_path).ok();
                Ok(())
            };

            Ok(Either::Right(server))
        }
    }
}

pub async fn listen_loop(app: Arc<App>, cancel: oneshot::Receiver<()>) {
    let loop_ = async move {
        loop {
            if let Err(e) = listen(app.clone()).await {
                log::error!("Failed to setup redis subscription: {e:#}");
            }
            log::warn!("Redis server disconnected, reconnecting in 1s");
            sleep(Duration::from_secs(1)).await;
        }
    };
    pin_mut!(loop_);
    select(cancel, loop_).await;
}

pub async fn listen(app: Arc<App>) -> Result<()> {
    let (mut pubsub_sink, mut event_stream) = event::subscribe(&app.redis).await?;

    let handle = move |event: Event| {
        // todo: any way to do this without cloning the arc every event (scoped?)
        let app = app.clone();
        async move {
            app.handle_event(event).await;
        }
    };

    let ping_handle = tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(15)).await;
            let _ = pubsub_sink.ping::<()>().await;
        }
    });

    while let Some(event) = event_stream.next().await {
        match event {
            Ok(event) => {
                log::debug!(
                    target: "notify_push::receive",
                    "Received {event}"
                );
                tokio::spawn(handle(event));
            }
            Err(e) => log::warn!("{e:#}"),
        }
    }

    ping_handle.abort();
    Ok(())
}

#[debug_handler]
async fn ws_handler(
    State(app): State<Arc<App>>,
    ws: WebSocketUpgrade,
    ForwardedFor(mut forwarded_for): ForwardedFor,
    remote: Result<
        ConnectInfo<SocketAddr>,
        <ConnectInfo<SocketAddr> as FromRequestParts<()>>::Rejection,
    >,
) -> impl IntoResponse {
    if let Ok(remote) = remote {
        forwarded_for.push(remote.ip());
    }
    log::debug!("new websocket connection from {:?}", forwarded_for.first());
    let opts = ConnectionOptions::new(app.max_debounce_time, app.max_connection_time);
    ws.on_upgrade(move |socket| handle_user_socket(socket, app, forwarded_for, opts))
}

#[debug_handler]
async fn test_cookie(
    State(app): State<Arc<App>>,
    ValidatedToken(_token): ValidatedToken,
) -> impl IntoResponse {
    let cookie = app.test_cookie.load(Ordering::SeqCst);
    log::debug!("current test cookie is {cookie}");
    cookie.to_string()
}

#[debug_handler]
async fn test_reverse_cookie(
    State(app): State<Arc<App>>,
    ValidatedToken(token): ValidatedToken,
) -> impl IntoResponse {
    let response = match app.nc_client.get_test_cookie(&token).await {
        Ok(cookie) => {
            log::debug!("got remote test cookie {cookie}");
            cookie.to_string()
        }
        Err(e) => {
            log::warn!("Error while trying to get cookie from Nextcloud {e:#}");
            format!("{e:#}")
        }
    };
    response
}

#[debug_handler]
async fn test_mapping(
    State(app): State<Arc<App>>,
    Path(storage_id): Path<u32>,
    ValidatedToken(_token): ValidatedToken,
) -> impl IntoResponse {
    let access = app
        .storage_mapping
        .get_users_for_storage_path(storage_id, "")
        .await
        .map(|access| {
            let count = access.count();
            log::debug!("storage mapping count for {storage_id} = {count}");
            count
        })
        .inspect_err(|err| {
            log::error!("error while getting mapping count for {storage_id}: {err:#}");
        })
        .unwrap_or(0);
    access.to_string()
}

#[debug_handler]
async fn test_remote(
    State(app): State<Arc<App>>,
    Path(remote): Path<IpAddr>,
    ValidatedToken(token): ValidatedToken,
) -> impl IntoResponse {
    let result = app
        .nc_client
        .test_set_remote(remote, &token)
        .await
        .map(|remote| remote.to_string());
    log::debug!("got remote {result:?} when trying to set remote {remote}");
    result
}

#[debug_handler]
async fn test_version(
    State(app): State<Arc<App>>,
    ValidatedToken(_token): ValidatedToken,
) -> impl IntoResponse {
    let version = match app.redis.connect().await {
        Ok(mut client) => {
            client
                .set("notify_push_version", env!("NOTIFY_PUSH_VERSION"))
                .await
                .ok();
            "set"
        }
        Err(e) => {
            log::warn!("Failed to get redis connection for version set: {e:#}");
            "error"
        }
    };
    version
}
