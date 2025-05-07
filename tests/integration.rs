/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use dashmap::DashMap;
use flexi_logger::{Logger, LoggerHandle};
use futures::future::select;
use futures::{pin_mut, FutureExt};
use futures::{SinkExt, StreamExt};
use http_auth_basic::Credentials;
use nextcloud_config_parser::{RedisConfig, RedisConnectionAddr, RedisConnectionInfo};
use notify_push::config::{Bind, Config};
use notify_push::message::DEBOUNCE_ENABLE;
use notify_push::redis::open_single;
use notify_push::{listen_loop, serve, App};
use once_cell::sync::Lazy;
use redis::AsyncCommands;
use smallvec::alloc::sync::Arc;
use sqlx::AnyPool;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::spawn;
use tokio::time::timeout;
use tokio::time::{sleep, Duration};
use tokio_stream::wrappers::TcpListenerStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use warp::http::StatusCode;
use warp::{Filter, Reply};

static LAST_PORT: AtomicU16 = AtomicU16::new(1024);

async fn listen_available_port() -> Option<TcpListener> {
    for _ in LAST_PORT.load(Ordering::SeqCst)..65535 {
        let port = LAST_PORT.fetch_add(1, Ordering::SeqCst);
        if let Ok(tcp) = TcpListener::bind(("127.0.0.1", port)).await {
            return Some(tcp);
        }
    }

    None
}

struct Services {
    redis: SocketAddr,
    nextcloud: SocketAddr,
    _redis_shutdown: oneshot::Sender<()>,
    _nextcloud_shutdown: oneshot::Sender<()>,
    users: Arc<DashMap<String, String>>,
    db: AnyPool,
}

static LOG_HANDLE: Lazy<LoggerHandle> =
    Lazy::new(|| Logger::try_with_str("").unwrap().start().unwrap());

impl Services {
    pub async fn new() -> Self {
        sqlx::any::install_default_drivers();
        DEBOUNCE_ENABLE.store(false, Ordering::SeqCst);
        let redis_tcp = listen_available_port()
            .await
            .expect("Can't find open port for redis");
        let nextcloud_tcp = listen_available_port()
            .await
            .expect("Can't find open port for nextcloud mock");

        let redis_addr = redis_tcp
            .local_addr()
            .expect("Failed to get redis socket address");
        let nextcloud_addr = nextcloud_tcp
            .local_addr()
            .expect("Failed to get nextcloud mock socket address");

        // use the port in the db name to prevent collisions
        let db = AnyPool::connect(&format!(
            "sqlite:file:memory{}?mode=memory&cache=shared",
            nextcloud_addr.port()
        ))
        .await
        .expect("Failed to connect sqlite database");

        sqlx::query("CREATE TABLE oc_filecache(fileid BIGINT, path TEXT)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE INDEX fc_id ON oc_filecache (fileid)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE oc_mounts(storage_id BIGINT, root_id BIGINT, user_id TEXT)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE INDEX mount_storage ON oc_mounts (storage_id)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE INDEX mount_root ON oc_mounts (root_id)")
            .execute(&db)
            .await
            .unwrap();

        let users: Arc<DashMap<String, String>> = Arc::default();

        let users_filter = users.clone();
        let users_filter = warp::any().map(move || users_filter.clone());

        let uid = warp::any()
            .and(warp::header::<String>("authorization"))
            .and(users_filter)
            .map(|auth, users: Arc<DashMap<String, String>>| {
                let credentials = match Credentials::from_header(auth) {
                    Ok(credentials) => credentials,
                    Err(_) => return Box::new(StatusCode::BAD_REQUEST) as Box<dyn Reply>,
                };
                match users.get(&credentials.user_id) {
                    Some(pass) if pass.value() == &credentials.password => {
                        Box::new(credentials.user_id)
                    }
                    _ => Box::new(StatusCode::UNAUTHORIZED),
                }
            });

        let (redis_shutdown, redis_shutdown_rx) = oneshot::channel();
        let (nextcloud_shutdown, nextcloud_shutdown_rx) = oneshot::channel();

        spawn(async move {
            warp::serve(uid)
                .serve_incoming_with_graceful_shutdown(
                    TcpListenerStream::new(nextcloud_tcp),
                    nextcloud_shutdown_rx.map(|_| ()),
                )
                .await;
        });
        spawn(async move {
            mini_redis::server::run(redis_tcp, redis_shutdown_rx)
                .await
                .ok();
        });

        Self {
            redis: redis_addr,
            nextcloud: nextcloud_addr,
            _redis_shutdown: redis_shutdown,
            _nextcloud_shutdown: nextcloud_shutdown,
            users,
            db,
        }
    }

