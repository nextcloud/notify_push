/*
 * SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use crate::config::{Bind, TlsConfig};
use crate::message::MessageType;
use crate::{serve_at, Result};
use serde::{Serialize, Serializer};
use std::fmt::Write;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;
use warp::Filter;

pub static METRICS: Metrics = Metrics::new();

#[derive(Default)]
pub struct Metrics {
    active_connection_count: AtomicUsize,
    active_user_count: AtomicUsize,
    total_connection_count: AtomicUsize,
    mapping_query_count: AtomicUsize,
    events_received: AtomicUsize,
    messages_sent: AtomicUsize,
    messages_sent_file: AtomicUsize,
    messages_sent_activity: AtomicUsize,
    messages_sent_notification: AtomicUsize,
    messages_sent_custom: AtomicUsize,
}

#[derive(Serialize)]
struct SerializeMetrics {
    active_connection_count: usize,
    active_user_count: usize,
    total_connection_count: usize,
    mapping_query_count: usize,
    events_received: usize,
    messages_sent: usize,
    messages_sent_file: usize,
    messages_sent_activity: usize,
    messages_sent_notification: usize,
    messages_sent_custom: usize,
}

impl From<&Metrics> for SerializeMetrics {
    fn from(metrics: &Metrics) -> Self {
        SerializeMetrics {
            active_connection_count: metrics.active_connection_count(),
            active_user_count: metrics.active_user_count(),
            total_connection_count: metrics.total_connection_count(),
            mapping_query_count: metrics.mapping_query_count(),
            events_received: metrics.events_received(),
            messages_sent: metrics.messages_sent(),
            messages_sent_file: metrics.messages_sent_file(),
            messages_sent_activity: metrics.messages_sent_activity(),
            messages_sent_notification: metrics.messages_sent_notification(),
            messages_sent_custom: metrics.messages_sent_custom(),
        }
    }
}

impl Serialize for Metrics {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        SerializeMetrics::from(self).serialize(serializer)
    }
}

impl Metrics {
    pub const fn new() -> Self {
        Metrics {
            active_connection_count: AtomicUsize::new(0),
            active_user_count: AtomicUsize::new(0),
            total_connection_count: AtomicUsize::new(0),
            mapping_query_count: AtomicUsize::new(0),
            events_received: AtomicUsize::new(0),
            messages_sent: AtomicUsize::new(0),
            messages_sent_file: AtomicUsize::new(0),
            messages_sent_activity: AtomicUsize::new(0),
            messages_sent_notification: AtomicUsize::new(0),
            messages_sent_custom: AtomicUsize::new(0),
        }
    }

    pub fn active_connection_count(&self) -> usize {
        self.active_connection_count.load(Ordering::Relaxed)
    }

    pub fn total_connection_count(&self) -> usize {
        self.total_connection_count.load(Ordering::Relaxed)
    }

    pub fn mapping_query_count(&self) -> usize {
        self.mapping_query_count.load(Ordering::Relaxed)
    }

    pub fn events_received(&self) -> usize {
        self.events_received.load(Ordering::Relaxed)
    }

    pub fn messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::Relaxed)
    }

    pub fn messages_sent_file(&self) -> usize {
        self.messages_sent_file.load(Ordering::Relaxed)
    }

    pub fn messages_sent_activity(&self) -> usize {
        self.messages_sent_activity.load(Ordering::Relaxed)
    }

    pub fn messages_sent_notification(&self) -> usize {
        self.messages_sent_notification.load(Ordering::Relaxed)
    }

    pub fn messages_sent_custom(&self) -> usize {
        self.messages_sent_custom.load(Ordering::Relaxed)
    }

    pub fn add_connection(&self) {
        self.total_connection_count.fetch_add(1, Ordering::Relaxed);
        self.active_connection_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn remove_connection(&self) {
        self.active_connection_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn active_user_count(&self) -> usize {
        self.active_user_count.load(Ordering::Relaxed)
    }

    pub fn add_user(&self) {
        self.active_user_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn remove_user(&self) {
        self.active_user_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn add_mapping_query(&self) {
        self.mapping_query_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_event(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_message(&self, ty: MessageType) {
        match ty {
            MessageType::File => self.messages_sent_file.fetch_add(1, Ordering::Relaxed),
            MessageType::Activity => self.messages_sent_activity.fetch_add(1, Ordering::Relaxed),
            MessageType::Notification => self
                .messages_sent_notification
                .fetch_add(1, Ordering::Relaxed),
            MessageType::Custom => self.messages_sent_custom.fetch_add(1, Ordering::Relaxed),
        };
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn serve_metrics(
    bind: Bind,
    cancel: oneshot::Receiver<()>,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = ()> + Send> {
    let metrics = warp::path!("metrics").map(|| {
        let mut response = String::with_capacity(128);
        let _ = writeln!(
            &mut response,
            "active_connection_count {}",
            METRICS.active_connection_count()
        );
        let _ = writeln!(
            &mut response,
            "active_user_count {}",
            METRICS.active_user_count()
        );
        let _ = writeln!(
            &mut response,
            "total_connection_count {}",
            METRICS.total_connection_count()
        );
        let _ = writeln!(
            &mut response,
            "mapping_query_count {}",
            METRICS.mapping_query_count()
        );
        let _ = writeln!(
            &mut response,
            "event_count_total {}",
            METRICS.events_received()
        );
        let _ = writeln!(
            &mut response,
            "message_count_total {}",
            METRICS.messages_sent()
        );
        let _ = writeln!(
            &mut response,
            "message_count_total{{type=\"file\"}} {}",
            METRICS.messages_sent_file()
        );
        let _ = writeln!(
            &mut response,
            "message_count_total{{type=\"notification\"}} {}",
            METRICS.messages_sent_notification()
        );
        let _ = writeln!(
            &mut response,
            "message_count_total{{type=\"activity\"}} {}",
            METRICS.messages_sent_activity()
        );
        let _ = writeln!(
            &mut response,
            "message_count_total{{type=\"custom\"}} {}",
            METRICS.messages_sent_custom()
        );
        response
    });

    serve_at(metrics, bind, cancel, tls)
}
