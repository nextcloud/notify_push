/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::passthru_hasher::PassthruHasher;
use ahash::RandomState;
use dashmap::DashMap;
use log::LevelFilter;
use once_cell::race::OnceBox;
use once_cell::sync::Lazy;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer};
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Type};
use std::fmt;
use std::hash::{BuildHasher, Hasher};

static USER_NAMES: Lazy<DashMap<u64, String, PassthruHasher>> = Lazy::new(DashMap::default);

// Use the same hash state for generating user hash for every instance
static RANDOM_STATE: OnceBox<RandomState> = OnceBox::new();

#[derive(Clone, Eq, PartialEq, Hash)]
pub struct UserId {
    hash: u64,
}

impl UserId {
    pub fn new(user_id: &str) -> Self {
        let state = RANDOM_STATE.get_or_init(|| Box::new(RandomState::new()));
        let mut hash = state.build_hasher();
        hash.write(user_id.as_bytes());
        let hash = hash.finish();

        if log::max_level() >= LevelFilter::Info {
            USER_NAMES
                .entry(hash)
                .or_insert_with(|| user_id.to_string());
        }

        UserId { hash }
    }
}

impl<'de> Deserialize<'de> for UserId {
    fn deserialize<D>(deserializer: D) -> Result<UserId, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct UserIdVisitor;

        impl Visitor<'_> for UserIdVisitor {
            type Value = UserId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(v.into())
            }
        }

        deserializer.deserialize_str(UserIdVisitor)
    }
}

impl From<String> for UserId {
    fn from(id: String) -> Self {
        UserId::new(&id)
    }
}

impl From<&str> for UserId {
    fn from(id: &str) -> Self {
        UserId::new(id)
    }
}

impl<'r, DB: Database> Decode<'r, DB> for UserId
where
    &'r str: Decode<'r, DB>,
{
    fn decode(value: DB::ValueRef<'r>) -> Result<Self, BoxDynError> {
        <&str as Decode<DB>>::decode(value).map(UserId::new)
    }
}

impl<DB: Database> Type<DB> for UserId
where
    String: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <String as Type<DB>>::type_info()
    }

    fn compatible(ty: &<DB as Database>::TypeInfo) -> bool {
        <String as Type<DB>>::compatible(ty)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if log::max_level() >= LevelFilter::Info {
            if let Some(user_name) = USER_NAMES.get(&self.hash) {
                f.write_str(user_name.value())
            } else {
                f.write_str("unknown user")
            }
        } else {
            f.write_str("unknown user (Set log level to INFO or higher)")
        }
    }
}

impl fmt::Debug for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if log::max_level() >= LevelFilter::Info {
            if let Some(user_name) = USER_NAMES.get(&self.hash) {
                write!(f, "{}(#{})", user_name.value(), self.hash)
            } else {
                write!(f, "user #{}", self.hash)
            }
        } else {
            write!(f, "user #{} (Set log level to INFO or higher)", self.hash)
        }
    }
}