    fn config(&self) -> Config {
        Config {
            database: "sqlite::memory:?cache=shared".parse().unwrap(),
            database_prefix: "oc_".to_string(),
            redis: RedisConfig::Single(RedisConnectionInfo {
                addr: RedisConnectionAddr::Tcp {
                    host: self.redis.ip().to_string(),
                    port: self.redis.port(),
                    tls: false,
                },
                db: 0,
                username: None,
                password: None,
                tls_params: None,
            }),
            nextcloud_url: format!("http://{}/", self.nextcloud),
            metrics_bind: None,
            log_level: "".to_string(),
            bind: Bind::Tcp(self.nextcloud),
            allow_self_signed: false,
            no_ansi: false,
            tls: None,
            max_debounce_time: 15,
            max_connection_time: 0,
        }
    }

    async fn app(&self) -> App {
        let config = self.config();
        App::with_connection(self.db.clone(), config, LOG_HANDLE.clone(), false)
            .await
            .unwrap()
    }

    async fn spawn_server(&self) -> ServerHandle {
        let app = Arc::new(self.app().await);
        let addr = async {
            let tcp = listen_available_port().await.unwrap();
            tcp.local_addr()
        }
        .await
        .unwrap();

        let (serve_tx, serve_rx) = oneshot::channel();
        let (listen_tx, listen_rx) = oneshot::channel();

        let bind = Bind::Tcp(addr);
        spawn(async move {
            let serve = serve(app.clone(), bind, serve_rx, None, 15, 0).unwrap();
            let listen = listen_loop(app.clone(), listen_rx);

            pin_mut!(serve);
            pin_mut!(listen);

            select(serve, listen).await;
        });

        sleep(Duration::from_millis(10)).await;

        ServerHandle {
            _serve_handle: serve_tx,
            _listen_handle: listen_tx,
            port: addr.port(),
        }
    }

    async fn redis_client(&self) -> redis::aio::MultiplexedConnection {
        let client = open_single(&self.config().redis.as_single().unwrap()).unwrap();
        client.get_multiplexed_async_connection().await.unwrap()
    }

    fn add_user(&self, username: &str, password: &str) {
        self.users.insert(username.into(), password.into());
    }

    async fn add_storage_mapping(&self, username: &str, storage: u32, root: u32) {
        sqlx::query("INSERT INTO oc_mounts(storage_id, root_id, user_id) VALUES(?, ?, ?)")
            .bind(storage as i64)
            .bind(root as i64)
            .bind(username)
            .execute(&self.db)
            .await
            .unwrap();
    }

    async fn add_filecache_item(&self, fileid: u32, path: &str) {
        sqlx::query("INSERT INTO oc_filecache(fileid, path) VALUES(?, ?)")
            .bind(fileid as i64)
            .bind(path)
            .execute(&self.db)
            .await
            .unwrap();
    }
}

struct ServerHandle {
    _serve_handle: oneshot::Sender<()>,
    _listen_handle: oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    async fn connect(&self) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", self.port))
            .await
            .unwrap()
            .0
    }

    async fn connect_auth(
        &self,
        username: &str,
        password: &str,
    ) -> WebSocketStream<MaybeTlsStream<TcpStream>> {
        let mut client =
            tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", self.port))
                .await
                .unwrap()
                .0;

        client.send(Message::Text(username.into())).await.unwrap();
        client.send(Message::Text(password.into())).await.unwrap();

        assert_next_message(&mut client, "authenticated").await;

        client
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_auth() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect().await;
    client.send(Message::Text("foo".into())).await.unwrap();
    client.send(Message::Text("bar".into())).await.unwrap();

    assert_next_message(&mut client, "authenticated").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_auth_failure() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect().await;
    client.send(Message::Text("foo".into())).await.unwrap();
    client.send(Message::Text("not_bar".into())).await.unwrap();

    assert_next_message(&mut client, "err: Invalid credentials").await;
}

