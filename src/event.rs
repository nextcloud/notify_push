use crate::metrics::METRICS;
use crate::{Redis, Result, UserId};
use parse_display::Display;
use redis::Msg;
use serde::Deserialize;
use serde_json::Value;
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};

#[derive(Debug, Deserialize)]
pub struct StorageUpdate {
    pub storage: u32,
    pub path: String,
    pub file_id: Option<u64>,
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

#[derive(Debug, Deserialize)]
pub struct Notification {
    pub user: UserId,
}

#[derive(Debug, Deserialize)]
pub struct PreAuth {
    pub user: UserId,
    pub token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Config {
    LogSpec(String),
    LogRestore,
}

#[derive(Debug, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
pub enum Query {
    Metrics,
}

#[derive(Debug, Deserialize)]
pub struct Custom {
    pub user: UserId,
    pub message: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Reset,
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
    #[display("notification notification for user {0.user}")]
    Notification(Notification),
    #[display("pre_auth user {0.user}")]
    PreAuth(PreAuth),
    #[display("custom notification {0.message} for user {0.user}")]
    Custom(Custom),
    #[display("config update")]
    Config(Config),
    #[display("{0} query")]
    Query(Query),
    #[display("{0} signal")]
    Signal(Signal),
}

#[derive(Debug, Error)]
pub enum MessageDecodeError {
    #[error("unsupported event type")]
    UnsupportedEventType,
    #[error("json deserialization error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Event {
    fn parse(channel_prefix: &str, msg: Msg) -> Result<Option<Self>, MessageDecodeError> {
        let channel = if let Some(channel) = msg.get_channel_name().strip_prefix(channel_prefix) {
            channel
        } else {
            return Ok(None);
        };

        match channel {
            "notify_storage_update" => Ok(Some(Event::StorageUpdate(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_group_membership_update" => Ok(Some(Event::GroupUpdate(
                serde_json::from_slice(msg.get_payload_bytes())?,
            ))),
            "notify_user_share_created" => Ok(Some(Event::ShareCreate(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_test_cookie" => Ok(Some(Event::TestCookie(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_activity" => Ok(Some(Event::Activity(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_notification" => Ok(Some(Event::Notification(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_pre_auth" => Ok(Some(Event::PreAuth(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_custom" => Ok(Some(Event::Custom(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_config" => Ok(Some(Event::Config(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_query" => Ok(Some(Event::Query(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            "notify_signal" => Ok(Some(Event::Signal(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?))),
            _ => Err(MessageDecodeError::UnsupportedEventType),
        }
    }
}

pub async fn subscribe(
    client: &Redis,
    db: i64,
) -> Result<impl Stream<Item = Result<Event, MessageDecodeError>>> {
    let mut pubsub = client.pubsub().await?;
    let channels = [
        "notify_storage_update",
        "notify_group_membership_update",
        "notify_user_share_created",
        "notify_test_cookie",
        "notify_activity",
        "notify_notification",
        "notify_pre_auth",
        "notify_custom",
        "notify_config",
        "notify_query",
        "notify_signal",
    ];
    for channel in channels.iter() {
        pubsub.subscribe(&format!("{}_{}", db, channel)).await?;
    }

    let prefix = format!("{}_", db);

    Ok(pubsub.into_on_message().filter_map(move |event| {
        METRICS.add_event();
        Event::parse(&prefix, event).transpose()
    }))
}
