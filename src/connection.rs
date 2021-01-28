use crate::message::{DebounceMap, MessageType};
use crate::UserId;
use dashmap::DashMap;
use futures::stream::SplitSink;
use futures::SinkExt;
use smallvec::SmallVec;
use std::sync::atomic::{AtomicUsize, Ordering};
use warp::ws::{Message, WebSocket};

type Sender = SplitSink<WebSocket, Message>;

static NEXT_CONNECTION_ID: AtomicUsize = AtomicUsize::new(1);

/// A unique id identifying a connection
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct ConnectionId(usize);

impl ConnectionId {
    pub fn next() -> Self {
        ConnectionId(NEXT_CONNECTION_ID.fetch_add(1, Ordering::SeqCst))
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
        if let Some(mut user_connections) = self.0.get_mut(user) {
            if user_connections.debounce_map.should_send(&msg) {
                log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);

                // todo: something more clean than this (can't do retain because sending is async)
                let mut to_cleanup = Vec::new();

                for connection in user_connections.connections.iter_mut() {
                    if let Err(e) = connection.sender.send(Message::text(msg.to_string())).await {
                        log::info!(
                            "Failed to send websocket message: {:#}, closing connection",
                            e
                        );
                        to_cleanup.push(connection.id);
                    }
                }

                for id in to_cleanup {
                    user_connections
                        .connections
                        .retain(|connection| connection.id != id);
                }
            } else {
                log::trace!(target: "notify_push::send", "Not sending {} to {} due to debounce limits", msg, user);
            }
        }
    }
}
