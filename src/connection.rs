use crate::message::MessageType;
use crate::UserId;
use ahash::RandomState;
use dashmap::DashMap;
use std::sync::atomic::AtomicUsize;
use tokio::sync::broadcast::{channel, Receiver, Sender};

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, Sender<MessageType>, RandomState>);

pub static CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MESSAGES_SEND: AtomicUsize = AtomicUsize::new(0);

impl ActiveConnections {
    pub async fn add(&self, user: UserId) -> Receiver<MessageType> {
        if let Some(sender) = self.0.get(&user) {
            sender.subscribe()
        } else {
            let (tx, rx) = channel(4);
            self.0.insert(user, tx);
            rx
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: MessageType) {
        if let Some(tx) = self.0.get(user) {
            log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);

            tx.send(msg).ok();
        }
    }
}
