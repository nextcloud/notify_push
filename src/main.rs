/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use clap::Parser;
use flexi_logger::{detailed_format, AdaptiveFormat, Logger, LoggerHandle};
use miette::{IntoDiagnostic, Result, WrapErr};
use notify_push::config::{Config, Opt};
use notify_push::error::ConfigError;
use notify_push::message::DEBOUNCE_ENABLE;
use notify_push::metrics::serve_metrics;
use notify_push::{listen_loop, serve, App, Error};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::oneshot;
use tokio::task::spawn;

fn main() -> Result<()> {
    miette::set_panic_hook();
    sqlx::any::install_default_drivers();
    let _ = dotenvy::dotenv();

    let opt: Opt = Opt::parse();
    if opt.version {
        println!("notify_push {}", env!("NOTIFY_PUSH_VERSION"));
        return Ok(());
    }
    let dump_config = opt.dump_config;
    let config = Config::from_opt(opt)?;

    if dump_config {
        println!("{config:#?}");
        return Ok(());
    }

    // initialize the logger before starting the tokio runtime
    // this prevents potential issues around getting the local time offset
    // which isn't properly tread safe on linux
    let log_handle = Logger::try_with_str(&config.log_level)
        .map_err(ConfigError::LogLevel)?
        .log_to_stdout();
    let log_handle = if config.no_ansi {
        log_handle.format_for_stdout(detailed_format)
    } else {
        log_handle.adaptive_format_for_stdout(AdaptiveFormat::Detailed)
    }
    .start()
    .into_diagnostic()
    .wrap_err("Failed to initialize log handler")?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(run(config, log_handle))?;
    Ok(())
}

async fn run(config: Config, log_handle: LoggerHandle) -> Result<()> {
    let (serve_cancel, serve_cancel_handle) = oneshot::channel();
    let (metrics_cancel, metrics_cancel_handle) = oneshot::channel();
    let (listen_cancel, listen_cancel_handle) = oneshot::channel();

    log::trace!("Running with config: {config:?}");

    if config.allow_self_signed {
        log::info!("Running with certificate validation disabled");
    }

    if dotenvy::var("DEBOUNCE_DISABLE").is_ok() {
        DEBOUNCE_ENABLE.store(false, Ordering::Relaxed);
    }

    let bind = config.bind.clone();
    let tls = config.tls.clone();
    let metrics_bind = config.metrics_bind.clone();
    let max_debounce_time = config.max_debounce_time;
    let max_connection_time = config.max_connection_time;
    let app = Arc::new(App::new(config, log_handle).await?);
    if let Err(e) = app.self_test().await {
        log::error!("Self test failed: {e:#}");
    }

    log::trace!("Listening on {bind}");
    let server = spawn(serve(
        app.clone(),
        bind,
        serve_cancel_handle,
        tls.as_ref(),
        max_debounce_time,
        max_connection_time,
    )?);

    if let Some(metrics_bind) = metrics_bind {
        log::trace!("Metrics listening {metrics_bind}");
        spawn(serve_metrics(
            metrics_bind,
            metrics_cancel_handle,
            tls.as_ref(),
        )?);
    }

    // tell SystemD that sockets have been bound to their addresses
    #[cfg(feature = "systemd")]
    sd_notify::notify(true, &[sd_notify::NotifyState::Ready]).map_err(Error::SystemD)?;

    spawn(listen_loop(app, listen_cancel_handle));

    // wait for either a sigint or sigterm
    let mut term = signal(SignalKind::terminate()).map_err(Error::SignalHook)?;
    let mut int = signal(SignalKind::interrupt()).map_err(Error::SignalHook)?;

    select! {
        _ = term.recv() => (),
        _ = int.recv() => (),
    };

    // then send cancel events to all of our spawned tasks

    log::info!("shutdown signal received, shutting down");

    serve_cancel.send(()).ok();
    metrics_cancel.send(()).ok();
    listen_cancel.send(()).ok();

    server
        .await
        .into_diagnostic()
        .wrap_err("Error while running warp server")?;

    Ok(())
}
