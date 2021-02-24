use color_eyre::{eyre::WrapErr, Report, Result};
use php_literal_parser::Value;
use redis::{ConnectionAddr, ConnectionInfo};
use sqlx::any::AnyConnectOptions;
use sqlx::mysql::MySqlConnectOptions;
use sqlx::postgres::PgConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
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
        let php = match content.find("$CONFIG") {
            Some(pos) => content[pos + "$CONFIG".len()..]
                .trim()
                .trim_start_matches("="),
            None => {
                return Err(Report::msg("$CONFIG not found"))
                    .wrap_err("Failed to parse config file")
            }
        };
        let parsed = php_literal_parser::from_str(php)
            .map_err(|err| Report::msg(err.with_source(php).to_string()))
            .wrap_err("Failed to parse config file")?;

        let database = parse_db_options(&parsed).wrap_err("Failed to create database config")?;
        let database_prefix = parsed["dbtableprefix"]
            .as_str()
            .unwrap_or("oc_")
            .to_string();
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
            let socket_addr1 = PathBuf::from("/var/run/mysqld/mysqld.sock");
            let socket_addr2 = PathBuf::from("/tmp/mysql.sock");
            let socket_addr3 = PathBuf::from("/run/mysql/mysql.sock");
            match split_host(parsed["dbhost"].as_str().unwrap_or_default()) {
                ("localhost", None, None) if socket_addr1.exists() => {
                    options = options.socket(socket_addr1);
                }
                ("localhost", None, None) if socket_addr2.exists() => {
                    options = options.socket(socket_addr2);
                }
                ("localhost", None, None) if socket_addr3.exists() => {
                    options = options.socket(socket_addr3);
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
                    let mut socket_path = Path::new(socket);

                    // sqlx wants the folder the socket is in, not the socket itself
                    if socket_path
                        .file_name()
                        .map(|name| name.to_str().unwrap().starts_with(".s"))
                        .unwrap_or(false)
                    {
                        socket_path = socket_path.parent().unwrap();
                    }
                    options = options.socket(socket_path);
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
    let mut host = parsed["redis"]["host"].as_str().unwrap_or("127.0.0.1");
    if host == "localhost" {
        host = "127.0.0.1";
    }
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

#[cfg(test)]
fn assert_debug_equal<T: Debug, U: Debug>(a: T, b: U) {
    assert_eq!(format!("{:?}", a), format!("{:?}", b),);
}

#[test]
fn test_parse_config_basic() {
    let config = Config::from_file("tests/configs/basic.php").unwrap();
    assert_eq!("https://cloud.example.com", config.nextcloud_url);
    assert_eq!("oc_", config.database_prefix);
    assert_debug_equal(
        AnyConnectOptions::from_str("mysql://nextcloud:secret@127.0.0.1/nextcloud").unwrap(),
        config.database,
    );
    assert_debug_equal(
        ConnectionInfo::from_str("redis://127.0.0.1").unwrap(),
        config.redis,
    );
}

#[test]
fn test_parse_implicit_prefix() {
    let config = Config::from_file("tests/configs/implicit_prefix.php").unwrap();
    assert_eq!("oc_", config.database_prefix);
}

#[test]
fn test_parse_empty_redis_password() {
    let config = Config::from_file("tests/configs/empty_redis_password.php").unwrap();
    assert_debug_equal(
        ConnectionInfo::from_str("redis://127.0.0.1").unwrap(),
        config.redis,
    );
}

#[test]
fn test_parse_full_redis() {
    let config = Config::from_file("tests/configs/full_redis.php").unwrap();
    assert_debug_equal(
        ConnectionInfo::from_str("redis://:moresecret@redis:1234/1").unwrap(),
        config.redis,
    );
}

#[test]
fn test_parse_redis_socket() {
    let config = Config::from_file("tests/configs/redis_socket.php").unwrap();
    assert_debug_equal(
        ConnectionInfo::from_str("redis+unix:///redis").unwrap(),
        config.redis,
    );
}

#[test]
fn test_parse_comment_whitespace() {
    let config = Config::from_file("tests/configs/comment_whitespace.php").unwrap();
    assert_eq!("https://cloud.example.com", config.nextcloud_url);
    assert_eq!("oc_", config.database_prefix);
    assert_debug_equal(
        AnyConnectOptions::from_str("mysql://nextcloud:secret@127.0.0.1/nextcloud").unwrap(),
        config.database,
    );
    assert_debug_equal(
        ConnectionInfo::from_str("redis://127.0.0.1").unwrap(),
        config.redis,
    );
}

#[test]
fn test_parse_port_in_host() {
    let config = Config::from_file("tests/configs/port_in_host.php").unwrap();
    assert_debug_equal(
        AnyConnectOptions::from_str("mysql://nextcloud:secret@127.0.0.1:1234/nextcloud").unwrap(),
        config.database,
    );
}

#[test]
fn test_parse_postgres_socket() {
    let config = Config::from_file("tests/configs/postgres_socket.php").unwrap();
    assert_debug_equal(
        AnyConnectOptions::from(
            PgConnectOptions::new()
                .socket("/var/run/postgresql")
                .username("redacted")
                .password("redacted")
                .database("nextcloud"),
        ),
        config.database,
    );
}

#[test]
fn test_parse_postgres_socket_folder() {
    let config = Config::from_file("tests/configs/postgres_socket_folder.php").unwrap();
    assert_debug_equal(
        AnyConnectOptions::from(
            PgConnectOptions::new()
                .socket("/var/run/postgresql")
                .username("redacted")
                .password("redacted")
                .database("nextcloud"),
        ),
        config.database,
    );
}
