use crate::error::DatabaseError;
use crate::metrics::METRICS;
use crate::{Result, UserId};
use ahash::RandomState;
use dashmap::DashMap;
use rand::{thread_rng, Rng};
use sqlx::any::AnyConnectOptions;
use sqlx::{AnyPool, FromRow};
use std::time::Instant;
use tokio::time::Duration;

#[derive(Debug, Clone, FromRow)]
pub struct UserStorageAccess {
    #[sqlx(rename = "user_id")]
    user: UserId,
    #[sqlx(rename = "path")]
    root: String,
}

struct CachedAccess {
    access: Vec<UserStorageAccess>,
    valid_till: Instant,
}

impl CachedAccess {
    pub fn new(access: Vec<UserStorageAccess>, now: Instant) -> Self {
        let mut rng = thread_rng();
        Self {
            access,
            valid_till: now
                + Duration::from_millis(rng.gen_range((4 * 60 * 1000)..(5 * 60 * 1000))),
        }
    }

    #[inline]
    pub fn is_valid(&self, now: Instant) -> bool {
        self.valid_till > now
    }

    pub fn get_users_for_storage_path(&self, path: &str) -> impl Iterator<Item = UserId> {
        self.access
            .iter()
            .filter_map(|access| {
                if path.starts_with(&access.root) {
                    Some(access.user)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .into_iter()
    }
}

pub struct StorageMapping {
    cache: DashMap<i64, CachedAccess, RandomState>,
    connection: AnyPool,
    prefix: String,
}

impl StorageMapping {
    pub fn from_connection(connection: AnyPool, prefix: String) -> Self {
        Self {
            cache: Default::default(),
            connection,
            prefix,
        }
    }

    pub async fn new(options: AnyConnectOptions, prefix: String) -> Result<Self, DatabaseError> {
        let connection = AnyPool::connect_with(options)
            .await
            .map_err(DatabaseError::Connect)?;

        Ok(Self::from_connection(connection, prefix))
    }

    pub async fn get_users_for_storage_path(
        &self,
        storage: i64,
        path: &str,
    ) -> Result<impl Iterator<Item = UserId>, DatabaseError> {
        let now = Instant::now();
        if let Some(cached) = self
            .cache
            .get(&storage)
            .filter(|cached| cached.is_valid(now))
        {
            return Ok(cached.get_users_for_storage_path(path));
        }

        let users = self.load_storage_mapping(storage).await?;
        let cached = CachedAccess::new(users, now);
        let filtered = cached.get_users_for_storage_path(path);
        self.cache.insert(storage, cached);
        Ok(filtered)
    }

    /// Remove invalid cache entries
    pub fn cache_cleanup(&self) {
        let now = Instant::now();
        self.cache.retain(|_, c| c.is_valid(now));
    }

    async fn load_storage_mapping(
        &self,
        storage: i64,
    ) -> Result<Vec<UserStorageAccess>, DatabaseError> {
        log::debug!("querying storage mapping for {}", storage);
        let users = sqlx::query_as::<_, UserStorageAccess>(&format!(
            "\
                SELECT user_id, path \
                FROM {prefix}mounts \
                INNER JOIN {prefix}filecache ON root_id = fileid \
                WHERE storage_id = $1",
            prefix = self.prefix,
        ))
        .bind(storage)
        .fetch_all(&self.connection)
        .await
        .map_err(DatabaseError::Query)?;
        METRICS.add_mapping_query();

        Ok(users)
    }
}
