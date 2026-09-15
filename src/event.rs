/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::metrics::METRICS;
use crate::{Redis, Result, UserId};
use parse_display::Display;
use redis::aio::PubSubSink;
use redis::Msg;
use serde::Deserialize;
use serde_json::Value;
use std::convert::TryFrom;
use std::fmt;
use thiserror::Error;
use tokio_stream::{Stream, StreamExt};

/// The recipient of a message, either a logged in user or an anonymous session
///
/// Serialized as a single field, either `{"user": "uid"}` or `{"session": "session id"}`
#[derive(Debug, Deserialize)]
pub enum Target {
    #[serde(rename = "user")]
    User(String),
    #[serde(rename = "session")]
    AnonymousSession(String),
}

impl Target {
    pub fn id(&self) -> UserId {
        match self {
            Target::User(user) => UserId::new(user),
            Target::AnonymousSession(session) => UserId::anonymous(session),
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::User(user) => write!(f, "user {user}"),
            Target::AnonymousSession(session) => write!(f, "anonymous session {session}"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StorageUpdate {
    pub storage: u32,
    pub path: String,
    pub file_id: u64,
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
    #[serde(flatten)]
    pub target: Target,
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
    #[serde(flatten)]
    pub target: Target,
    pub message: String,
    #[serde(default)]
    pub body: Box<Value>, // use `Box` to keep the size of the `Event` enum down
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
    #[display("pre_auth {0.target}")]
    PreAuth(PreAuth),
    #[display("custom notification {0.message} for {0.target}")]
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
            "notify_notification" => Ok(Event::Notification(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_pre_auth" => Ok(Event::PreAuth(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_custom" => Ok(Event::Custom(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_config" => Ok(Event::Config(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_query" => Ok(Event::Query(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            "notify_signal" => Ok(Event::Signal(serde_json::from_slice(
                msg.get_payload_bytes(),
            )?)),
            _ => Err(MessageDecodeError::UnsupportedEventType),
        }
    }
}

#[test]
fn test_decode_custom_target() {
    let user: Custom = serde_json::from_str(r#"{"user":"foo","message":"msg"}"#).unwrap();
    assert_eq!(user.target.id(), UserId::new("foo"));

    let session: Custom =
        serde_json::from_str(r#"{"session":"myapp:foo","message":"msg","body":null}"#).unwrap();
    assert_eq!(session.target.id(), UserId::anonymous("myapp:foo"));
    assert_eq!(*session.body, Value::Null);

    let with_body: Custom =
        serde_json::from_str(r#"{"session":"myapp:foo","message":"msg","body":{"a":1}}"#).unwrap();
    assert_eq!(*with_body.body, serde_json::json!({"a": 1}));

    let pre_auth: PreAuth =
        serde_json::from_str(r#"{"session":"myapp:foo","token":"tok"}"#).unwrap();
    assert_eq!(pre_auth.target.id(), UserId::anonymous("myapp:foo"));
    assert_eq!(pre_auth.token, "tok");

    assert!(serde_json::from_str::<Custom>(r#"{"message":"msg"}"#).is_err());
}

pub async fn subscribe(
    client: &Redis,
) -> Result<(
    PubSubSink,
    impl Stream<Item = Result<Event, MessageDecodeError>>,
)> {
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
        pubsub.subscribe(*channel).await?;
    }

    let (sink, stream) = pubsub.split();
    Ok((
        sink,
        stream.map(|event| {
            METRICS.add_event();
            Event::try_from(event)
        }),
    ))
}
