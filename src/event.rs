use crate::UserId;
use color_eyre::{eyre::WrapErr, Result};
use parse_display::Display;
use redis::{Client, Msg};
use serde::Deserialize;
use std::convert::TryFrom;
use thiserror::Error;
use tokio::stream::{Stream, StreamExt};

#[derive(Debug, Deserialize)]
pub struct StorageUpdate {
    pub storage: u32,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct GroupUpdate {
    pub user: UserId,
    pub group: String,
}

#[derive(Debug, Deserialize)]
pub struct ShareCreate {
    pub user: UserId,
}

#[derive(Debug, Deserialize)]
pub struct Activity {
    pub user: UserId,
}

#[derive(Debug, Display)]
pub enum Event {
    #[display("storage update notification for storage {0.storage} and path {0.path}")]
    StorageUpdate(StorageUpdate),
    #[display("group update notification for user {0.user}")]
    GroupUpdate(GroupUpdate),
    #[display("share create notification for user {0.user}")]
    ShareCreate(ShareCreate),
    #[display("test cookie {0}")]
    TestCookie(u32),
    #[display("activity notification for user {0.user}")]
    Activity(Activity),
}

#[derive(Debug, Error)]
pub enum MessageDecodeError {
    #[error("unsupported event type")]
    UnsupportedEventType,
    #[error("json deserialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl TryFrom<Msg> for Event {
    type Error = MessageDecodeError;

    fn try_from(msg: Msg) -> Result<Self, Self::Error> {
        match msg.get_channel_name() {
            "notify_storage_update" => Ok(Event::StorageUpdate(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_group_membership_update" => Ok(Event::GroupUpdate(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_user_share_created" => Ok(Event::ShareCreate(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_test_cookie" => Ok(Event::TestCookie(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_activity" => Ok(Event::Activity(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            _ => Err(MessageDecodeError::UnsupportedEventType),
        }
    }
}

pub async fn subscribe(
    client: Client,
) -> Result<impl Stream<Item = Result<Event, MessageDecodeError>>> {
    let con = client
        .get_async_connection()
        .await
        .wrap_err("Failed to connect to redis")?;
    let mut pubsub = con.into_pubsub();
    let channels = [
        "notify_storage_update",
        "notify_group_membership_update",
        "notify_user_share_created",
        "notify_test_cookie",
        "notify_activity",
    ];
    for channel in channels.iter() {
        pubsub
            .subscribe(*channel)
            .await
            .wrap_err("Failed to subscribe to redis pubsub")?;
    }

    Ok(pubsub.into_on_message().map(|msg| Event::try_from(msg)))
}
