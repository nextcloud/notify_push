mod nc;

/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::config::nc::parse_config_file;
use crate::error::ConfigError;
use crate::{Error, Result};
use clap::builder::styling::{AnsiColor, Effects};
use clap::builder::Styles;
use clap::Parser;
use nextcloud_config_parser::{
    RedisClusterConnectionInfo, RedisConfig, RedisConnectionAddr, RedisConnectionInfo,
    RedisTlsParams,
};
use redis::{ConnectionAddr, ConnectionInfo};
use sqlx::any::AnyConnectOptions;
use sqlx::ConnectOptions;
use std::convert::{TryFrom, TryInto};
use std::env::var;
use std::fmt::{Debug, Display, Formatter};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::str::FromStr;

const REDACTED: &str = "REDACTED";

/// Parses a url given on the command line.
///
/// Both url options are taken as plain strings so that clap never has to format
/// a parse error: it puts the raw argument into its own message, password and all.
fn parse_url_option<T: FromStr>(option: &'static str, url: &str) -> Result<T, ConfigError>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    T::from_str(url).map_err(|e| ConfigError::UrlOption(option, Box::new(e)))
}

fn styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .usage(AnsiColor::Yellow.on_default() | Effects::BOLD)
        .literal(AnsiColor::Blue.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Green.on_default())
}

#[derive(Parser)]
#[command(name = "notify_push", styles = styles())]
pub struct Opt {
    /// The database connect url
    #[clap(long)]
    pub database_url: Option<String>,
    /// The redis connect url
    #[clap(long)]
    pub redis_url: Vec<String>,
    /// The client certificate to use when connecting to redis over TLS
    #[clap(long)]
    pub redis_tls_cert: Option<PathBuf>,
    /// The client key to use when connecting to redis over TLS
    #[clap(long)]
    pub redis_tls_key: Option<PathBuf>,
    /// The CA certificate to use when connecting to redis over TLS
    #[clap(long)]
    pub redis_tls_ca: Option<PathBuf>,
    /// Don't validate the server's hostname when connecting to redis over TLS
    #[clap(long)]
    pub redis_tls_dont_validate_hostname: bool,
    /// Don't validate the server's certificate when connecting to redis over TLS
    #[clap(long)]
    pub redis_tls_insecure: bool,
    /// The table prefix for Nextcloud's database tables
    #[clap(long)]
    pub database_prefix: Option<String>,
    /// The url the push server can access the nextcloud instance on
    #[clap(long)]
    pub nextcloud_url: Option<String>,
    /// The port to serve the push server on
    #[clap(short, long)]
    pub port: Option<u16>,
    /// The port to serve metrics on
    #[clap(short = 'm', long)]
    pub metrics_port: Option<u16>,
    /// The ip address to bind to
    #[clap(long)]
    pub bind: Option<IpAddr>,
    /// Listen to a unix socket instead of TCP
    #[clap(long)]
    pub socket_path: Option<PathBuf>,
    /// File permissions for
    #[clap(long)]
    pub socket_permissions: Option<String>,
    /// Listen to a unix socket instead of TCP for serving metrics
    #[clap(long)]
    pub metrics_socket_path: Option<PathBuf>,
    /// Disable validating of certificates when connecting to the nextcloud instance
    #[clap(long)]
    pub allow_self_signed: bool,
    /// The user agent to use when connecting to the nextcloud instance
    #[clap(long)]
    pub user_agent: Option<String>,
    /// The path to the nextcloud config file
    #[clap(name = "CONFIG_FILE")]
    pub config_file: Option<PathBuf>,
    /// Print the binary version and exit
    #[clap(long)]
    pub version: bool,
    /// The log level
    #[clap(long)]
    pub log_level: Option<String>,
    /// Print the parsed config and exit
    #[clap(long)]
    pub dump_config: bool,
    /// Disable ansi escape sequences in logging output
    #[clap(long)]
    pub no_ansi: bool,
    /// Load other files named *.config.php in the config folder
    #[clap(long)]
    pub glob_config: bool,
    /// TLS certificate
    #[clap(long)]
    pub tls_cert: Option<PathBuf>,
    /// TLS key
    #[clap(long)]
    pub tls_key: Option<PathBuf>,
    /// The maximum debounce time between messages, in seconds.
    #[clap(long)]
    pub max_debounce_time: Option<usize>,
    /// The maximum connection time, in seconds. Zero means unlimited.
    #[clap(long)]
    pub max_connection_time: Option<usize>,
}