async fn assert_next_message(
    client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>,
    expected: &str,
) {
    sleep(Duration::from_millis(100)).await;
    assert_eq!(
        timeout(Duration::from_millis(200), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Message::Text(expected.into())
    );
}

async fn assert_no_message(client: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) {
    sleep(Duration::from_millis(5)).await;
    assert!(timeout(Duration::from_millis(10), client.next())
        .await
        .is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_activity() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect_auth("foo", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_activity", r#"{"user":"foo"}"#)
        .await
        .unwrap();

    assert_next_message(&mut client, "notify_activity").await;
    std::mem::forget(services);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_activity_other_user() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect_auth("foo", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_activity", r#"{"user":"someone_else"}"#)
        .await
        .unwrap();

    assert_no_message(&mut client).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_file() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_filecache_item(10, "foo").await;
    services.add_filecache_item(11, "foo/bar").await;
    services.add_storage_mapping("foo", 10, 11).await;

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect_auth("foo", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_storage_update",
            r#"{"storage":10, "path":"foo/bar", "file_id":5}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client, "notify_file").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_file_different_storage() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_filecache_item(10, "foo").await;
    services.add_filecache_item(11, "foo/bar").await;
    services.add_storage_mapping("foo", 10, 11).await;

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect_auth("foo", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_storage_update",
            r#"{"storage":11, "path":"foo/bar", "file_id":5}"#,
        )
        .await
        .unwrap();

    assert_no_message(&mut client).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_file_multiple() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");
    services.add_user("foo3", "bar");

    services.add_filecache_item(10, "foo").await;
    services.add_filecache_item(11, "foo/bar").await;
    services.add_filecache_item(12, "foo/outside").await;

    services.add_storage_mapping("foo", 10, 10).await;
    services.add_storage_mapping("foo2", 10, 11).await;
    services.add_storage_mapping("foo2", 10, 12).await;

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;
    let mut client3 = server_handle.connect_auth("foo3", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_storage_update",
            r#"{"storage":10, "path":"foo/bar", "file_id":5}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client1, "notify_file").await;
    assert_next_message(&mut client2, "notify_file").await;
    assert_no_message(&mut client3).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_pre_auth() {
    let services = Services::new().await;

    let server_handle = services.spawn_server().await;

    sleep(Duration::from_millis(500)).await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_pre_auth", r#"{"user":"foo", "token": "token"}"#)
        .await
        .unwrap();

    sleep(Duration::from_millis(100)).await;

    let mut client = server_handle.connect_auth("", "token").await;

    // verify that we are the correct user
    redis
        .publish::<_, _, ()>("notify_activity", r#"{"user":"foo"}"#)
        .await
        .unwrap();

    assert_next_message(&mut client, "notify_activity").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_notification() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_notification", r#"{"user":"foo"}"#)
        .await
        .unwrap();

    assert_next_message(&mut client1, "notify_notification").await;
    assert_no_message(&mut client2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_share() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_user_share_created", r#"{"user":"foo"}"#)
        .await
        .unwrap();

    assert_next_message(&mut client1, "notify_file").await;
    assert_no_message(&mut client2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_group() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_group_membership_update",
            r#"{"user":"foo", "group":"asd"}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client1, "notify_file").await;
    assert_no_message(&mut client2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_custom() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_custom",
            r#"{"user":"foo", "message":"my_custom_message"}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client1, "my_custom_message").await;
    assert_no_message(&mut client2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn test_notify_custom_body() {
    let services = Services::new().await;
    services.add_user("foo", "bar");
    services.add_user("foo2", "bar");

    let server_handle = services.spawn_server().await;
    let mut client1 = server_handle.connect_auth("foo", "bar").await;
    let mut client2 = server_handle.connect_auth("foo2", "bar").await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>(
            "notify_custom",
            r#"{"user":"foo", "message":"my_custom_message", "body": [1,2,3]}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client1, "my_custom_message [1,2,3]").await;
    assert_no_message(&mut client2).await;
}
