use crate::UserId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use warp::ws::Message;

type Sender = mpsc::UnboundedSender<Result<Message, warp::Error>>;

static NEXT_CONNECTION_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ConnectionId(usize);

impl ConnectionId {
    pub fn next() -> Self {
        ConnectionId(NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed))
    }
}

struct UserConnection {
    id: ConnectionId,
    sender: Sender,
}

#[derive(Default, Clone)]
pub struct ActiveConnections(Arc<RwLock<HashMap<UserId, Vec<UserConnection>>>>);

impl ActiveConnections {
    pub async fn add(&self, user: UserId, sender: Sender) -> ConnectionId {
        let id = ConnectionId::next();
        let connection = UserConnection { id, sender };
        self.0
            .write()
            .await
            .entry(user)
            .or_default()
            .push(connection);
        id
    }

    pub async fn remove(&self, user: &UserId, id: ConnectionId) {
        if let Some(user_connections) = self.0.write().await.get_mut(user) {
            user_connections.retain(|connection| connection.id != id)
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: &str) {
        log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);
        if let Some(connections) = self.0.read().await.get(user) {
            for connection in connections {
                if let Err(_disconnected) = connection.sender.send(Ok(Message::text(msg))) {
                    // other side disconnected
                }
            }
        }
    }
}
