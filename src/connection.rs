use crate::message::{DebounceMap, MessageType};
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
struct UserConnectionList {
    debounce_map: DebounceMap,
    connections: SmallVec<[UserConnection; 8]>,
}

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, UserConnectionList>);

impl ActiveConnections {
    pub fn add(&self, user: UserId, sender: Sender) -> ConnectionId {
        let id = ConnectionId::next();
        let connection = UserConnection { id, sender };
        self.0.entry(user).or_default().connections.push(connection);
        id
    }

    pub fn remove(&self, user: &UserId, id: ConnectionId) {
        let should_remove = if let Some(mut user_connections) = self.0.get_mut(user) {
            user_connections
                .connections
                .retain(|connection| connection.id != id);
            user_connections.connections.is_empty()
        } else {
            false
        };

        if should_remove {
            self.0.remove(user);
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: MessageType) {
        if let Some(mut connections) = self.0.get_mut(user) {
            if connections.debounce_map.should_send(&msg) {
                log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);
                for connection in connections.connections.iter() {
                    if let Err(e) = connection.sender.send(Ok(Message::text(msg.to_string()))) {
                        log::info!("Failed to send websocket message: {}", e)
                    }
                }
            } else {
                log::trace!(target: "notify_push::send", "Not sending {} to {} due to debounce limits", msg, user);
            }
        }
    }
}