pub struct Config {
    pub database: AnyConnectOptions,
    pub database_prefix: String,
    pub redis: RedisConfig,
    pub nextcloud_url: String,
    pub metrics_bind: Option<Bind>,
    pub log_level: String,
    pub bind: Bind,
    pub allow_self_signed: bool,
    pub user_agent: Option<String>,
    pub no_ansi: bool,
    pub tls: Option<TlsConfig>,
    pub max_debounce_time: usize,
    pub max_connection_time: usize,
}

/// Formats a database url without its password
struct RedactedDatabase<'a>(&'a AnyConnectOptions);

impl Debug for RedactedDatabase<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut url = self.0.to_url_lossy();
        if url.password().is_some() && url.set_password(Some(REDACTED)).is_err() {
            // never fall through to printing the url we failed to redact
            return f.write_str(REDACTED);
        }
        Display::fmt(&url, f)
    }
}

/// Formats a redis config without its password
struct RedactedRedis<'a>(&'a RedisConfig);

impl Debug for RedactedRedis<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            RedisConfig::Single(single) => f
                .debug_struct("Single")
                .field("addr", &single.addr)
                .field("db", &single.db)
                .field("username", &single.username)
                .field("password", &single.password.as_ref().map(|_| REDACTED))
                .field("tls_params", &single.tls_params)
                .finish(),
            RedisConfig::Cluster(cluster) => f
                .debug_struct("Cluster")
                .field("addr", &cluster.addr)
                .field("db", &cluster.db)
                .field("username", &cluster.username)
                .field("password", &cluster.password.as_ref().map(|_| REDACTED))
                .field("tls_params", &cluster.tls_params)
                .finish(),
        }
    }
}

impl Debug for Config {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database", &RedactedDatabase(&self.database))
            .field("database_prefix", &self.database_prefix)
            .field("redis", &RedactedRedis(&self.redis))
            .field("nextcloud_url", &self.nextcloud_url)
            .field("metrics_bind", &self.metrics_bind)
            .field("log_level", &self.log_level)
            .field("bind", &self.bind)
            .field("allow_self_signed", &self.allow_self_signed)
            .field("user_agent", &self.user_agent)
            .field("no_ansi", &self.no_ansi)
            .field("tls", &self.tls)
            .field("max_debounce_time", &self.max_debounce_time)
            .field("max_connection_time", &self.max_connection_time)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub key: PathBuf,
    pub cert: PathBuf,
}

#[derive(Clone)]
pub enum Bind {
    Tcp(SocketAddr),
    Unix(PathBuf, u32),
}

impl Debug for Bind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Bind::Tcp(addr) => f.debug_tuple("Tcp").field(addr).finish(),
            Bind::Unix(path, permissions) => f
                .debug_tuple("Unix")
                .field(path)
                .field(&format!("0{permissions:0}"))
                .finish(),
        }
    }
}

impl Display for Bind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Bind::Tcp(addr) => Display::fmt(addr, f),
            Bind::Unix(path, _) => Display::fmt(&path.display(), f),
        }
    }
}

fn default_addr() -> IpAddr {
    let v6 = IpAddr::V6(Ipv6Addr::UNSPECIFIED);
    if TcpListener::bind((v6, 0)).is_ok() {
        v6
    } else {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    }
}

impl TryFrom<PartialConfig> for Config {
    type Error = Error;

    fn try_from(config: PartialConfig) -> Result<Self> {
        let socket_permissions = config
            .socket_permissions
            .map(|perm| {
                if perm.len() != 4 && !perm.starts_with('0') {
                    return Err(ConfigError::SocketPermissions(perm, None));
                }
                u32::from_str_radix(&perm, 8)
                    .map_err(|e| ConfigError::SocketPermissions(perm, Some(e)))
            })
            .transpose()?
            .unwrap_or(0o660);
        let bind = match config.socket {
            Some(socket) => Bind::Unix(socket, socket_permissions),
            None => {
                let ip = config.bind.unwrap_or_else(default_addr);
                let port = config.port.unwrap_or(7867);
                Bind::Tcp((ip, port).into())
            }
        };

        let metrics_bind = match (config.metrics_socket, config.metrics_port) {
            (Some(socket), _) => Some(Bind::Unix(socket, socket_permissions)),
            (None, Some(port)) => {
                let ip = config.bind.unwrap_or_else(default_addr);
                Some(Bind::Tcp((ip, port).into()))
            }
            _ => None,
        };

        let mut nextcloud_url = config
            .nextcloud_url
            .ok_or_else(|| ConfigError::NoNextcloud)?;
        if !nextcloud_url.ends_with('/') {
            nextcloud_url.push('/');
        }

        Ok(Config {
            database: config.database.ok_or_else(|| ConfigError::NoDatabase)?,
            database_prefix: config
                .database_prefix
                .unwrap_or_else(|| String::from("oc_")),
            redis: config.redis.ok_or(ConfigError::NoRedis)?,
            nextcloud_url,
            metrics_bind,
            log_level: config.log_level.unwrap_or_else(|| String::from("warn")),
            bind,
            allow_self_signed: config.allow_self_signed.unwrap_or(false),
            user_agent: config.user_agent,
            no_ansi: config.no_ansi.unwrap_or(false),
            tls: config.tls,
            max_debounce_time: config.max_debounce_time.unwrap_or(15),
            max_connection_time: config.max_connection_time.unwrap_or(0),
        })
    }
}

