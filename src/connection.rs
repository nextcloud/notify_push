use crate::message::{DebounceMap, MessageType};
use crate::UserId;
use dashmap::DashMap;
use futures::stream::SplitSink;
use futures::{SinkExt, StreamExt};
use smallvec::SmallVec;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::spawn;
use warp::ws::{Message, WebSocket};

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
    sender: SplitSink<WebSocket, Message>,
}

#[derive(Default)]
pub struct ActiveConnections(DashMap<UserId, UnboundedSender<UserMessage>>);

pub static CONNECTION_COUNT: AtomicUsize = AtomicUsize::new(0);
pub static MESSAGES_SEND: AtomicUsize = AtomicUsize::new(0);

impl ActiveConnections {
    pub fn add(&self, user: UserId, sender: SplitSink<WebSocket, Message>) -> ConnectionId {
        let id = ConnectionId::next();
        let tx = self
            .0
            .entry(user)
            .or_insert_with(|| UserTask::default().spawn());
        let _ = tx.send(UserMessage::Add(id, sender));
        id
    }

    pub fn remove(&self, user: &UserId, id: ConnectionId) {
        if let Some(tx) = self.0.get(user) {
            let _ = tx.send(UserMessage::Remove(id));
        }
    }

    pub async fn send_to_user(&self, user: &UserId, msg: MessageType) {
        if let Some(tx) = self.0.get(user) {
            log::debug!(target: "notify_push::send", "Sending {} to {}", msg, user);

            let _ = tx.send(UserMessage::Message(msg));
        }
    }
}

pub enum UserMessage {
    Add(ConnectionId, SplitSink<WebSocket, Message>),
    Remove(ConnectionId),
    Message(MessageType),
}

#[derive(Default)]
pub struct UserTask {
    debounce_map: DebounceMap,
    connections: SmallVec<[UserConnection; 8]>,
}

impl UserTask {
    fn add_connection(&mut self, id: ConnectionId, sender: SplitSink<WebSocket, Message>) {
        let connection = UserConnection { id, sender };
        self.connections.push(connection);
        CONNECTION_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    fn remove_connection(&mut self, id: ConnectionId) {
        self.connections.retain(|connection| connection.id != id);
        CONNECTION_COUNT.fetch_sub(1, Ordering::Relaxed);
    }

    async fn send(&mut self, msg: MessageType) {
        if self.debounce_map.should_send(&msg) {
            // todo: something more clean than this (can't do retain because sending is async)
            let mut to_cleanup = Vec::new();

            for connection in self.connections.iter_mut() {
                MESSAGES_SEND.fetch_add(1, Ordering::Relaxed);

                if let Err(e) = connection.sender.send(Message::text(msg.to_string())).await {
                    log::info!(
                        "Failed to send websocket message: {:#}, closing connection",
                        e
                    );
                    to_cleanup.push(connection.id);
                }
            }

            self.connections
                .retain(|connection| !to_cleanup.contains(&connection.id));

            CONNECTION_COUNT.fetch_sub(to_cleanup.len(), Ordering::Relaxed);
        }
    }

    async fn run(mut self, mut rx: UnboundedReceiver<UserMessage>) {
        while let Some(event) = rx.next().await {
            match event {
                UserMessage::Add(id, sender) => self.add_connection(id, sender),
                UserMessage::Remove(id) => self.remove_connection(id),
                UserMessage::Message(message) => self.send(message).await,
            }
        }
    }

    pub fn spawn(self) -> UnboundedSender<UserMessage> {
        let (tx, rx) = unbounded_channel();

        spawn(self.run(rx));

        tx
    }
}
