use sqlx::database::HasValueRef;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Type};
use std::fmt;

// todo: pre-hash this and only save the hash since we never need to actually know the full user id, just match them
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct UserId {
    id: String,
}

impl From<String> for UserId {
    fn from(id: String) -> Self {
        UserId { id }
    }
}

impl From<&str> for UserId {
    fn from(id: &str) -> Self {
        UserId { id: id.to_string() }
    }
}

impl<'r, DB: Database> Decode<'r, DB> for UserId
where
    &'r str: Decode<'r, DB>,
{
    fn decode(value: <DB as HasValueRef<'r>>::ValueRef) -> Result<Self, BoxDynError> {
        <&str as Decode<DB>>::decode(value).map(UserId::from)
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
        f.write_str(&self.id)
    }
}