impl Config {
    pub fn from_opt(opt: Opt) -> Result<Self> {
        let from_config = opt
            .config_file
            .as_ref()
            .map(|path| PartialConfig::from_file(path, opt.glob_config))
            .transpose()?
            .unwrap_or_default();
        let from_env = PartialConfig::from_env()?;
        let from_opt = PartialConfig::from_opt(opt)?;

        from_opt.merge(from_env).merge(from_config).try_into()
    }
}

#[derive(Default)]
struct PartialConfig {
    pub database: Option<AnyConnectOptions>,
    pub database_prefix: Option<String>,
    pub redis: Option<RedisConfig>,
    pub nextcloud_url: Option<String>,
    pub port: Option<u16>,
    pub metrics_port: Option<u16>,
    pub metrics_socket: Option<PathBuf>,
    pub log_level: Option<String>,
    pub bind: Option<IpAddr>,
    pub socket: Option<PathBuf>,
    pub socket_permissions: Option<String>,
    pub allow_self_signed: Option<bool>,
    pub user_agent: Option<String>,
    pub no_ansi: Option<bool>,
    pub tls: Option<TlsConfig>,
    pub max_debounce_time: Option<usize>,
    pub max_connection_time: Option<usize>,
}

impl PartialConfig {
    fn from_env() -> Result<Self> {
        let database = parse_var("DATABASE_URL")?;
        let database_prefix = var("DATABASE_PREFIX").ok();
        let redis: Option<ConnectionInfo> = parse_var("REDIS_URL")?;
        let redis_tls_cert = parse_var("REDIS_TLS_CERT")?;
        let redis_tls_key = parse_var("REDIS_TLS_KEY")?;
        let redis_tls_ca = parse_var("REDIS_TLS_CA")?;
        let redis_tls_dont_validate_hostname: Option<u8> =
            parse_var("REDIS_TLS_DONT_VALIDATE_HOSTNAME")?;
        let redis_tls_insecure: Option<u8> = parse_var("REDIS_TLS_INSECURE")?;
        let nextcloud_url = var("NEXTCLOUD_URL").ok();
        let port = parse_var("PORT")?;
        let metrics_port = parse_var("METRICS_PORT")?;
        let metrics_socket = parse_var("METRICS_SOCKET_PATH")?;
        let log_level = var("LOG").ok();
        let bind = parse_var("BIND")?;
        let socket = var("SOCKET_PATH").map(PathBuf::from).ok();
        let socket_permissions = var("SOCKET_PERMISSIONS").ok();
        let allow_self_signed = var("ALLOW_SELF_SIGNED").map(|val| val == "true").ok();
        let user_agent = var("USER_AGENT").ok();
        let no_ansi = var("NO_ANSI").map(|val| val == "true").ok();

        let tls_cert = parse_var("TLS_CERT")?;
        let tls_key = parse_var("TLS_KEY")?;

        let tls = if let (Some(cert), Some(key)) = (tls_cert, tls_key) {
            Some(TlsConfig { cert, key })
        } else {
            None
        };
        let max_debounce_time = parse_var("MAX_DEBOUNCE_TIME")?;
        let max_connection_time = parse_var("MAX_CONNECTION_TIME")?;

        let redis = redis.map(|redis| {
            let addr = map_redis_addr(redis.addr().clone());

            let accept_invalid_hostname = redis_tls_dont_validate_hostname
                .filter(|b| *b != 0)
                .is_some();
            let redis_tls = matches!(addr, RedisConnectionAddr::Tcp { tls: true, .. });
            let redis_tls_insecure = redis_tls_insecure.filter(|b| *b != 0).is_some();

            let tls_params = redis_tls.then_some(RedisTlsParams {
                local_cert: redis_tls_cert,
                local_pk: redis_tls_key,
                ca_file: redis_tls_ca,
                accept_invalid_hostname,
                insecure: redis_tls_insecure,
            });

            RedisConfig::Single(RedisConnectionInfo {
                addr,
                db: redis.redis_settings().db(),
                username: redis.redis_settings().username().map(String::from),
                password: redis.redis_settings().password().map(String::from),
                tls_params,
            })
        });

        Ok(PartialConfig {
            database,
            database_prefix,
            redis,
            nextcloud_url,
            port,
            metrics_port,
            metrics_socket,
            log_level,
            bind,
            socket,
            socket_permissions,
            allow_self_signed,
            user_agent,
            no_ansi,
            tls,
            max_debounce_time,
            max_connection_time,
        })
    }

