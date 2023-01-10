use flexi_logger::FlexiLoggerError;
use miette::Diagnostic;
use redis::RedisError;
use reqwest::StatusCode;
use std::net::AddrParseError;
use std::num::ParseIntError;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Redis(#[from] RedisError),
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] ConfigError),
    #[error("Error while running self test: {0}")]
    #[diagnostic(transparent)]
    SelfTest(#[from] SelfTestError),
    #[error("Failed to set signal hook")]
    SignalHook(#[source] std::io::Error),
    #[error("Failed to listen to socket")]
    Socket(#[from] SocketError),
    #[error("Error while handling authentication")]
    Authentication(#[from] AuthenticationError),
    #[error("Error while communicating with Nextcloud")]
    NextCloud(#[from] NextCloudError),
}

#[derive(Debug, Error, Diagnostic)]
pub enum NextCloudError {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("Invalid nextcloud url")]
    NextcloudUrl(#[from] url::ParseError),
    #[error("Error while connecting to nextcloud")]
    NextcloudConnect(#[source] reqwest::Error),
    #[error("Client error: {0}")]
    Client(StatusCode),
    #[error("Server error: {0}")]
    Server(StatusCode),
    #[error("Unexpected status code: {0}")]
    Other(StatusCode),
    #[error("{0} is not configured as a trusted domain for the nextcloud server")]
    NotATrustedDomain(String),
    #[error("Invalid response when getting test cookie")]
    MalformedCookieResponse(#[source] ParseIntError),
    #[error("Invalid response when testing if the push server is a trusted proxy")]
    MalformedRemote(#[source] AddrParseError),
}

#[derive(Debug, Error, Diagnostic)]
pub enum DatabaseError {
    #[error("Failed to connect to database")]
    Connect(#[source] sqlx::Error),
    #[error("Failed to query database")]
    Query(#[source] sqlx::Error),
}

#[derive(Debug, Error, Diagnostic)]
pub enum SelfTestError {
    #[error("Failed to test database access")]
    Database(#[from] DatabaseError),
    #[error("Failed to test redis access")]
    Redis(#[from] RedisError),
    #[error("Error while communicating with nextcloud instance")]
    NextcloudCommunication(#[from] NextCloudError),
}

#[derive(Debug, Error, Diagnostic)]
pub enum SocketError {
    #[error("Failed to bind to socket at {1}")]
    Bind(#[source] std::io::Error, String),
    #[error("Failed to set socket permissions")]
    SocketPermissions(#[source] std::io::Error),
}

#[derive(Debug, Error, Diagnostic)]
pub enum ConfigError {
    #[error("No redis server is configured")]
    NoRedis,
    #[error("No nextcloud server is configured")]
    NoNextcloud,
    #[error("No database server is configured")]
    NoDatabase,
    #[error("Error while parsing nextcloud config.php")]
    #[diagnostic(transparent)]
    Parse(#[from] nextcloud_config_parser::Error),
    #[error("Invalid {0} environment variable")]
    Env(
        &'static str,
        #[source] Box<dyn std::error::Error + Send + Sync>,
    ),
    #[error("socket permissions should be provided in the octal form `0xxx`, got {0}")]
    SocketPermissions(String, Option<ParseIntError>),
    #[error("Failed to parse log level")]
    LogLevel(#[from] FlexiLoggerError),
}

#[derive(Debug, Error, Diagnostic)]
pub enum WebSocketError {
    #[error("Client disconnected unexpectedly")]
    Disconnected,
    #[error(transparent)]
    Error(#[from] warp::Error),
}

#[derive(Debug, Error, Diagnostic)]
pub enum AuthenticationError {
    #[error(transparent)]
    Socket(#[from] WebSocketError),
    #[error("Invalid authentication message")]
    InvalidMessage,
    #[error("Error while sending authentication request to nextcloud")]
    Nextcloud(#[from] NextCloudError),
    #[error("Invalid credentials")]
    Invalid,
    #[error("Connection limit exceeded for user")]
    LimitExceeded,
}
