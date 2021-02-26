mod nc;

use crate::config::nc::parse_config_file;
use color_eyre::eyre::ContextCompat;
use color_eyre::{eyre::WrapErr, Report, Result};
use redis::ConnectionInfo;
use sqlx::any::AnyConnectOptions;
use std::convert::{TryFrom, TryInto};
use std::env::var;
use std::fmt::{Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "notify_push")]
pub struct Opt {
    /// The database connect url
    #[structopt(long)]
    pub database_url: Option<AnyConnectOptions>,
    /// The redis connect url
    #[structopt(long)]
    pub redis_url: Option<ConnectionInfo>,
    /// The table prefix for Nextcloud's database tables
    #[structopt(long)]
    pub database_prefix: Option<String>,
    /// The url the push server can access the nextcloud instance on
    #[structopt(long)]
    pub nextcloud_url: Option<String>,
    /// The port to serve the push server on
    #[structopt(short, long)]
    pub port: Option<u16>,
    /// The port to serve metrics on
    #[structopt(short = "m", long)]
    pub metrics_port: Option<u16>,
    /// The ip address to bind to
    #[structopt(long)]
    pub bind: Option<IpAddr>,
    /// Listen to a unix socket instead of TCP
    #[structopt(long)]
    pub socket_path: Option<PathBuf>,
    /// Disable validating of certificates when connecting to the nextcloud instance
    #[structopt(long)]
    pub allow_self_signed: bool,
    /// The path to the nextcloud config file
    #[structopt(name = "CONFIG_FILE", parse(from_os_str))]
    pub config_file: Option<PathBuf>,
    /// Print the binary version and exit
    #[structopt(long)]
    pub version: bool,
    /// The log level
    #[structopt(long)]
    pub log_level: Option<String>,
}

#[derive(Debug)]
pub struct Config {
    pub database: AnyConnectOptions,
    pub database_prefix: String,
    pub redis: ConnectionInfo,
    pub nextcloud_url: String,
    pub metrics_port: Option<u16>,
    pub log_level: String,
    pub bind: Bind,
    pub allow_self_signed: bool,
}

#[derive(Debug, Clone)]
pub enum Bind {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl Display for Bind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Bind::Tcp(addr) => addr.fmt(f),
            Bind::Unix(path) => path.to_string_lossy().fmt(f),
        }
    }
}

impl TryFrom<PartialConfig> for Config {
    type Error = Report;

    fn try_from(config: PartialConfig) -> Result<Self> {
        let bind = match config.socket {
            Some(socket) => Bind::Unix(socket),
            None => {
                let ip = config
                    .bind
                    .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)));
                let port = config.port.unwrap_or(7867);
                Bind::Tcp((ip, port).into())
            }
        };

        Ok(Config {
            database: config
                .database
                .ok_or_else(|| Report::msg("No database url configured"))?,
            database_prefix: config
                .database_prefix
                .unwrap_or_else(|| String::from("oc_")),
            redis: config
                .redis
                .ok_or_else(|| Report::msg("No redis url configured"))?,
            nextcloud_url: config
                .nextcloud_url
                .ok_or_else(|| Report::msg("No nextcloud url configured"))?,
            metrics_port: config.metrics_port,
            log_level: config.log_level.unwrap_or_else(|| String::from("warn")),
            bind,
            allow_self_signed: config.allow_self_signed.unwrap_or(false),
        })
    }
}

impl Config {
    pub fn from_opt(opt: Opt) -> Result<Self> {
        let from_config = opt
            .config_file
            .as_ref()
            .map(PartialConfig::from_file)
            .transpose()?
            .unwrap_or_default();
        let from_env = PartialConfig::from_env()?;
        let from_opt = PartialConfig::from_opt(opt);

        from_opt.merge(from_env).merge(from_config).try_into()
    }
}

#[derive(Debug, Default)]
struct PartialConfig {
    pub database: Option<AnyConnectOptions>,
    pub database_prefix: Option<String>,
    pub redis: Option<ConnectionInfo>,
    pub nextcloud_url: Option<String>,
    pub port: Option<u16>,
    pub metrics_port: Option<u16>,
    pub log_level: Option<String>,
    pub bind: Option<IpAddr>,
    pub socket: Option<PathBuf>,
    pub allow_self_signed: Option<bool>,
}

impl PartialConfig {
    fn from_env() -> Result<Self> {
        let database = parse_var("DATABASE_URL").wrap_err("Failed to parse DATABASE_URL")?;
        let database_prefix = var("DATABASE_PREFIX").ok();
        let redis = parse_var("REDIS_URL").wrap_err("Failed to parse REDIS_URL")?;
        let nextcloud_url = var("NEXTCLOUD_URL").ok();
        let port = parse_var("PORT").ok().wrap_err("Invalid PORT")?;
        let metrics_port = parse_var("METRICS_PORT").wrap_err("Invalid METRICS_PORT")?;
        let log_level = var("LOG").ok();
        let bind = parse_var("BIND").wrap_err("Invalid BIND")?;
        let socket = var("SOCKET_PATH").map(PathBuf::from).ok();
        let allow_self_signed = var("ALLOW_SELF_SIGNED").map(|val| val == "true").ok();

        Ok(PartialConfig {
            database,
            database_prefix,
            redis,
            nextcloud_url,
            port,
            metrics_port,
            log_level,
            bind,
            socket,
            allow_self_signed,
        })
    }

    fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        parse_config_file(file)
    }

    fn from_opt(opt: Opt) -> Self {
        PartialConfig {
            database: opt.database_url,
            database_prefix: opt.database_prefix,
            redis: opt.redis_url,
            nextcloud_url: opt.nextcloud_url,
            port: opt.port,
            metrics_port: opt.metrics_port,
            log_level: opt.log_level,
            bind: opt.bind,
            socket: opt.socket_path,
            allow_self_signed: if opt.allow_self_signed {
                Some(true)
            } else {
                None
            },
        }
    }

    fn merge(self, fallback: Self) -> Self {
        PartialConfig {
            database: self.database.or(fallback.database),
            database_prefix: self.database_prefix.or(fallback.database_prefix),
            redis: self.redis.or(fallback.redis),
            nextcloud_url: self.nextcloud_url.or(fallback.nextcloud_url),
            port: self.port.or(fallback.port),
            metrics_port: self.metrics_port.or(fallback.metrics_port),
            log_level: self.log_level.or(fallback.log_level),
            bind: self.bind.or(fallback.bind),
            socket: self.socket.or(fallback.socket),
            allow_self_signed: self.allow_self_signed.or(fallback.allow_self_signed),
        }
    }
}

fn parse_var<T>(name: &str) -> Result<Option<T>>
where
    T: FromStr + 'static,
    T::Err: std::error::Error + Sync + Send,
{
    var(name)
        .ok()
        .map(|val| T::from_str(&val))
        .transpose()
        .map_err(Report::from)
}
