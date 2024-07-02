/*
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */
 
use crate::error::ConfigError;
use crate::Result;
use redis::aio::{MultiplexedConnection, PubSub};
use redis::cluster::ClusterClient;
use redis::cluster_async::ClusterConnection;
use redis::{AsyncCommands, Client, ConnectionInfo, RedisError};

pub struct Redis {
    config: Vec<ConnectionInfo>,
}

impl Redis {
    pub fn new(config: Vec<ConnectionInfo>) -> Result<Redis> {
        if config.is_empty() {
            return Err(ConfigError::NoRedis.into());
        }
        Ok(Redis { config })
    }

    /// Get an async pubsub connection
    pub async fn pubsub(&self) -> Result<PubSub, RedisError> {
        // since pubsub performs a multicast for all nodes in a cluster,
        // listening to a single server in the cluster is sufficient for cluster setups
        let client = Client::open(self.config.first().unwrap().clone())?;
        client.get_async_pubsub().await
    }

    pub async fn connect(&self) -> Result<RedisConnection, RedisError> {
        let connection = match self.config.as_slice() {
            [single] => {
                let client = Client::open(single.clone())?
                    .get_multiplexed_async_connection()
                    .await?;
                RedisConnection::Single(client)
            }
            config => {
                let client = ClusterClient::new(config.to_vec())?
                    .get_async_connection()
                    .await?;
                RedisConnection::Cluster(client)
            }
        };
        Ok(connection)
    }
}

pub enum RedisConnection {
    Single(MultiplexedConnection),
    Cluster(ClusterConnection),
}

impl RedisConnection {
    pub async fn del(&mut self, key: &str) -> Result<(), RedisError> {
        match self {
            RedisConnection::Single(client) => {
                client.del(key).await?;
            }
            RedisConnection::Cluster(client) => {
                client.del(key).await?;
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
                client.set(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                client.set(key, value).await?;
            }
        }
        Ok(())
    }
}
