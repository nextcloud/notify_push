use crate::config::Bind;
use crate::serve_at;
use color_eyre::Result;
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
    total_connection_count: AtomicUsize,
    mapping_query_count: AtomicUsize,
    events_received: AtomicUsize,
    messages_send: AtomicUsize,
}

#[derive(Serialize)]
struct SerializeMetrics {
    active_connection_count: usize,
    total_connection_count: usize,
    mapping_query_count: usize,
    events_received: usize,
    messages_send: usize,
}

impl From<Metrics> for SerializeMetrics {
    fn from(metrics: Metrics) -> Self {
        SerializeMetrics {
            active_connection_count: metrics.active_connection_count(),
            total_connection_count: metrics.total_connection_count(),
            mapping_query_count: metrics.mapping_query_count(),
            events_received: metrics.events_received(),
            messages_send: metrics.messages_send(),
        }
    }
}

impl From<&Metrics> for SerializeMetrics {
    fn from(metrics: &Metrics) -> Self {
        SerializeMetrics {
            active_connection_count: metrics.active_connection_count(),
            total_connection_count: metrics.total_connection_count(),
            mapping_query_count: metrics.mapping_query_count(),
            events_received: metrics.events_received(),
            messages_send: metrics.messages_send(),
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
            total_connection_count: AtomicUsize::new(0),
            mapping_query_count: AtomicUsize::new(0),
            events_received: AtomicUsize::new(0),
            messages_send: AtomicUsize::new(0),
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

    pub fn messages_send(&self) -> usize {
        self.messages_send.load(Ordering::Relaxed)
    }

    pub fn add_connection(&self) {
        self.total_connection_count.fetch_add(1, Ordering::Relaxed);
        self.active_connection_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn remove_connection(&self) {
        self.active_connection_count.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn add_mapping_query(&self) {
        self.mapping_query_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_event(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_message(&self) {
        self.messages_send.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn serve_metrics(
    bind: Bind,
    cancel: oneshot::Receiver<()>,
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
            METRICS.messages_send()
        );
        response
    });

    serve_at(metrics, bind, cancel)
}
