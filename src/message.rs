use parse_display::Display;
use rand::{thread_rng, Rng};
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
    file_held: bool,
    activity_held: bool,
    notification_held: bool,
}

impl Default for DebounceMap {
    fn default() -> Self {
        let past = Instant::now() - Duration::from_secs(600);
        DebounceMap {
            file: past,
            activity: past,
            notification: past,
            file_held: false,
            activity_held: false,
            notification_held: false,
        }
    }
}

impl DebounceMap {
    /// Check if the debounce time has passed and set the last send time if so
    pub fn should_send(&mut self, ty: &MessageType) -> bool {
        if DEBOUNCE_ENABLE.load(Ordering::Relaxed) {
            let last_send = self.get_last_send(ty);
            if Instant::now().duration_since(last_send) > Self::debounce_time(ty) {
                self.set_last_send(ty);
                self.set_held(ty, false);
                true
            } else if Instant::now().duration_since(last_send) > Duration::from_millis(100) {
                self.set_held(ty, true);
                false
            } else {
                false
            }
        } else {
            true
        }
    }

    pub fn has_held_message(&self) -> bool {
        self.file_held || self.activity_held || self.notification_held
    }

    pub fn get_held_messages(&self) -> impl Iterator<Item = MessageType> {
        let file_opt = self.file_held.then(|| MessageType::File);
        let activity_opt = self.activity_held.then(|| MessageType::Activity);
        let notification_opt = self.notification_held.then(|| MessageType::Notification);
        file_opt
            .into_iter()
            .chain(activity_opt.into_iter())
            .chain(notification_opt.into_iter())
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
        // apply a randomized offset to the last_send
        // this helps mitigate against load bursts from many clients receiving the same updates
        let spread = Duration::from_millis(thread_rng().gen_range(0..1000));
        match ty {
            MessageType::File => self.file = Instant::now() - spread,
            MessageType::Activity => self.activity = Instant::now() - spread,
            MessageType::Notification => self.notification = Instant::now() - spread,
            MessageType::Custom(..) => {} // no debouncing for custom messages
        }
    }

    fn set_held(&mut self, ty: &MessageType, held: bool) {
        match ty {
            MessageType::File => self.file_held = held,
            MessageType::Activity => self.activity_held = held,
            MessageType::Notification => self.notification_held = held,
            MessageType::Custom(..) => {} // no debouncing for custom messages
        }
    }

    fn debounce_time(ty: &MessageType) -> Duration {
        match ty {
            MessageType::File => Duration::from_secs(60),
            MessageType::Activity => Duration::from_secs(120),
            MessageType::Notification => Duration::from_secs(30),
            MessageType::Custom(..) => Duration::from_millis(1), // no debouncing for custom messages
        }
    }
}
