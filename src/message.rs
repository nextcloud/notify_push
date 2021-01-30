use parse_display::Display;
use std::time::Instant;
use tokio::time::Duration;

#[derive(Debug, Display, Clone)]
pub enum MessageType {
    #[display("notify_file")]
    File,
    #[display("notify_activity")]
    Activity,
    #[display("notify_notification")]
    Notification,
    #[display("{0}")]
    Custom(String),
}

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
        let last_send = self.get_last_send(ty);
        if Instant::now().duration_since(last_send) > Self::get_debounce_time(ty) {
            self.set_last_send(&ty);
            true
        } else {
            false
        }
    }

    fn get_last_send(&self, ty: &MessageType) -> Instant {
        match ty {
            MessageType::File => self.file,
            MessageType::Activity => self.activity,
            MessageType::Notification => self.notification,
            MessageType::Custom(_) => Instant::now() - Duration::from_secs(600), // no debouncing for custom messages
        }
    }

    fn set_last_send(&mut self, ty: &MessageType) {
        match ty {
            MessageType::File => self.file = Instant::now(),
            MessageType::Activity => self.activity = Instant::now(),
            MessageType::Notification => self.notification = Instant::now(),
            MessageType::Custom(_) => {} // no debouncing for custom messages
        }
    }

    const fn get_debounce_time(ty: &MessageType) -> Duration {
        match ty {
            MessageType::File => Duration::from_secs(5),
            MessageType::Activity => Duration::from_secs(15),
            MessageType::Notification => Duration::from_secs(1),
            MessageType::Custom(_) => Duration::from_millis(1), // no debouncing for custom messages
        }
    }
}
