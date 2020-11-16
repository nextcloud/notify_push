use crate::UserId;
use dashmap::DashMap;
use sqlx::{Any, AnyPool, FromRow};
use std::time::Instant;
use tokio::time::Duration;

#[derive(Debug, Clone, FromRow)]
pub struct UserStorageAccess {
    #[sqlx(rename = "user_id")]
    user: UserId,
    #[sqlx(rename = "path")]
    root: String,
}

pub struct StorageMapping {
    cache: DashMap<u32, (Instant, Vec<UserStorageAccess>)>,
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

    fn get_cached(&self, storage: u32) -> Option<Vec<UserStorageAccess>> {
        let pair = self.cache.get(&storage)?;
        let (time, cache) = pair.value();
        if time.elapsed() < Duration::from_secs(5 * 60) {
            Some(cache.clone())
        } else {
            None
        }
    }

    async fn get_access_for_storage(
        &self,
        storage: u32,
    ) -> Result<impl Iterator<Item = UserStorageAccess>, sqlx::Error> {
        Ok(if let Some(cached) = self.get_cached(storage) {
            cached.into_iter()
        } else {
            let users = sqlx::query_as::<Any, UserStorageAccess>(
                "\
                SELECT user_id, path \
                FROM oc_mounts \
                INNER JOIN oc_filecache ON root_id = fileid \
                WHERE storage_id = $1",
            )
            .bind(storage as i32)
            .fetch_all(&self.connection)
            .await?;

            self.cache.insert(storage, (Instant::now(), users.clone()));
            users.into_iter()
        })
    }

    pub async fn get_users_for_storage_path<'a>(
        &self,
        storage: u32,
        path: &'a str,
    ) -> Result<impl Iterator<Item = UserId> + 'a, sqlx::Error> {
        let access = self.get_access_for_storage(storage).await?;
        Ok(access.filter_map(move |access| {
            if path.starts_with(&access.root) {
                Some(access.user)
            } else {
                None
            }
        }))
    }
}