    fn from_file(file: impl AsRef<Path>, glob: bool) -> Result<Self> {
        Ok(parse_config_file(file, glob)?)
    }

    fn from_opt(opt: Opt) -> Result<Self> {
        let tls = if let (Some(cert), Some(key)) = (opt.tls_cert, opt.tls_key) {
            Some(TlsConfig { cert, key })
        } else {
            None
        };

        let redis_url = opt
            .redis_url
            .iter()
            .map(|url| parse_url_option::<ConnectionInfo>("redis-url", url))
            .collect::<Result<Vec<_>, _>>()?;

        let redis = match redis_url.len() {
            0 => None,
            1 => {
                let redis = redis_url.into_iter().next().unwrap();
                let addr = map_redis_addr(redis.addr().clone());

                let redis_tls = matches!(addr, RedisConnectionAddr::Tcp { tls: true, .. });

                let tls_params = redis_tls.then_some(RedisTlsParams {
                    local_cert: opt.redis_tls_cert,
                    local_pk: opt.redis_tls_key,
                    ca_file: opt.redis_tls_ca,
                    accept_invalid_hostname: opt.redis_tls_dont_validate_hostname,
                    insecure: opt.redis_tls_insecure,
                });

                Some(RedisConfig::Single(RedisConnectionInfo {
                    addr,
                    db: redis.redis_settings().db(),
                    username: redis.redis_settings().username().map(String::from),
                    password: redis.redis_settings().password().map(String::from),
                    tls_params,
                }))
            }
            _ => {
                let addr: Vec<_> = redis_url
                    .iter()
                    .map(|redis| map_redis_addr(redis.addr().clone()))
                    .collect();

                let redis_tls = matches!(
                    addr.first(),
                    Some(RedisConnectionAddr::Tcp { tls: true, .. })
                );

                let tls_params = redis_tls.then_some(RedisTlsParams {
                    local_cert: opt.redis_tls_cert,
                    local_pk: opt.redis_tls_key,
                    ca_file: opt.redis_tls_ca,
                    accept_invalid_hostname: opt.redis_tls_dont_validate_hostname,
                    insecure: opt.redis_tls_insecure,
                });

                let redis = redis_url.into_iter().next().unwrap();
                let redis = redis.redis_settings();
                Some(RedisConfig::Cluster(RedisClusterConnectionInfo {
                    addr,
                    db: redis.db(),
                    username: redis.username().map(String::from),
                    password: redis.password().map(String::from),
                    tls_params,
                }))
            }
        };

        Ok(PartialConfig {
            database: opt
                .database_url
                .as_deref()
                .map(|url| parse_url_option("database-url", url))
                .transpose()?,
            database_prefix: opt.database_prefix,
            redis,
            nextcloud_url: opt.nextcloud_url,
            port: opt.port,
            metrics_port: opt.metrics_port,
            metrics_socket: opt.metrics_socket_path,
            log_level: opt.log_level,
            bind: opt.bind,
            socket: opt.socket_path,
            socket_permissions: opt.socket_permissions,
            allow_self_signed: if opt.allow_self_signed {
                Some(true)
            } else {
                None
            },
            user_agent: opt.user_agent,
            no_ansi: if opt.no_ansi { Some(true) } else { None },
            tls,
            max_debounce_time: opt.max_debounce_time,
            max_connection_time: opt.max_connection_time,
        })
    }

