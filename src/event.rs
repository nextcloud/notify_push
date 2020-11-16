use redis::{Client, Msg, RedisError};
use serde::Deserialize;
use std::convert::TryFrom;
use thiserror::Error;
use tokio::stream::{Stream, StreamExt};

#[derive(Debug, Deserialize)]
pub struct StorageUpdate {
    pub storage: u32,
}

#[derive(Debug)]
pub enum Event {
    StorageUpdate(StorageUpdate),
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
            _ => Err(MessageDecodeError::UnsupportedEventType),
        }
    }
}

pub async fn subscribe(client: Client) -> Result<impl Stream<Item = Event>, RedisError> {
    let con = client.get_async_connection().await?;
    let mut pubsub = con.into_pubsub();
    pubsub.subscribe("notify_storage_update").await?;

    Ok(pubsub
        .into_on_message()
        .filter_map(|msg| Event::try_from(msg).ok()))
}
