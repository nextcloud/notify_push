/*
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */
use crate::connection::ConnectionOptions;
use parse_display::Display;
use serde_json::Value;
use smallvec::{smallvec, SmallVec};
use std::cmp::{max, min};
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::time::Duration;
use warp::ws::Message;

#[derive(Debug, Clone, PartialEq)]
pub enum UpdatedFiles {
    Unknown,
    Known(SmallVec<[u64; 4]>),
}

impl UpdatedFiles {
    pub fn extend(&mut self, more: &UpdatedFiles) {
        match (self, more) {
            (UpdatedFiles::Known(items), UpdatedFiles::Known(b)) => {
                for id in b {
                    if !items.contains(id) {
                        items.push(*id);
                    }
                }
            }
            (self_, _) => *self_ = UpdatedFiles::Unknown,
        }
    }
}

impl From<u64> for UpdatedFiles {
    fn from(id: u64) -> Self {
        UpdatedFiles::Known(smallvec![id])
    }
}

#[derive(Debug, Clone, Display, PartialEq)]
pub enum PushMessage {
    #[display("notify_file")]
    File(UpdatedFiles),
    #[display("notify_activity")]
    Activity,
    #[display("notify_notification")]
    Notification,
    #[display("{0}")]
    Custom(String, Box<Value>),
}

impl PushMessage {
    pub fn merge(&mut self, other: &PushMessage) {
        if let (PushMessage::File(a), PushMessage::File(b)) = (self, other) {
            a.extend(b)
        }
    }

    pub fn debounce_time(
        &self,
        connection_count: usize,
        max_debounce_time: usize,
        debounce_factor: f32,
    ) -> Duration {
        // scale the debounce time between 1s and 15s based on the number of active connections
        // this provides a decent balance between performance and load.
        // Additionally, each connection will have a random debounce_factor between 0.5 and 1.5
        // to spread out the load of notifications.
        let time = max(1, min(connection_count / 10, max_debounce_time)) as f32;
        let time = time * debounce_factor;
        match self {
            PushMessage::File(_) => Duration::from_secs_f32(time),
            PushMessage::Activity => Duration::from_secs_f32(time),
            PushMessage::Notification => Duration::from_secs(1),
            PushMessage::Custom(..) => Duration::from_millis(1), // no debouncing for custom messages
        }
    }

    pub fn into_message(self, opts: &ConnectionOptions) -> Message {
        match self {
            PushMessage::File(ids) => match ids {
                UpdatedFiles::Known(ids) if opts.listen_file_id.load(Ordering::Relaxed) => {
                    Message::text(format!(
                        "notify_file_id {}",
                        serde_json::to_string(&ids).unwrap()
                    ))
                }
                _ => Message::text(String::from("notify_file")),
            },
            PushMessage::Activity => Message::text(String::from("notify_activity")),
            PushMessage::Notification => Message::text(String::from("notify_notification")),
            PushMessage::Custom(ty, body) => Message::text({
                if *body == Value::Null {
                    ty
                } else {
                    let mut str = ty;
                    write!(&mut str, " {body}").ok();
                    str
                }
            }),
        }
    }

    pub fn message_type(&self) -> MessageType {
        match self {
            PushMessage::File(_) => MessageType::File,
            PushMessage::Activity => MessageType::Activity,
            PushMessage::Notification => MessageType::Notification,
            PushMessage::Custom(_, _) => MessageType::Custom,
        }
    }
}

pub enum MessageType {
    File,
    Activity,
    Notification,
    Custom,
}

pub static DEBOUNCE_ENABLE: AtomicBool = AtomicBool::new(true);

#[derive(Clone, Debug)]
struct SendQueueItem {
    received: Instant,
    sent: Instant,
    message: Option<PushMessage>,
}

impl Default for SendQueueItem {
    fn default() -> Self {
        SendQueueItem {
            received: Instant::now() - Duration::from_secs(120),
            sent: Instant::now() - Duration::from_secs(120),
            message: None,
        }
    }
}

/// Queue for sending outgoing messages to a user for debounce
///
/// The server maintains once queue per connection
#[derive(Debug)]
pub struct SendQueue {
    max_debounce_time: usize,
    debounce_factor: f32,
    items: [SendQueueItem; 3],
}

