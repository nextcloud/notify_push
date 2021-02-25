mod nc;

use crate::config::nc::parse_config_file;
use color_eyre::eyre::ContextCompat;
use color_eyre::{eyre::WrapErr, Report, Result};
use redis::ConnectionInfo;
use sqlx::any::AnyConnectOptions;
use std::convert::{TryFrom, TryInto};
use std::env::var;
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
    pub port: u16,
    pub metrics_port: Option<u16>,
    pub log_level: String,
}

impl TryFrom<PartialConfig> for Config {
    type Error = Report;

    fn try_from(config: PartialConfig) -> Result<Self> {
        Ok(Config {
            database: config
                .database
                .ok_or(Report::msg("No database url configured"))?,
            database_prefix: config
                .database_prefix
                .unwrap_or_else(|| String::from("oc_")),
            redis: config.redis.ok_or(Report::msg("No redis url configured"))?,
            nextcloud_url: config
                .nextcloud_url
                .ok_or(Report::msg("No nextcloud url configured"))?,
            port: config.port.unwrap_or(7867),
            metrics_port: config.metrics_port,
            log_level: config.log_level.unwrap_or_else(|| String::from("warn")),
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
        let from_opt = PartialConfig::from_opt(opt)?;

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

        Ok(PartialConfig {
            database,
            database_prefix,
            redis,
            nextcloud_url,
            port,
            metrics_port,
            log_level,
        })
    }

    fn from_file(file: impl AsRef<Path>) -> Result<Self> {
        parse_config_file(file)
    }

    fn from_opt(opt: Opt) -> Result<Self> {
        let database = opt.database_url;
        let database_prefix = opt.database_prefix;
        let redis = opt.redis_url;
        let nextcloud_url = opt.nextcloud_url;
        let port = opt.port;
        let metrics_port = opt.metrics_port;
        let log_level = opt.log_level;

        Ok(PartialConfig {
            database,
            database_prefix,
            redis,
            nextcloud_url,
            port,
            metrics_port,
            log_level,
        })
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
