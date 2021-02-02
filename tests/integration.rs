use dashmap::DashMap;
use futures::future::select;
use futures::{pin_mut, FutureExt};
use futures::{SinkExt, StreamExt};
use http_auth_basic::Credentials;
use notify_push::config::Config;
use notify_push::{listen, serve, App};
use redis::AsyncCommands;
use smallvec::alloc::sync::Arc;
use sqlx::AnyPool;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU16, Ordering};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::spawn;
use tokio::time::timeout;
use tokio::time::{delay_for, Duration};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use warp::http::StatusCode;
use warp::{Filter, Reply};

static LAST_PORT: AtomicU16 = AtomicU16::new(1024);

async fn listen_available_port() -> Option<TcpListener> {
    let start = LAST_PORT.load(Ordering::SeqCst) + 1;
    for port in start..65535 {
        LAST_PORT.store(port, Ordering::SeqCst);
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(tcp) => return Some(tcp),
            _ => {}
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

impl Services {
    pub async fn new() -> Self {
        let _ = pretty_env_logger::try_init();
        let redis_tcp = listen_available_port()
            .await
            .expect("Can't find open port for redis");
        let mut nextcloud_tcp = listen_available_port()
            .await
            .expect("Can't find open port for nextcloud mock");

        let redis_addr = redis_tcp
            .local_addr()
            .expect("Failed to get redis socket address");
        let nextcloud_addr = nextcloud_tcp
            .local_addr()
            .expect("Failed to get nextcloud mock socket address");

        let db = AnyPool::connect("sqlite::memory:?cache=shared")
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
                    nextcloud_tcp.incoming(),
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
            database_url: "sqlite::memory:?cache=shared".to_string(),
            database_prefix: "oc_".to_string(),
            redis_url: format!("redis://{}", self.redis.to_string()),
            nextcloud_url: format!("http://{}/", self.nextcloud),
        }
    }

    async fn app(&self) -> App {
        let config = self.config();
        App::with_connection(self.db.clone(), config).await.unwrap()
    }

    async fn spawn_server(&self) -> ServerHandle {
        let app = Arc::new(self.app().await);
        let port = async {
            let tcp = listen_available_port().await.unwrap();
            tcp.local_addr().unwrap().port()
        }
        .await;

        let (tx, rx) = oneshot::channel();

        spawn(async move {
            let serve = serve(app.clone(), port);
            let listen = listen(app.clone());
            pin_mut!(serve);
            pin_mut!(listen);
            select(select(serve, listen), rx).await;
        });

        delay_for(Duration::from_millis(10)).await;

        ServerHandle { _handle: tx, port }
    }

    async fn redis_client(&self) -> redis::aio::Connection {
        let client = redis::Client::open(self.config().redis_url).unwrap();
        client.get_async_connection().await.unwrap()
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
    _handle: oneshot::Sender<()>,
    port: u16,
}

impl ServerHandle {
    async fn connect(&self) -> WebSocketStream<TcpStream> {
        tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{}/ws", self.port))
            .await
            .unwrap()
            .0
    }

    async fn connect_auth(&self, username: &str, password: &str) -> WebSocketStream<TcpStream> {
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

#[tokio::test(core_threads = 2)]
async fn test_self_test() {
    let services = Services::new().await;
    let app = services.app().await;
    app.self_test().await.unwrap();
}

#[tokio::test(core_threads = 2)]
async fn test_auth() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect().await;
    client.send(Message::Text("foo".into())).await.unwrap();
    client.send(Message::Text("bar".into())).await.unwrap();

    assert_next_message(&mut client, "authenticated").await;
}

#[tokio::test(core_threads = 2)]
async fn test_auth_failure() {
    let services = Services::new().await;
    services.add_user("foo", "bar");

    let server_handle = services.spawn_server().await;
    let mut client = server_handle.connect().await;
    client.send(Message::Text("foo".into())).await.unwrap();
    client.send(Message::Text("not_bar".into())).await.unwrap();

    assert_next_message(&mut client, "err: Invalid credentials").await;
}

#[track_caller]
async fn assert_next_message(client: &mut WebSocketStream<TcpStream>, expected: &str) {
    assert_eq!(
        timeout(Duration::from_millis(200), client.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        Message::Text(expected.to_string())
    );
}

#[track_caller]
async fn assert_no_message(client: &mut WebSocketStream<TcpStream>) {
    assert!(timeout(Duration::from_millis(10), client.next())
        .await
        .is_err());
}

#[tokio::test(core_threads = 2)]
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
}

#[tokio::test(core_threads = 2)]
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

#[tokio::test(core_threads = 2)]
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
            r#"{"storage":10, "path":"foo/bar"}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client, "notify_file").await;
}

#[tokio::test(core_threads = 2)]
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
            r#"{"storage":11, "path":"foo/bar"}"#,
        )
        .await
        .unwrap();

    assert_no_message(&mut client).await;
}

#[tokio::test(core_threads = 2)]
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
            r#"{"storage":10, "path":"foo/bar"}"#,
        )
        .await
        .unwrap();

    assert_next_message(&mut client1, "notify_file").await;
    assert_next_message(&mut client2, "notify_file").await;
    assert_no_message(&mut client3).await;
}

#[tokio::test(core_threads = 2)]
async fn test_pre_auth() {
    let services = Services::new().await;

    let server_handle = services.spawn_server().await;

    let mut redis = services.redis_client().await;
    redis
        .publish::<_, _, ()>("notify_pre_auth", r#"{"user":"foo", "token": "token"}"#)
        .await
        .unwrap();

    let mut client = server_handle.connect_auth("", "token").await;

    // verify that we are the correct user
    redis
        .publish::<_, _, ()>("notify_activity", r#"{"user":"foo"}"#)
        .await
        .unwrap();

    assert_next_message(&mut client, "notify_activity").await;
}
