use color_eyre::{Report, Result};
use redis::aio::{Connection, PubSub};
use redis::cluster::{ClusterClient, ClusterConnection};
use redis::{AsyncCommands, Client, Commands, ConnectionInfo};
use tokio::task::block_in_place;

pub struct Redis {
    config: Vec<ConnectionInfo>,
}

impl Redis {
    pub fn new(config: Vec<ConnectionInfo>) -> Result<Redis> {
        if config.is_empty() {
            return Err(Report::msg("No redis server configured"));
        }
        Ok(Redis { config })
    }

    /// Get an async pubsub connection
    pub async fn pubsub(&self) -> Result<PubSub> {
        // since pubsub performs a multicast for all nodes in a cluster,
        // listening to a single server in the cluster is sufficient for cluster setups
        let client = Client::open(self.config.first().unwrap().clone())?;
        Ok(client.get_async_connection().await?.into_pubsub())
    }

    pub async fn connect(&self) -> Result<RedisConnection> {
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
    pub async fn del(&mut self, key: &str) -> Result<()> {
        match self {
            RedisConnection::Async(client) => {
                client.del::<_, ()>(key).await?;
            }
            RedisConnection::Cluster(client) => {
                block_in_place(|| client.del::<_, ()>(key))?;
            }
        }
        Ok(())
    }

    pub async fn get(&mut self, key: &str) -> Result<String> {
        Ok(match self {
            RedisConnection::Async(client) => client.get::<_, String>(key).await?,
            RedisConnection::Cluster(client) => block_in_place(|| client.get::<_, String>(key))?,
        })
    }

    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match self {
            RedisConnection::Async(client) => {
                client.set::<_, _, ()>(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                block_in_place(|| client.set::<_, _, ()>(key, value))?;
            }
        }
        Ok(())
    }
}
