use crate::UserId;
use dashmap::DashMap;
use smallvec::SmallVec;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use warp::ws::Message;

type Sender = mpsc::UnboundedSender<Result<Message, warp::Error>>;

static NEXT_CONNECTION_ID: AtomicUsize = AtomicUsize::new(1);

/// A unique id identifying a connection
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

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, SmallVec<[UserConnection; 4]>>);

impl ActiveConnections {
    pub fn add(&self, user: UserId, sender: Sender) -> ConnectionId {
        let id = ConnectionId::next();
        let connection = UserConnection { id, sender };
        self.0.entry(user).or_default().push(connection);
        id
    }

    pub fn remove(&self, user: &UserId, id: ConnectionId) {
        if let Some(mut user_connections) = self.0.get_mut(user) {
            user_connections.retain(|connection| connection.id != id)
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: &str) {
        log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);
        if let Some(connections) = self.0.get(user) {
            for connection in connections.iter() {
                if let Err(e) = connection.sender.send(Ok(Message::text(msg))) {
                    log::info!("Failed to send websocket message: {}", e)
                }
            }
        }
    }
}
