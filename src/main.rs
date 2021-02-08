use color_eyre::{eyre::WrapErr, Result};
use flexi_logger::{colored_detailed_format, LogTarget, Logger};
use notify_push::config::Config;
use notify_push::message::DEBOUNCE_ENABLE;
use notify_push::metrics::serve_metrics;
use notify_push::{listen_loop, serve, App};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::oneshot;
use tokio::task::spawn;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let level = dotenv::var("LOG").ok();
    let level = level.as_deref().unwrap_or("warn");
    let log_handle = Logger::with_str(level)
        .log_target(LogTarget::StdOut)
        .format(colored_detailed_format)
        .start()?;
    let _ = dotenv::dotenv();

    let (serve_cancel, serve_cancel_handle) = oneshot::channel();
    let (metrics_cancel, metrics_cancel_handle) = oneshot::channel();
    let (listen_cancel, listen_cancel_handle) = oneshot::channel();

    let mut args = std::env::args();
    let config = match args.nth(1) {
        Some(file) => {
            Config::from_file(&file).wrap_err("Failed to load config from nextcloud config file")?
        }
        None => Config::from_env().wrap_err("Failed to load config from environment variables")?,
    };

    let port = dotenv::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(80u16);

    let metrics_port = dotenv::var("METRICS_PORT")
        .ok()
        .and_then(|port| port.parse().ok());

    if dotenv::var("DEBOUNCE_DISABLE").is_ok() {
        DEBOUNCE_ENABLE.store(false, Ordering::Relaxed);
    }

    log::trace!("Running with config: {:?} on port {}", config, port);

    let app = Arc::new(App::new(config, log_handle).await?);
    app.self_test().await?;

    spawn(serve(app.clone(), port, serve_cancel_handle));

    if let Some(metrics_port) = metrics_port {
        spawn(serve_metrics(metrics_port, metrics_cancel_handle));
    }

    spawn(listen_loop(app, listen_cancel_handle));

    // wait for either a sigint or sigterm
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;

    select! {
        _ = term.recv() => (),
        _ = int.recv() => (),
    };

    // then send cancel events to all of our spawned tasks

    log::info!("shutdown signal received, shutting down");

    serve_cancel.send(()).ok();
    metrics_cancel.send(()).ok();
    listen_cancel.send(()).ok();

    Ok(())
}
