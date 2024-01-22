use crate::config::PartialConfig;
use crate::error::ConfigError;
use nextcloud_config_parser::{parse, parse_glob};
use sqlx_oldapi::any::AnyConnectOptions;
use std::path::Path;
use std::str::FromStr;

pub(super) fn parse_config_file(
    path: impl AsRef<Path>,
    glob: bool,
) -> Result<PartialConfig, ConfigError> {
    let config = if glob { parse_glob(path) } else { parse(path) }?;

    Ok(PartialConfig {
        database: Some(AnyConnectOptions::from_str(&config.database.url())?),
        database_prefix: Some(config.database_prefix),
        nextcloud_url: Some(config.nextcloud_url),
        redis: config.redis.into_vec(),
        ..PartialConfig::default()
    })
}
