use color_eyre::{Report, Result};
use redis::aio::{Connection, PubSub};
use redis::cluster::{ClusterClient, ClusterConnection};
use redis::{AsyncCommands, Client, Commands, ConnectionInfo};
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

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
        let connection = if self.config.len() == 1 {
            let client = Client::open(self.config.first().unwrap().clone())?
                .get_async_connection()
                .await?;
            RedisConnection::Async(client)
        } else {
            let config = self.config.clone();
            let client = spawn_blocking(|| ClusterClient::open(config)?.get_connection()).await??;
            RedisConnection::Cluster(Arc::new(Mutex::new(client)))
        };
        Ok(connection)
    }
}

pub enum RedisConnection {
    Async(Connection),
    // pretty inefficient, but not really a problem since this del/get/set
    // is only used during self-test related operations
    Cluster(Arc<Mutex<ClusterConnection>>),
}

impl RedisConnection {
    pub async fn del(&mut self, key: &str) -> Result<()> {
        Ok(match self {
            RedisConnection::Async(client) => {
                client.del::<_, ()>(key).await?;
            }
            RedisConnection::Cluster(client) => {
                let client = client.clone();
                let key = key.to_string();
                spawn_blocking(move || client.lock().unwrap().del::<_, ()>(key)).await??;
            }
        })
    }

    pub async fn get(&mut self, key: &str) -> Result<String> {
        Ok(match self {
            RedisConnection::Async(client) => client.get::<_, String>(key).await?,
            RedisConnection::Cluster(client) => {
                let client = client.clone();
                let key = key.to_string();
                spawn_blocking(move || client.lock().unwrap().get::<_, String>(key)).await??
            }
        })
    }

    pub async fn set(&mut self, key: &str, value: &str) -> Result<()> {
        Ok(match self {
            RedisConnection::Async(client) => {
                client.set::<_, _, ()>(key, value).await?;
            }
            RedisConnection::Cluster(client) => {
                let client = client.clone();
                let key = key.to_string();
                let value = value.to_string();
                spawn_blocking(move || client.lock().unwrap().set::<_, _, ()>(key, value))
                    .await??;
            }
        })
    }
}
