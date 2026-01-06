<!--
  - SPDX-FileCopyrightText: 2021 Nextcloud GmbH and Nextcloud contributors
  - SPDX-License-Identifier: AGPL-3.0-or-later
-->

# Developing

As developer of a Nextcloud app or client you can use the `notify_push` app to receive real time notifications from the
Nextcloud server.

## Nextcloud web interface

If you want to listen to incoming events from the web interface of your Nextcloud app,
you can use the [`@nextcloud/notify_push`](https://www.npmjs.com/package/@nextcloud/notify_push) javascript library.
Which will handle all the details for authenticating and connecting to the push server.

```js
import {listen} from '@nextcloud/notify_push'

let has_push = listen('notify_file', () => {
    console.log("A file has been changed")
})

if (!hash_push) {
    console.log("notify_push not available on the server")
}
```

## Clients

Desktop and other clients that don't run in the Nextcloud web interface can use the following steps to receive
notifications.

- Get the push server url from the `notify_push` capability by sending an authenticated request
  to `https://cloud.example.com/ocs/v2.php/cloud/capabilities`
- Open a websocket connection to the provided websocket url
- Send the username over the websocket connection
- Send the password over the websocket connection (see also [pre-authenticated tokens])
- If the credentials are correct, the server will return with "authenticated"
- The server will send the following notifications
    - "notify_file" when a file for the user has been changed
    - "notify_activity" when a new activity item for a user is created (note, due to workings of the activity app, file
      related activity doesn't trigger this notification)
    - "notify_notification" when a notification is created, processed or dismissed for a user
- Starting with version 0.4 you can opt into receiving the changed file ids for file update notifications by sending
  `listen notify_file_id` over the websocket.  
  Once enabled, the server will send "notify_file_id" followed by a json encoded array of file ids if the push server
  knows
  the ids of the changed files.
  In cases where there push server doesn't know which files have changed, it will send the regular "notify_file"
  message.

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

## Pre-authenticated tokens

In situations where you don't have the user credentials but you can send authenticated requests to nextcloud(such as
when you have authenticated cookies)
you can use "pre-authenticated tokens" instead of the username and password.

- Get the `pre_auth` endpoint from the ocs capabilities request
- Send an authenticated request to the endpoint, a token will be returned.
- Open the websocket as normal
- Send an empty string as username over the websocket
- Send the token from the `pre_auth` request as passwor

## Sending custom events

You can send custom events from a nextcloud app using the methods provided by `OCA\NotifyPush\IQueue`.

```php
// in a real app, you'll want to setup DI to get an instance of `IQueue`
$queue = \OC::$server->get(OCA\NotifyPush\IQueue::class);
$queue->push('notify_custom', [
	'user' => "uid",
	'message' => "my_message_type",
    'body' => ["foo" => "bar"], // optional
]);
```

Which will be pushed to client as `'my_message_type {"foo": "bar"}'` and can be used with the `@nextcloud/notify_push`
client using

```js
import {listen} from '@nextcloud/notify_push'

listen('my_message_type', (message_type, optional_body) => {

})
```

## Building

The server binary is built using rust and cargo, and requires a minimum of rust `1.85`.

- Install `rust` through your package manager or [rustup](https://rustup.rs/)
- Run `cargo build`

Any build intended for production use or distribution
should be compiled in release mode for optimal performance and targeting musl libc for improved portability.

```bash
cargo build --release --target=x86_64-unknown-linux-musl
```

### Cross compiling

Cross compiling to other platforms can be done using two ways:

- using [`nix`](https://nixos.org/download.html) and `nix build .#aarch64-unknown-linux-musl` (recommended, binaries can
  be found in `./result/bin`)
- using [`cross`](https://github.com/rust-embedded/cross) and
  `cross build --release --target=aarch64-unknown-linux-musl`