    fn merge(self, fallback: Self) -> Self {
        PartialConfig {
            database: self.database.or(fallback.database),
            database_prefix: self.database_prefix.or(fallback.database_prefix),
            redis: if self.redis.is_some() {
                self.redis
            } else {
                fallback.redis
            },
            nextcloud_url: self.nextcloud_url.or(fallback.nextcloud_url),
            port: self.port.or(fallback.port),
            metrics_port: self.metrics_port.or(fallback.metrics_port),
            metrics_socket: self.metrics_socket.or(fallback.metrics_socket),
            log_level: self.log_level.or(fallback.log_level),
            bind: self.bind.or(fallback.bind),
            socket: self.socket.or(fallback.socket),
            socket_permissions: self.socket_permissions.or(fallback.socket_permissions),
            allow_self_signed: self.allow_self_signed.or(fallback.allow_self_signed),
            user_agent: self.user_agent.or(fallback.user_agent),
            no_ansi: self.no_ansi.or(fallback.no_ansi),
            tls: self.tls.or(fallback.tls),
            max_debounce_time: self.max_debounce_time.or(fallback.max_debounce_time),
            max_connection_time: self.max_connection_time.or(fallback.max_connection_time),
        }
    }
}

fn parse_var<T>(name: &'static str) -> Result<Option<T>>
where
    T: FromStr + 'static,
    T::Err: std::error::Error + Sync + Send,
{
    var(name)
        .ok()
        .map(|val| T::from_str(&val))
        .transpose()
        .map_err(|e| ConfigError::Env(name, Box::new(e)).into())
}

fn map_redis_addr(addr: ConnectionAddr) -> RedisConnectionAddr {
    match addr {
        ConnectionAddr::Tcp(host, port) => RedisConnectionAddr::Tcp {
            host,
            port,
            tls: false,
        },
        ConnectionAddr::TcpTls { host, port, .. } => RedisConnectionAddr::Tcp {
            host,
            port,
            tls: true,
        },
        ConnectionAddr::Unix(path) => RedisConnectionAddr::Unix { path },
        _ => unreachable!("unknown redis address"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(database: &str, redis_password: Option<&str>) -> Config {
        sqlx::any::install_default_drivers();
        Config {
            database: database.parse().unwrap(),
            database_prefix: "oc_".into(),
            redis: RedisConfig::Single(RedisConnectionInfo {
                addr: RedisConnectionAddr::Tcp {
                    host: "127.0.0.1".into(),
                    port: 6379,
                    tls: false,
                },
                db: 0,
                username: Some("redis_user".into()),
                password: redis_password.map(String::from),
                tls_params: None,
            }),
            nextcloud_url: "https://cloud.example.com/".into(),
            metrics_bind: None,
            log_level: "warn".into(),
            bind: Bind::Tcp(([127, 0, 0, 1], 7867).into()),
            allow_self_signed: false,
            user_agent: None,
            no_ansi: false,
            tls: None,
            max_debounce_time: 15,
            max_connection_time: 0,
        }
    }

    /// Renders an error together with its whole source chain
    fn full_error(err: &dyn std::error::Error) -> String {
        let mut rendered = err.to_string();
        let mut source = err.source();
        while let Some(inner) = source {
            rendered.push_str(" / ");
            rendered.push_str(&inner.to_string());
            source = inner.source();
        }
        rendered
    }

    #[test]
    fn test_malformed_urls_are_rejected_without_echoing_them() {
        let opt = Opt::try_parse_from([
            "notify_push",
            "--database-url",
            "postgres://ncuser:db-hunter2@host:not_a_port/nextcloud",
            "--redis-url",
            "redis://redisuser:redis-hunter2@host:not_a_port/0",
        ])
        .expect("clap has to accept the raw value and leave validation to us");

        let err = Config::from_opt(opt).expect_err("a malformed url still has to be rejected");
        let rendered = full_error(&err);
        assert!(!rendered.contains("db-hunter2"), "{rendered}");
        assert!(!rendered.contains("redis-hunter2"), "{rendered}");
    }

    #[test]
    fn test_config_debug_does_not_leak_passwords() {
        // `--dump-config` and `log::trace!` both format the whole config
        let config = test_config(
            "postgres://nextcloud:db-hunter2@localhost/nextcloud",
            Some("redis-hunter2"),
        );
        for formatted in [format!("{config:?}"), format!("{config:#?}")] {
            assert!(
                !formatted.contains("db-hunter2"),
                "database password leaked into config debug output: {formatted}"
            );
            assert!(
                !formatted.contains("redis-hunter2"),
                "redis password leaked into config debug output: {formatted}"
            );
        }
    }

    #[test]
    fn test_config_debug_keeps_non_secret_details() {
        let config = test_config("postgres://nextcloud@localhost/nextcloud", None);
        let formatted = format!("{config:#?}");
        assert!(formatted.contains("localhost"), "{formatted}");
        assert!(formatted.contains("cloud.example.com"), "{formatted}");
        assert!(formatted.contains("redis_user"), "{formatted}");
    }
}
