use crate::config::{Bind, TlsConfig};
use crate::{serve_at, App, Result};
use serde::Serialize;
use std::fmt;
use std::fmt::Write;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use warp::Filter;

pub static METRICS: Metrics = Metrics::new();

pub struct Metrics {
    active_connection_count: AtomicUsize,
    total_connection_count: AtomicUsize,
    mapping_query_count: AtomicUsize,
    events_received: AtomicUsize,
    messages_sent: AtomicUsize,
}

#[derive(Serialize)]
pub struct SerializeMetrics {
    active_connection_count: usize,
    active_user_count: usize,
    total_connection_count: usize,
    mapping_query_count: usize,
    events_received: usize,
    messages_sent: usize,
}

impl SerializeMetrics {
    #[inline]
    pub fn new(metrics: &Metrics, active_user_count: usize) -> Self {
        Self {
            active_connection_count: metrics.active_connection_count(),
            active_user_count,
            total_connection_count: metrics.total_connection_count(),
            mapping_query_count: metrics.mapping_query_count(),
            events_received: metrics.events_received(),
            messages_sent: metrics.messages_sent(),
        }
    }
}

impl fmt::Display for SerializeMetrics {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            fmt,
            "active_connection_count {}",
            self.active_connection_count
        )?;
        writeln!(fmt, "active_user_count {}", self.active_user_count)?;
        writeln!(
            fmt,
            "total_connection_count {}",
            self.total_connection_count
        )?;
        writeln!(fmt, "mapping_query_count {}", self.mapping_query_count)?;
        writeln!(fmt, "events_received {}", self.events_received)?;
        writeln!(fmt, "messages_sent {}", self.messages_sent)?;
        Ok(())
    }
}

impl Metrics {
    pub const fn new() -> Self {
        Metrics {
            active_connection_count: AtomicUsize::new(0),
            total_connection_count: AtomicUsize::new(0),
            mapping_query_count: AtomicUsize::new(0),
            events_received: AtomicUsize::new(0),
            messages_sent: AtomicUsize::new(0),
        }
    }

    #[inline]
    pub fn active_connection_count(&self) -> usize {
        self.active_connection_count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn total_connection_count(&self) -> usize {
        self.total_connection_count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn mapping_query_count(&self) -> usize {
        self.mapping_query_count.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn events_received(&self) -> usize {
        self.events_received.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn messages_sent(&self) -> usize {
        self.messages_sent.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn add_connection(&self) {
        self.total_connection_count.fetch_add(1, Ordering::Relaxed);
        self.active_connection_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn remove_connection(&self) {
        self.active_connection_count.fetch_sub(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_mapping_query(&self) {
        self.mapping_query_count.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_event(&self) {
        self.events_received.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn add_message(&self) {
        self.messages_sent.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn serve_metrics(
    app: Arc<App>,
    bind: Bind,
    cancel: oneshot::Receiver<()>,
    tls: Option<&TlsConfig>,
) -> Result<impl Future<Output = ()> + Send> {
    let app = warp::any().map(move || app.clone());

    let metrics = warp::path!("metrics").and(app).map(move |app: Arc<App>| {
        let metrics = SerializeMetrics::new(&METRICS, app.active_user_count());
        let mut response = String::with_capacity(128);
        write!(&mut response, "{}", metrics).unwrap();
        response
    });

    serve_at(metrics, bind, cancel, tls)
}
