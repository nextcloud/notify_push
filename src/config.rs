use color_eyre::{eyre::WrapErr, Result};

pub struct Config {
    pub database_url: String,
    pub database_prefix: String,
    pub redis_url: String,
    pub nextcloud_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let _ = dotenv::dotenv();

        let database_url = std::env::var("DATABASE_URL").wrap_err("`DATABASE_URL` not set")?;
        let database_prefix =
            std::env::var("DATABASE_PREFIX").unwrap_or_else(|_| "oc_".to_string());
        let redis_url = std::env::var("REDIS_URL").wrap_err("`REDIS_URL` not set")?;
        let nextcloud_url = std::env::var("NEXTCLOUD_URL").wrap_err("`NEXTCLOUD_URL` not set")?;

        Ok(Config {
            database_url,
            database_prefix,
            redis_url,
            nextcloud_url,
        })
    }
}
