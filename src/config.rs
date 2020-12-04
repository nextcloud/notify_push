use color_eyre::{eyre::WrapErr, Report, Result};
use std::net::IpAddr;
use std::str::FromStr;

#[derive(Debug)]
pub struct Config {
    pub database_url: String,
    pub database_prefix: String,
    pub redis_url: String,
    pub nextcloud_url: String,
    pub trusted_proxies: Vec<IpAddr>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = get_env("DATABASE_URL")?;
        let database_prefix = get_env("DATABASE_PREFIX").unwrap_or_else(|_| "oc_".to_string());
        let redis_url = get_env("REDIS_URL")?;
        let nextcloud_url = get_env("NEXTCLOUD_URL")?;
        let trusted_proxies = get_env("TRUSTED_PROXIES").unwrap_or_default();
        let trusted_proxies = parse_proxies(&trusted_proxies)?;

        Ok(Config {
            database_url,
            database_prefix,
            redis_url,
            nextcloud_url,
            trusted_proxies,
        })
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read config file {}", path))?;
        let literal = content.trim_start_matches("<?php\n$CONFIG =").to_string();
        let parsed = php_literal_parser::parse(&literal)
            .map_err(|err| Report::msg(err.to_string()))
            .wrap_err("Failed to parse config file")?;

        let database_url = format!(
            "{}://{}:{}@{}{}{}/{}",
            map_db_type(
                parsed["dbtype"]
                    .as_str()
                    .ok_or(Report::msg("invalid 'dbtype'"))?
            ),
            parsed["dbuser"],
            parsed["dbpassword"],
            parsed["dbhost"],
            if parsed["dbport"] != "" { ":" } else { "" },
            parsed["dbport"],
            parsed["dbname"]
        );
        let database_prefix = parsed["dbtableprefix"].to_string();
        let nextcloud_url = parsed["overwrite.cli.url"]
            .clone()
            .into_string()
            .ok_or(Report::msg("'overwrite.cli.url' not set"))?;
        let redis_url = format!(
            "redis://{}/",
            parsed["redis"]["host"].as_str().unwrap_or("127.0.0.1")
        );
        let trusted_proxies = parsed["trusted_proxies"]
            .clone()
            .into_hashmap()
            .unwrap_or_default()
            .values()
            .map(|proxy| {
                Ok(IpAddr::from_str(
                    proxy
                        .as_str()
                        .ok_or(Report::msg("invalid 'trusted_proxies' value"))?,
                )?)
            })
            .collect::<Result<Vec<_>, Report>>()?;

        // allow env overwrites

        let database_url = match get_env("DATABASE_URL") {
            Ok(database_url) => database_url,
            _ => database_url,
        };
        let database_prefix = match get_env("DATABASE_PREFIX") {
            Ok(database_prefix) => database_prefix,
            _ => database_prefix,
        };
        let nextcloud_url = match get_env("NEXTCLOUD_URL") {
            Ok(nextcloud_url) => nextcloud_url,
            _ => nextcloud_url,
        };
        let redis_url = match get_env("REDIS_URL") {
            Ok(redis_url) => redis_url,
            _ => redis_url,
        };
        let trusted_proxies = match get_env("TRUSTED_PROXIES") {
            Ok(trusted_proxies) => parse_proxies(&trusted_proxies)?,
            _ => trusted_proxies,
        };

        Ok(Config {
            database_url,
            database_prefix,
            nextcloud_url,
            redis_url,
            trusted_proxies,
        })
    }
}

fn get_env(name: &str) -> Result<String> {
    std::env::var(name).wrap_err_with(|| format!("`{}` not set", name))
}

fn map_db_type(ty: &str) -> &str {
    match ty {
        "pgsql" => "postgres",
        ty => ty,
    }
}

fn parse_proxies(proxies: &str) -> Result<Vec<IpAddr>> {
    Ok(proxies
        .split(',')
        .filter(|proxy| !proxy.is_empty())
        .map(|proxy| {
            IpAddr::from_str(proxy).wrap_err_with(|| format!("Invalid ip addr: {}", proxy))
        })
        .collect::<Result<Vec<_>>>()
        .wrap_err("Invalid `TRUSTED_PROXIES`")?)
}
