use color_eyre::{eyre::WrapErr, Report, Result};
use tungstenite::http::Request;
use tungstenite::{connect, Message};

fn main() -> Result<()> {
    color_eyre::install()?;

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
        let msg = socket.read_message()?;
        let text = msg.to_text()?;
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
            println!("Received: {}", msg);
        }
    }
}

fn get_endpoint(nc_url: &str, user: &str, password: &str) -> Result<String> {
    let json = ureq::get(&format!("{}/ocs/v2.php/cloud/capabilities", nc_url))
        .auth(user, password)
        .set("Accept", "application/json")
        .set("OCS-APIREQUEST", "true")
        .call()
        .into_json()
        .wrap_err("Failed to decode json capabilities response")?;
    Ok(
        json["ocs"]["data"]["capabilities"]["notify_push"]["endpoints"]["websocket"]
            .as_str()
            .map(|url| url.to_string())
            .ok_or(Report::msg(
                "notify_push app not enabled, invalid credentials or invalid capabilities response",
            ))?,
    )
}
