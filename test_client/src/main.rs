/*
 * SPDX-FileCopyrightText: 2020 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use base64::Engine;
use flexi_logger::{AdaptiveFormat, Logger};
use log::{debug, info, trace, warn};
use miette::{IntoDiagnostic, Report, Result, WrapErr};
use serde_json::Value;
use std::env::var;
use tungstenite::{connect, Message};
use url::Url;

fn main() -> Result<()> {
    Logger::try_with_str(var("LOG").unwrap_or_else(|_| String::from("test_client=info,warn")))
        .into_diagnostic()?
        .adaptive_format_for_stdout(AdaptiveFormat::Detailed)
        .adaptive_format_for_stderr(AdaptiveFormat::Detailed)
        .start()
        .into_diagnostic()?;

    let mut args = std::env::args();

    let bin = args.next().unwrap();
    let (nc_url, username, password) = match (args.next(), args.next(), args.next()) {
        (Some(host), Some(username), Some(password)) => (host, username, password),
        _ => {
            eprintln!("usage {bin} <nextcloud url> <username> <password>");
            return Ok(());
        }
    };

    let ws_url = if nc_url.starts_with("ws") {
        nc_url
    } else {
        get_endpoint(&nc_url, &username, &password)?
    };
    info!("Found push server at {ws_url}");

    let ws_url = Url::parse(&ws_url)
        .into_diagnostic()
        .wrap_err("Invalid websocket url")?;
    let (mut socket, _response) = connect(ws_url)
        .into_diagnostic()
        .wrap_err("Can't connect to server")?;

    socket
        .send(Message::Text(username.into()))
        .into_diagnostic()
        .wrap_err("Failed to send username")?;
    socket
        .send(Message::Text(password.into()))
        .into_diagnostic()
        .wrap_err("Failed to send password")?;
    socket
        .send(Message::Text("listen notify_file_id".into()))
        .into_diagnostic()
        .wrap_err("Failed to send username")?;

    loop {
        if let Message::Text(text) = socket.read().into_diagnostic()? {
            if let Some(err) = text.strip_prefix("err: ") {
                warn!("Received error: {err}");
                return Ok(());
            } else if text.starts_with("notify_file") {
                info!("Received file update notification {text}");
            } else if text == "notify_activity" {
                info!("Received activity notification");
            } else if text == "notify_notification" {
                info!("Received notification notification");
            } else if text == "authenticated" {
                info!("Authenticated");
            } else {
                info!("Received: {text}");
            }
        }
    }
}

fn get_endpoint(nc_url: &str, user: &str, password: &str) -> Result<String> {
    let raw = ureq::get(&format!("{nc_url}/ocs/v2.php/cloud/capabilities"))
        .set(
            "Authorization",
            &format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"))
            ),
        )
        .set("Accept", "application/json")
        .set("OCS-APIREQUEST", "true")
        .call()
        .into_diagnostic()?
        .into_string()
        .into_diagnostic()?;
    trace!("Capabilities response: {raw}");
    let json: Value = serde_json::from_str(&raw)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to decode json capabilities response: {raw}"))?;
    if let Some(capabilities) = json["ocs"]["data"]["capabilities"].as_object() {
        debug!(
            "Supported capabilities: {:?}",
            capabilities.keys().collect::<Vec<_>>()
        );
        if let Some(notify_push) = capabilities.get("notify_push") {
            notify_push["endpoints"]["websocket"]
                .as_str()
                .map(|url| url.to_string())
                .ok_or(Report::msg("invalid notify_push capabilities"))
        } else if !capabilities.contains_key("files_sharing") {
            Err(Report::msg("capabilities response doesn't contain expect items, credentials are probably invalid"))
        } else {
            Err(Report::msg(
                "notify_push app doesn't seem to be enabled or setup",
            ))
        }
    } else {
        Err(Report::msg(format!(
            "invalid capabilities response: {json}"
        )))
    }
}
