/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::error::DatabaseError;
use crate::metrics::METRICS;
use crate::{Result, UserId};
use ahash::RandomState;
use dashmap::mapref::one::Ref;
use dashmap::DashMap;
use log::debug;
use rand::{thread_rng, Rng};
use sqlx::any::AnyConnectOptions;
use sqlx::{query_as, Any, AnyPool, FromRow};
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
    pub fn new(access: Vec<UserStorageAccess>) -> Self {
        let mut rng = thread_rng();
        Self {
            access,
            valid_till: Instant::now()
                + Duration::from_millis(rng.gen_range((4 * 60 * 1000)..(5 * 60 * 1000))),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.valid_till > Instant::now()
    }
}

pub struct StorageMapping {
    cache: DashMap<u32, CachedAccess, RandomState>,
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

    async fn get_storage_mapping(
        &self,
        storage: u32,
    ) -> Result<Ref<'_, u32, CachedAccess>, DatabaseError> {
        if let Some(cached) = self.cache.get(&storage).filter(|cached| cached.is_valid()) {
            Ok(cached)
        } else {
            let users = self.load_storage_mapping(storage).await?;

            self.cache.insert(storage, CachedAccess::new(users));
            Ok(self.cache.get(&storage).unwrap())
        }
    }

    pub async fn get_users_for_storage_path(
        &self,
        storage: u32,
        path: &str,
    ) -> Result<impl Iterator<Item = UserId>, DatabaseError> {
        let cached = self.get_storage_mapping(storage).await?;
        Ok(cached
            .access
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

    async fn load_storage_mapping(
        &self,
        storage: u32,
    ) -> Result<Vec<UserStorageAccess>, DatabaseError> {
        debug!("querying storage mapping for {storage}");
        let users = query_as::<Any, UserStorageAccess>(&format!(
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
        .map_err(DatabaseError::Query)?;
        METRICS.add_mapping_query();

        debug!("got storage mappings for {storage}: {users:?}");

        Ok(users)
    }
}
