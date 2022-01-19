use crate::config::PartialConfig;
use crate::error::ConfigError;
use nextcloud_config_parser::{parse, parse_glob};
use std::path::Path;

pub(super) fn parse_config_file(
    path: impl AsRef<Path>,
    glob: bool,
) -> Result<PartialConfig, ConfigError> {
    let config = if glob { parse_glob(path) } else { parse(path) }?;

    Ok(PartialConfig {
        database: Some(config.database.into()),
        database_prefix: Some(config.database_prefix),
        nextcloud_url: Some(config.nextcloud_url),
        redis: config.redis.into_vec(),
        ..PartialConfig::default()
    })
}
