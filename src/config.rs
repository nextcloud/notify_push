use color_eyre::{eyre::WrapErr, Result};
use std::net::IpAddr;
use std::str::FromStr;

pub struct Config {
    pub database_url: String,
    pub database_prefix: String,
    pub redis_url: String,
    pub nextcloud_url: String,
    pub trusted_proxies: Vec<IpAddr>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenv::dotenv();

        let database_url = std::env::var("DATABASE_URL").wrap_err("`DATABASE_URL` not set")?;
        let database_prefix =
            std::env::var("DATABASE_PREFIX").unwrap_or_else(|_| "oc_".to_string());
        let redis_url = std::env::var("REDIS_URL").wrap_err("`REDIS_URL` not set")?;
        let nextcloud_url = std::env::var("NEXTCLOUD_URL").wrap_err("`NEXTCLOUD_URL` not set")?;
        let trusted_proxies = std::env::var("TRUSTED_PROXIES").unwrap_or_default();
        let trusted_proxies = trusted_proxies
            .split(',')
            .filter(|proxy| !proxy.is_empty())
            .map(|proxy| {
                IpAddr::from_str(proxy).wrap_err_with(|| format!("Invalid ip addr: {}", proxy))
            })
            .collect::<Result<Vec<_>>>()
            .wrap_err("Invalid `TRUSTED_PROXIES`")?;

        Ok(Config {
            database_url,
            database_prefix,
            redis_url,
            nextcloud_url,
            trusted_proxies,
        })
    }
}