impl SendQueue {
    pub fn new(max_debounce_time: usize, debounce_factor: f32) -> Self {
        SendQueue {
            max_debounce_time,
            debounce_factor,
            items: Default::default(),
        }
    }

    fn item_mut(&mut self, message: &PushMessage) -> Option<&mut SendQueueItem> {
        match message {
            PushMessage::File(_) => Some(&mut self.items[0]),
            PushMessage::Activity => Some(&mut self.items[1]),
            PushMessage::Notification => Some(&mut self.items[2]),
            PushMessage::Custom(_, _) => None,
        }
    }

    pub fn push(&mut self, message: PushMessage, time: Instant) -> Option<PushMessage> {
        if !DEBOUNCE_ENABLE.load(Ordering::Relaxed) {
            return Some(message);
        }
        let item = match self.item_mut(&message) {
            Some(item) => item,
            None => return Some(message),
        };

        match &mut item.message {
            Some(queued) => {
                queued.merge(&message);
            }
            opt => {
                *opt = Some(message);
            }
        };
        item.received = time;

        None
    }

    pub fn drain(
        &mut self,
        now: Instant,
        connection_count: usize,
    ) -> impl Iterator<Item = PushMessage> + '_ {
        let max_debounce_time = self.max_debounce_time;
        let debounce_factor = self.debounce_factor;
        self.items.iter_mut().filter_map(move |item| {
            let debounce_time = item.message.as_ref()?.debounce_time(
                connection_count,
                max_debounce_time,
                debounce_factor,
            );
            if now.duration_since(item.sent) > debounce_time {
                if now.duration_since(item.received) > Duration::from_millis(100) {
                    item.sent = now;
                    item.message.take()
                } else {
                    None
                }
            } else {
                None
            }
        })
    }
}

#[test]
fn test_send_queue_100() {
    let base_time = Instant::now();
    let mut queue = SendQueue::new(15, 1.0);
    queue.push(PushMessage::Activity, base_time);
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![1].into())),
        base_time,
    );
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![2].into())),
        base_time + Duration::from_millis(10),
    );

    // within 100ms the messages get merged
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_millis(20), 100)
            .collect::<Vec<_>>()
    );

    // after 100ms the merged messages get send
    assert_eq!(
        vec![
            PushMessage::File(UpdatedFiles::Known(vec![1, 2].into())),
            PushMessage::Activity
        ],
        queue
            .drain(base_time + Duration::from_millis(200), 100)
            .collect::<Vec<_>>()
    );

    // messages send within debounce time get held back
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![3].into())),
        base_time + Duration::from_secs(5),
    );
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![4].into())),
        base_time + Duration::from_secs(6),
    );
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_secs(10), 100)
            .collect::<Vec<_>>()
    );

    // after debounce time we get the merged messages from the timeframe
    assert_eq!(
        vec![PushMessage::File(UpdatedFiles::Known(vec![3, 4].into()))],
        queue
            .drain(base_time + Duration::from_secs(70), 100)
            .collect::<Vec<_>>()
    );

    // nothing left
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_secs(300), 100)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_send_queue_1() {
    let base_time = Instant::now();
    let mut queue = SendQueue::new(15, 1.0);
    queue.push(PushMessage::Activity, base_time);
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![1].into())),
        base_time,
    );
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![2].into())),
        base_time + Duration::from_millis(10),
    );

    // within 100ms the messages get merged
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_millis(20), 1)
            .collect::<Vec<_>>()
    );

    // after 100ms the merged messages get send
    assert_eq!(
        vec![
            PushMessage::File(UpdatedFiles::Known(vec![1, 2].into())),
            PushMessage::Activity
        ],
        queue
            .drain(base_time + Duration::from_millis(200), 1)
            .collect::<Vec<_>>()
    );

    // messages send within debounce time get held back
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![3].into())),
        base_time + Duration::from_secs_f32(1.2),
    );
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![4].into())),
        base_time + Duration::from_secs_f32(1.3),
    );
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_secs(1), 1)
            .collect::<Vec<_>>()
    );

    // after debounce time we get the merged messages from the timeframe
    assert_eq!(
        vec![PushMessage::File(UpdatedFiles::Known(vec![3, 4].into()))],
        queue
            .drain(base_time + Duration::from_secs(3), 1)
            .collect::<Vec<_>>()
    );

    // nothing left
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_secs(5), 1)
            .collect::<Vec<_>>()
    );
}
