# Developing

As developer of a Nextcloud app or client you can use the `notify_push` app to receive real time notifications from the
Nextcloud server.

## Nextcloud web interface

If you want to listen to incoming events from the web interface of your Nextcloud app,
you can use the [`@nextcloud/notify_push`](https://www.npmjs.com/package/@nextcloud/notify_push) javascript library.
Which will handle all the details for authenticating and connecting to the push server.

## Clients

Desktop and other clients that don't run in the Nextcloud web interface can use the following steps to receive notifications.

- Get the push server url from the `notify_push` capability by sending an authenticated request
  to `https://cloud.example.com/ocs/v2.php/cloud/capabilities`
- Open a websocket connection to the provided websocket url
- Send the username over the websocket connection
- Send the password over the websocket connection
- If the credentials are correct, the server will return with "authenticated"
- The server will send the following notifications
    - "notify_file" when a file for the user has been changed
    - "notify_activity" when a new activity item for a user is created (note, due to workings of the activity app, file
      related activity doesn't trigger this notification)
    - "notify_notification" when a notification is created, processed or dismissed for a user

### Example

An example javascript implementation would be

```javascript
function discover_endpoint(nextcloud_url, user, password) {
    let headers = new Headers();
    headers.set('Accept', 'application/json');
    headers.set('OCS-APIREQUEST', 'true');
    headers.set('Authorization', 'Basic ' + btoa(user + ":" + password));

    return fetch(`${nextcloud_url}/ocs/v2.php/cloud/capabilities`, {
        method: 'GET',
        headers: headers,
    })
        .then(response => response.json())
        .then(json => json.ocs.data.capabilities.notify_push.endpoints.websocket);
}

function listen(url, user, password) {
    let ws = new WebSocket(url);
    ws.onmessage = (msg) => {
        console.log(msg);
    }
    ws.onopen = () => {
        ws.send(user);
        ws.send(password);
    }
}

let username = "...";
let password = "...";
let nextcloud_url = "https://cloud.example.com";
discover_endpoint(nextcloud_url, username, password).then((endpoint) => {
    console.log(`push server is at ${endpoint}`)
    listen(endpoint, "admin", "admin");
});

```

```bash
test_client https://cloud.example.com username password
```

Note that this does not support two-factor authentication of non-default login flows, you can use an app-password in those cases.

## Building

The server binary is built using rust and cargo, and requires a minimum of rust `1.46`.

- Install `rust` through your package manager or [rustup](https://rustup.rs/)
- Run `cargo build`

Any build intended for production use or distribution
should be compiled in release mode for optimal performance and targeting musl libc for improved portability.

```bash
cargo build --release --target=x86_64-unknown-linux-musl
```

Cross compiling for other platform is done easiest using [`cross`](https://github.com/rust-embedded/cross), for example:

```bash
cross build --release --target=aarch64-unknown-linux-musl
```

If you're running into an issue building the `termion` dependency on a non-linux OS, try building with `--no-default-features`.
