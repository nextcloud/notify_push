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

    pub async fn get_users_for_storage_path<'a>(
        &self,
        storage: u32,
        path: &str,
    ) -> Result<impl Iterator<Item = UserId>, sqlx::Error> {
        let cached = if let Some(cached) = self.cache.get(&storage).and_then(|cached| {
            let (time, _cache) = cached.value();
            if time.elapsed() < Duration::from_secs(5 * 60) {
                Some(cached)
            } else {
                None
            }
        }) {
            cached
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

            self.cache.insert(storage, (Instant::now(), users));
            self.cache.get(&storage).unwrap()
        };
        let (_, access) = cached.value();
        Ok(access
            .iter()
            .filter_map(move |access| {
                if path.starts_with(&access.root) {
                    Some(access.user.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .into_iter())
    }
}
