use std::env::VarError;

pub struct Config {
    pub database_url: String,
    pub database_prefix: String,
    pub redis_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self, VarError> {
        let _ = dotenv::dotenv();

        let database_url = std::env::var("DATABASE_URL")?;
        let database_prefix =
            std::env::var("DATABASE_PREFIX").unwrap_or_else(|_| "oc_".to_string());
        let redis_url = std::env::var("REDIS_URL")?;

        Ok(Config {
            database_url,
            database_prefix,
            redis_url,
        })
    }
}
