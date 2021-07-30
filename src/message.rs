use parse_display::Display;
use serde_json::Value;
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::time::Duration;
use warp::ws::Message;

#[derive(Debug, Clone, Display)]
pub enum MessageType {
    #[display("notify_file")]
    File,
    #[display("notify_activity")]
    Activity,
    #[display("notify_notification")]
    Notification,
    #[display("{0}")]
    Custom(String, Value),
}

impl From<MessageType> for Message {
    fn from(msg: MessageType) -> Self {
        match msg {
            MessageType::File => Message::text(String::from("notify_file")),
            MessageType::Activity => Message::text(String::from("notify_activity")),
            MessageType::Notification => Message::text(String::from("notify_notification")),
            MessageType::Custom(ty, Value::Null) => Message::text(ty),
            MessageType::Custom(ty, body) => Message::text({
                let mut str = ty;
                write!(&mut str, " {}", body).ok();
                str
            }),
        }
    }
}

pub static DEBOUNCE_ENABLE: AtomicBool = AtomicBool::new(true);

pub struct DebounceMap {
    file: Instant,
    activity: Instant,
    notification: Instant,
}

impl Default for DebounceMap {
    fn default() -> Self {
        let past = Instant::now() - Duration::from_secs(600);
        DebounceMap {
            file: past,
            activity: past,
            notification: past,
        }
    }
}

impl DebounceMap {
    /// Check if the debounce time has passed and set the last send time if so
    pub fn should_send(&mut self, ty: &MessageType) -> bool {
        if DEBOUNCE_ENABLE.load(Ordering::Relaxed) {
            let last_send = self.get_last_send(ty);
            if Instant::now().duration_since(last_send) > Self::get_debounce_time(ty) {
                self.set_last_send(ty);
                true
            } else {
                false
            }
        } else {
            true
        }
    }

    fn get_last_send(&self, ty: &MessageType) -> Instant {
        match ty {
            MessageType::File => self.file,
            MessageType::Activity => self.activity,
            MessageType::Notification => self.notification,
            MessageType::Custom(..) => Instant::now() - Duration::from_secs(600), // no debouncing for custom messages
        }
    }

    fn set_last_send(&mut self, ty: &MessageType) {
        match ty {
            MessageType::File => self.file = Instant::now(),
            MessageType::Activity => self.activity = Instant::now(),
            MessageType::Notification => self.notification = Instant::now(),
            MessageType::Custom(..) => {} // no debouncing for custom messages
        }
    }

    fn get_debounce_time(ty: &MessageType) -> Duration {
        match ty {
            MessageType::File => Duration::from_secs(5),
            MessageType::Activity => Duration::from_secs(15),
            MessageType::Notification => Duration::from_secs(1),
            MessageType::Custom(..) => Duration::from_millis(1), // no debouncing for custom messages
        }
    }
}
