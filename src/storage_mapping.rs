use sqlx::any::AnyRow;
use sqlx::{Any, AnyPool, Row};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct StorageMapping {
    cache: RwLock<HashMap<u32, (Instant, Vec<String>)>>,
    connection: AnyPool,
}

impl StorageMapping {
    pub async fn new(connect: &str) -> Result<Self, sqlx::Error> {
        let connection = AnyPool::connect(connect).await?;
        Ok(StorageMapping {
            cache: Default::default(),
            connection,
        })
    }

    pub async fn get_users_for_storage(&self, storage: u32) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query::<Any>("SELECT DISTINCT user_id FROM oc_mounts WHERE storage_id = $1")
            .bind(storage as i32)
            .map(|row: AnyRow| row.get(0))
            .fetch_all(&self.connection)
            .await
    }
}
