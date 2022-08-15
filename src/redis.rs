use crate::error::ConfigError;
use crate::Result;
use redis::aio::{Connection, PubSub};
use redis::cluster::{ClusterClient, ClusterConnection};
use redis::{AsyncCommands, Client, Commands, ConnectionInfo, RedisError};
use tokio::task::block_in_place;

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
        Ok(client.get_async_connection().await?.into_pubsub())
    }

    pub async fn connect(&self) -> Result<RedisConnection, RedisError> {
        let connection = match self.config.as_slice() {
            [single] => {
                let client = Client::open(single.clone())?.get_async_connection().await?;
                RedisConnection::Async(client)
            }
            config => {
                let client =
                    block_in_place(|| ClusterClient::open(config.to_vec())?.get_connection())?;
                RedisConnection::Cluster(client)
            }
        };
        Ok(connection)
    }
}

pub enum RedisConnection {
    Async(Connection),
    Cluster(ClusterConnection),
}

impl RedisConnection {
    pub async fn del(&mut self, key: &str) -> Result<(), redis::RedisError> {
        match self {
            RedisConnection::Async(client) => {
                client.del(key).await?;
            }
            RedisConnection::Cluster(client) => {
                block_in_place(|| client.del(key))?;
            }
        }
        Ok(())
    }

    pub async fn get(&mut self, key: &str) -> Result<String> {
        Ok(match self {
            RedisConnection::Async(client) => client.get(key).await?,
            RedisConnection::Cluster(client) => block_in_place(|| client.get(key))?,
        })
    }

    pub async fn lpush(&mut self, key: &str, value: &str) -> Result<()> {
        match self {
            RedisConnection::Async(client) => {
                client.lpush(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                block_in_place(|| client.lpush(key, value))?;
            }
        }
        Ok(())
    }

    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match self {
            RedisConnection::Async(client) => {
                client.set(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                block_in_place(|| client.set(key, value))?;
            }
        }
        Ok(())
    }
}
