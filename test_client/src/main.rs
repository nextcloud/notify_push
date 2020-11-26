use color_eyre::{eyre::WrapErr, Result};
use tungstenite::{connect, Message};
use url::Url;

fn main() -> Result<()> {
    color_eyre::install()?;

    let mut args = std::env::args();

    let bin = args.next().unwrap();
    let (host, username, password) = match (args.next(), args.next(), args.next()) {
        (Some(host), Some(username), Some(password)) => (host, username, password),
        _ => {
            eprintln!("usage {} <ws url> <username> <password>", bin);
            return Ok(());
        }
    };

    let (mut socket, _response) = connect(Url::parse(&host).wrap_err("Invalid websocket url")?)
        .wrap_err("Can't connect to server")?;

    socket
        .write_message(Message::Text(username))
        .wrap_err("Failed to send username")?;
    socket
        .write_message(Message::Text(password))
        .wrap_err("Failed to send password")?;

    loop {
        let msg = socket.read_message()?;
        println!("Received: {}", msg);
    }
}
