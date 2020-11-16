use dashmap::DashMap;
use sqlx::any::AnyRow;
use sqlx::{Any, AnyPool, Row};
use std::time::Instant;
use tokio::time::Duration;

pub struct StorageMapping {
    cache: DashMap<u32, (Instant, Vec<String>)>,
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

    fn get_cached(&self, storage: u32) -> Option<Vec<String>> {
        let pair = self.cache.get(&storage)?;
        let (time, cache) = pair.value();
        if time.elapsed() < Duration::from_secs(5 * 60) {
            Some(cache.clone())
        } else {
            None
        }
    }

    pub async fn get_users_for_storage(&self, storage: u32) -> Result<Vec<String>, sqlx::Error> {
        Ok(if let Some(cached) = self.get_cached(storage) {
            cached
        } else {
            let users: Vec<String> =
                sqlx::query::<Any>("SELECT DISTINCT user_id FROM oc_mounts WHERE storage_id = $1")
                    .bind(storage as i32)
                    .map(|row: AnyRow| row.get(0))
                    .fetch_all(&self.connection)
                    .await?;

            self.cache.insert(storage, (Instant::now(), users.clone()));
            users
        })
    }
}
