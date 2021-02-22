use color_eyre::{eyre::WrapErr, Report, Result};
use php_literal_parser::Value;
use redis::{ConnectionAddr, ConnectionInfo};
use sqlx::any::AnyConnectOptions;
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug)]
pub struct Config {
    pub database: AnyConnectOptions,
    pub database_prefix: String,
    pub redis: ConnectionInfo,
    pub nextcloud_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let database_url = get_env("DATABASE_URL")?;
        let database_prefix = get_env("DATABASE_PREFIX").unwrap_or_else(|_| "oc_".to_string());
        let redis_url = get_env("REDIS_URL")?;
        let nextcloud_url = get_env("NEXTCLOUD_URL")?;

        Ok(Config {
            database: database_url
                .parse()
                .wrap_err("Failed to parse DATABASE_URL")?,
            database_prefix,
            redis: redis_url.parse().wrap_err("Failed to parse REDIS_URL")?,
            nextcloud_url,
        })
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("Failed to read config file {}", path))?;
        let literal = content.trim_start_matches("<?php\n$CONFIG =").to_string();
        let parsed = php_literal_parser::from_str(&literal)
            .map_err(|err| Report::msg(err.to_string()))
            .wrap_err("Failed to parse config file")?;

        let database = parse_db_options(&parsed).wrap_err("Failed to create database config")?;
        let database_prefix = parsed["dbtableprefix"].to_string();
        let nextcloud_url = parsed["overwrite.cli.url"]
            .clone()
            .into_string()
            .ok_or_else(|| Report::msg("'overwrite.cli.url' not set"))?;
        let redis = parse_redis_options(&parsed);

        // allow env overwrites
        let database = match get_env("DATABASE_URL") {
            Ok(database_url) => database_url
                .parse()
                .wrap_err("Failed to parse DATABASE_URL")?,
            _ => database,
        };
        let database_prefix = match get_env("DATABASE_PREFIX") {
            Ok(database_prefix) => database_prefix,
            _ => database_prefix,
        };
        let nextcloud_url = match get_env("NEXTCLOUD_URL") {
            Ok(nextcloud_url) => nextcloud_url,
            _ => nextcloud_url,
        };
        let redis = match get_env("REDIS_URL") {
            Ok(redis_url) => redis_url.parse().wrap_err("Failed to parse REDIS_URL")?,
            _ => redis,
        };

        Ok(Config {
            database,
            database_prefix,
            nextcloud_url,
            redis,
        })
    }
}

fn get_env(name: &str) -> Result<String> {
    std::env::var(name).wrap_err_with(|| format!("`{}` not set", name))
}

fn parse_db_options(parsed: &Value) -> Result<AnyConnectOptions> {
    match parsed["dbtype"].as_str() {
        Some("mysql") => {
            let mut options = MySqlConnectOptions::new();
            if let Some(username) = parsed["dbuser"].as_str() {
                options = options.username(username);
            }
            if let Some(password) = parsed["dbpassword"].as_str() {
                options = options.password(password);
            }
            let socket_addr = PathBuf::from("/var/run/mysqld/mysqld.sock");
            match split_host(parsed["dbhost"].as_str().unwrap_or_default()) {
                ("localhost", None, None) if socket_addr.exists() => {
                    options = options.socket(socket_addr);
                }
                (addr, None, None) => {
                    options = options.host(addr);
                }
                (addr, Some(port), None) => {
                    options = options.host(addr).port(port);
                }
                (_, None, Some(socket)) => {
                    options = options.socket(socket);
                }
                (_, Some(_), Some(_)) => {
                    unreachable!()
                }
            }
            if let Some(port) = parsed["dbport"].clone().into_int() {
                options = options.port(port as u16);
            }
            if let Some(name) = parsed["dbname"].as_str() {
                options = options.database(name);
            }
            Ok(options.into())
        }
        Some("pgsql") => {
            let mut options = PgConnectOptions::new();
            if let Some(username) = parsed["dbuser"].as_str() {
                options = options.username(username);
            }
            if let Some(password) = parsed["dbpassword"].as_str() {
                options = options.password(password);
            }
            match split_host(parsed["dbhost"].as_str().unwrap_or_default()) {
                (addr, None, None) => {
                    options = options.host(addr);
                }
                (addr, Some(port), None) => {
                    options = options.host(addr).port(port);
                }
                (_, None, Some(socket)) => {
                    options = options.socket(socket);
                }
                (_, Some(_), Some(_)) => {
                    unreachable!()
                }
            }
            if let Some(port) = parsed["dbport"].clone().into_int() {
                options = options.port(port as u16);
            }
            if let Some(name) = parsed["dbname"].as_str() {
                options = options.database(name);
            }
            Ok(options.into())
        }
        Some("sqlite3") => {
            let mut options = SqliteConnectOptions::new();
            if let Some(data_dir) = parsed["datadirectory"].as_str() {
                let db_name = parsed["dbname"]
                    .clone()
                    .into_string()
                    .unwrap_or_else(|| String::from("owncloud"));
                options = options.filename(format!("{}/{}.db", data_dir, db_name));
            }
            Ok(options.into())
        }
        _ => Err(Report::msg("Unsupported database type")),
    }
}

fn split_host(host: &str) -> (&str, Option<u16>, Option<&str>) {
    let mut parts = host.split(':');
    let host = parts.next().unwrap();
    match parts
        .next()
        .map(|port_or_socket| u16::from_str(port_or_socket).map_err(|_| port_or_socket))
    {
        Some(Ok(port)) => (host, Some(port), None),
        Some(Err(socket)) => (host, None, Some(socket)),
        None => (host, None, None),
    }
}

fn parse_redis_options(parsed: &Value) -> ConnectionInfo {
    let host = parsed["redis"]["host"].as_str().unwrap_or("127.0.0.1");
    let db = parsed["redis"]["dbindex"].clone().into_int().unwrap_or(0);
    let addr = if host.starts_with('/') {
        ConnectionAddr::Unix(host.into())
    } else {
        ConnectionAddr::Tcp(
            host.into(),
            parsed["redis"]["port"].clone().into_int().unwrap_or(6379) as u16,
        )
    };
    let passwd = parsed["redis"]["password"]
        .as_str()
        .filter(|pass| !pass.is_empty())
        .map(String::from);
    ConnectionInfo {
        addr: Box::new(addr),
        db,
        username: None,
        passwd,
    }
}

#[test]
fn test_redis_empty_password_none() {
    let config =
        php_literal_parser::from_str(r#"["redis" => ["host" => "redis", "password" => "pass"]]"#)
            .unwrap();
    let redis = parse_redis_options(&config);
    assert_eq!(redis.passwd, Some("pass".to_string()));

    let config =
        php_literal_parser::from_str(r#"["redis" => ["host" => "redis", "password" => ""]]"#)
            .unwrap();
    let redis = parse_redis_options(&config);
    assert_eq!(redis.passwd, None);
}
