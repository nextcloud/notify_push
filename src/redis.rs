/*
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */
use crate::error::ConfigError;
use crate::Result;
use nextcloud_config_parser::{
    RedisClusterConnectionInfo, RedisConfig, RedisConnectionAddr, RedisTlsParams,
};
use redis::aio::{MultiplexedConnection, PubSub};
use redis::cluster::ClusterClient;
use redis::cluster_async::ClusterConnection;
use redis::{
    AsyncCommands, Client, ClientTlsConfig, ConnectionAddr, ConnectionInfo, RedisConnectionInfo,
    RedisError, TlsCertificates,
};
use std::fs::read;

pub struct Redis {
    config: RedisConfig,
}

impl Redis {
    pub fn new(config: RedisConfig) -> Result<Redis> {
        if config.is_empty() {
            return Err(ConfigError::NoRedis.into());
        }
        Ok(Redis { config })
    }

    /// Get an async pubsub connection
    pub async fn pubsub(&self) -> Result<PubSub, RedisError> {
        // since pubsub performs a multicast for all nodes in a cluster,
        // listening to a single server in the cluster is sufficient for cluster setups
        let client = open_single(&self.config.as_single().unwrap())?;
        client.get_async_pubsub().await
    }

    pub async fn connect(&self) -> Result<RedisConnection, RedisError> {
        let connection = match &self.config {
            RedisConfig::Single(single) => {
                let client = open_single(single)?
                    .get_multiplexed_async_connection()
                    .await?;
                RedisConnection::Single(client)
            }
            RedisConfig::Cluster(cluster) => {
                let client = open_cluster(cluster)?.get_async_connection().await?;
                RedisConnection::Cluster(client)
            }
        };
        Ok(connection)
    }
}

pub fn open_single(
    info: &nextcloud_config_parser::RedisConnectionInfo,
) -> Result<Client, RedisError> {
    let redis = RedisConnectionInfo {
        db: info.db,
        username: info.username.clone(),
        password: info.password.clone(),
        protocol: Default::default(),
    };
    let connection_info = build_connection_info(info.addr.clone(), redis, info.tls_params.as_ref());
    Ok(match info.tls_params.as_ref() {
        None => Client::open(connection_info)?,
        Some(tls_params) => {
            // the redis library doesn't let us set both `danger_accept_invalid_hostnames` and certificates without this mess:
            // `Client::build_with_tls` doesn't use the `danger_accept_invalid_hostnames` from the passed in info
            // so we first use it to get build the certificates then take the connection info from it so we can configure it further
            let mut connection_info =
                Client::build_with_tls(connection_info, build_tls_certificates(tls_params)?)?
                    .get_connection_info()
                    .clone();
            connection_info
                .addr
                .set_danger_accept_invalid_hostnames(tls_params.accept_invalid_hostname);
            Client::open(connection_info)?
        }
    })
}

fn build_connection_info(
    addr: RedisConnectionAddr,
    redis: RedisConnectionInfo,
    tls: Option<&RedisTlsParams>,
) -> ConnectionInfo {
    match (addr, tls) {
        (
            RedisConnectionAddr::Tcp {
                host,
                port,
                tls: false,
            },
            _,
        ) => ConnectionInfo {
            addr: ConnectionAddr::Tcp(host, port),
            redis,
        },
        (
            RedisConnectionAddr::Tcp {
                host,
                port,
                tls: true,
            },
            tls_params,
        ) => ConnectionInfo {
            addr: ConnectionAddr::TcpTls {
                host,
                port,
                insecure: tls_params.map(|tls| tls.insecure).unwrap_or_default(),
                tls_params: None,
            },
            redis,
        },
        (RedisConnectionAddr::Unix { path }, _) => ConnectionInfo {
            addr: ConnectionAddr::Unix(path),
            redis,
        },
    }
}

fn open_cluster(info: &RedisClusterConnectionInfo) -> Result<ClusterClient, RedisError> {
    let redis = RedisConnectionInfo {
        db: info.db,
        username: info.username.clone(),
        password: info.password.clone(),
        protocol: Default::default(),
    };
    let mut builder =
        ClusterClient::builder(info.addr.iter().map(|addr| {
            build_connection_info(addr.clone(), redis.clone(), info.tls_params.as_ref())
        }));
    if let Some(tls) = info.tls_params.as_ref() {
        builder = builder
            .certs(build_tls_certificates(tls)?)
            .danger_accept_invalid_hostnames(tls.accept_invalid_hostname)
    }
    builder.build()
}

fn build_tls_certificates(params: &RedisTlsParams) -> Result<TlsCertificates, std::io::Error> {
    let client_tls = match (&params.local_cert, &params.local_pk) {
        (Some(cert_path), Some(pk_path)) => Some(ClientTlsConfig {
            client_cert: read(cert_path)?,
            client_key: read(pk_path)?,
        }),
        _ => None,
    };
    let root_cert = match &params.ca_file {
        Some(ca_path) => Some(read(ca_path)?),
        None => None,
    };
    Ok(TlsCertificates {
        client_tls,
        root_cert,
    })
}

pub enum RedisConnection {
    Single(MultiplexedConnection),
    Cluster(ClusterConnection),
}

impl RedisConnection {
    pub async fn del(&mut self, key: &str) -> Result<(), RedisError> {
        match self {
            RedisConnection::Single(client) => {
                client.del::<_, ()>(key).await?;
            }
            RedisConnection::Cluster(client) => {
                client.del::<_, ()>(key).await?;
            }
        }
        Ok(())
    }

    pub async fn get(&mut self, key: &str) -> Result<String> {
        Ok(match self {
            RedisConnection::Single(client) => client.get(key).await?,
            RedisConnection::Cluster(client) => client.get(key).await?,
        })
    }

    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match self {
            RedisConnection::Single(client) => {
                client.set::<_, _, ()>(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                client.set::<_, _, ()>(key, value).await?;
            }
        }
        Ok(())
    }
}
