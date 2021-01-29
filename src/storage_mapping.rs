use crate::UserId;
use color_eyre::{eyre::WrapErr, Result};
use dashmap::DashMap;
use sqlx::{Any, AnyPool, FromRow};
use std::sync::atomic::{AtomicUsize, Ordering};
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
    prefix: String,
}

pub static MAPPING_QUERY_COUNT: AtomicUsize = AtomicUsize::new(0);

impl StorageMapping {
    pub async fn new(connect: &str, prefix: String) -> Result<Self> {
        let connection = AnyPool::connect(connect)
            .await
            .wrap_err("Failed to connect to Nextcloud database")?;
        Ok(StorageMapping {
            cache: Default::default(),
            connection,
            prefix,
        })
    }

    pub async fn get_users_for_storage_path<'a>(
        &self,
        storage: u32,
        path: &str,
    ) -> Result<impl Iterator<Item = UserId>> {
        let cached = if let Some(cached) = self.cache.get(&storage).and_then(|cached| {
            let (time, _cache) = cached.value();
            if time.elapsed() < Duration::from_secs(5 * 60) {
                Some(cached)
            } else {
                None
            }
        }) {
            log::trace!("using cached storage mapping for {}", storage);
            cached
        } else {
            log::debug!("querying storage mapping for {}", storage);
            let users = sqlx::query_as::<Any, UserStorageAccess>(&format!(
                "\
                SELECT user_id, path \
                FROM {prefix}mounts \
                INNER JOIN {prefix}filecache ON root_id = fileid \
                WHERE storage_id = {storage}",
                prefix = self.prefix,
                storage = storage
            ))
            .fetch_all(&self.connection)
            .await
            .wrap_err("Failed to load storage mapping from database")?;
            MAPPING_QUERY_COUNT.fetch_add(1, Ordering::SeqCst);

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
