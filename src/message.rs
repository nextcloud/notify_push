use crate::connection::ConnectionOptions;
use parse_display::Display;
use serde_json::Value;
use smallvec::SmallVec;
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

impl From<Option<u64>> for UpdatedFiles {
    fn from(id: Option<u64>) -> Self {
        match id {
            Some(id) => {
                let mut ids = SmallVec::new();
                ids.push(id);
                UpdatedFiles::Known(ids)
            }
            None => UpdatedFiles::Unknown,
        }
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
    Custom(String, Value),
}

impl PushMessage {
    pub fn merge(&mut self, other: &PushMessage) {
        if let (PushMessage::File(a), PushMessage::File(b)) = (self, other) {
            a.extend(b)
        }
    }

    pub fn debounce_time(&self) -> Duration {
        match self {
            PushMessage::File(_) => Duration::from_secs(60),
            PushMessage::Activity => Duration::from_secs(60),
            PushMessage::Notification => Duration::from_secs(3),
            PushMessage::Custom(..) => Duration::from_millis(1), // no debouncing for custom messages
        }
    }
}

impl PushMessage {
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
            PushMessage::Custom(ty, Value::Null) => Message::text(ty),
            PushMessage::Custom(ty, body) => Message::text({
                let mut str = ty;
                write!(&mut str, " {}", body).ok();
                str
            }),
        }
    }
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

#[derive(Default, Debug)]
pub struct SendQueue {
    items: [SendQueueItem; 3],
}

impl SendQueue {
    pub fn new() -> Self {
        SendQueue::default()
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

    pub fn drain(&mut self, now: Instant) -> impl Iterator<Item = PushMessage> + '_ {
        self.items.iter_mut().filter_map(move |item| {
            let debounce_time = item.message.as_ref()?.debounce_time();
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
fn test_send_queue() {
    let base_time = Instant::now();
    let mut queue = SendQueue::new();
    queue.push(PushMessage::Activity, base_time);
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![1].into())),
        base_time,
    );
    queue.push(
        PushMessage::File(UpdatedFiles::Known(vec![2].into())),
        base_time + Duration::from_millis(10),
    );

    // without 100ms the messages get merged
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_millis(20))
            .collect::<Vec<_>>()
    );

    // after 100ms the merged messages get send
    assert_eq!(
        vec![
            PushMessage::File(UpdatedFiles::Known(vec![1, 2].into())),
            PushMessage::Activity
        ],
        queue
            .drain(base_time + Duration::from_millis(200))
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
            .drain(base_time + Duration::from_secs(10))
            .collect::<Vec<_>>()
    );

    // after debounce time we get the merged messages from the timeframe
    assert_eq!(
        vec![PushMessage::File(UpdatedFiles::Known(vec![3, 4].into()))],
        queue
            .drain(base_time + Duration::from_secs(70))
            .collect::<Vec<_>>()
    );

    // nothing left
    assert_eq!(
        Vec::<PushMessage>::new(),
        queue
            .drain(base_time + Duration::from_secs(300))
            .collect::<Vec<_>>()
    );
}
