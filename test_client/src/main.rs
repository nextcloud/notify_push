use color_eyre::{eyre::WrapErr, Report, Result};
use flexi_logger::Logger;
use serde_json::Value;
use std::env::var;
use tungstenite::http::Request;
use tungstenite::{connect, Message};

fn main() -> Result<()> {
    color_eyre::install()?;
    Logger::try_with_str(&var("LOG").unwrap_or_else(|_| String::from("warn")))?.start()?;

    let mut args = std::env::args();

    let bin = args.next().unwrap();
    let (nc_url, username, password) = match (args.next(), args.next(), args.next()) {
        (Some(host), Some(username), Some(password)) => (host, username, password),
        _ => {
            eprintln!("usage {} <nextcloud url> <username> <password>", bin);
            return Ok(());
        }
    };

    let ws_url = get_endpoint(&nc_url, &username, &password)?;
    println!("Found push server at {}", ws_url);

    let ws_request = Request::get(ws_url)
        .body(())
        .wrap_err("Invalid websocket url")?;
    let (mut socket, _response) = connect(ws_request).wrap_err("Can't connect to server")?;

    socket
        .write_message(Message::Text(username))
        .wrap_err("Failed to send username")?;
    socket
        .write_message(Message::Text(password))
        .wrap_err("Failed to send password")?;

    loop {
        if let Message::Text(text) = socket.read_message()? {
            if text.starts_with("err: ") {
                eprintln!("Received error: {}", &text[5..]);
                return Ok(());
            } else if text == "notify_file" {
                println!("Received file update notification");
            } else if text == "notify_activity" {
                println!("Received activity notification");
            } else if text == "notify_notification" {
                println!("Received notification notification");
            } else if text == "authenticated" {
                println!("Authenticated");
            } else {
                println!("Received: {}", text);
            }
        }
    }
}

fn get_endpoint(nc_url: &str, user: &str, password: &str) -> Result<String> {
    let raw = ureq::get(&format!("{}/ocs/v2.php/cloud/capabilities", nc_url))
        .set(
            "Authorization",
            &format!(
                "Basic {}",
                base64::encode(&format!("{}:{}", user, password))
            ),
        )
        .set("Accept", "application/json")
        .set("OCS-APIREQUEST", "true")
        .call()?
        .into_string()?;
    log::debug!("Capabilities response: {}", raw);
    let json: Value = serde_json::from_str(&raw)
        .wrap_err_with(|| format!("Failed to decode json capabilities response: {}", raw))?;
    if let Some(capabilities) = json["ocs"]["data"]["capabilities"].as_object() {
        log::info!(
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
            "invalid capabilities response: {}",
            json
        )))
    }
}
