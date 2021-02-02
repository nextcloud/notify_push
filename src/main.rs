use color_eyre::{eyre::WrapErr, Result};
use notify_push::config::Config;
use notify_push::message::DEBOUNCE_ENABLE;
use notify_push::metrics::serve_metrics;
use notify_push::{listen, serve, App};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    pretty_env_logger::init();
    let _ = dotenv::dotenv();

    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

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

    let app = Arc::new(App::new(config).await?);
    app.self_test().await?;

    tokio::task::spawn(serve(app.clone(), port));

    if let Some(metrics_port) = metrics_port {
        tokio::task::spawn(serve_metrics(metrics_port));
    }

    loop {
        if let Err(e) = listen(app.clone()).await {
            eprintln!("Failed to setup redis subscription: {:#}", e);
        }
        log::warn!("Redis server disconnected, reconnecting in 1s");
        tokio::time::delay_for(Duration::from_secs(1)).await;
    }
}
