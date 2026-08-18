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
$queue = \OCP\Server::get(OCA\NotifyPush\Queue\IQueue::class);
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

## Anonymous sessions

Sometimes you want to push messages to a client that doesn't have a user session, such as a visitor of a public share
or a device that is still going through some kind of pairing flow.

For these cases your app can create an "anonymous session". Creating a session gives you two values:

- an **id**, which your app keeps server side and uses to address the session
- a **token**, which you hand to the client and which the client uses to connect to the push server

Anonymous sessions live in a separate namespace from users, an anonymous session can never receive the messages of a
user (or of another session) and only receives the messages your app explicitly sends to it. In particular it does
*not* receive any of the built in `notify_file`, `notify_activity` or `notify_notification` messages.

### Creating a session

```php
// in a real app, you'll want to setup DI to get an instance of `IAnonymousSessionManager`
$sessionManager = \OCP\Server::get(OCA\NotifyPush\IAnonymousSessionManager::class);

$session = $sessionManager->createSession('myapp');

$session->getId();         // "myapp:xIhK1i..." keep this, you need it to send messages
$session->getToken();      // "eyJpZCI6..." hand this to the client
$session->getExpiration(); // unix timestamp after which the client can no longer connect
```

Sessions are not stored server side, the id and expiration date are encoded in the token itself and signed with an
instance wide key. This means the client can keep reconnecting with the same token until it expires, but also that an
individual session can not be revoked before it expires. Because of that the lifetime is kept short: the `ttl` (second
argument of `createSession`) is one hour by default and can not be more than 24 hours.

If a client needs to keep listening for longer than that, issue it a new token for the same session instead of
creating a new session, so the id your app stored stays valid:

```php
$session = $sessionManager->renewToken($sessionId);
```

The previous token is not invalidated by this, it keeps working until it expires on its own.

### Sending messages to a session

```php
$sessionManager->send($sessionId, 'my_message_type', ['foo' => 'bar']);
```

Which is delivered to the client exactly like a custom event for a user, as `'my_message_type {"foo":"bar"}'`.

Messages sent while no client is connected with the session are dropped, there is no buffering.

Session ids are prefixed with the id of the app that created them to keep apps from accidentally addressing each
others sessions. Note that this is a convention and not a security boundary, every app on the server can push to the
message queue directly and therefore to any session. Don't send data over a session of another app, and don't rely on
other apps not being able to send to yours.

### Connecting from the client

The client only needs the token, everything else is discovered from the capabilities. Note that the
`ocs/v2.php/cloud/capabilities` request can be made without authentication.

- Get the `websocket` and `anon_pre_auth` endpoints from the ocs capabilities request
- `POST` the token to the `anon_pre_auth` endpoint as `token`, a short lived pre-authentication token is returned
- Open the websocket
- Send an empty string as username over the websocket
- Send the pre-authentication token as password
- On disconnect, request a new pre-authentication token and connect again

```javascript
async function connect(nextcloud_url, session_token) {
    let capabilities = await fetch(`${nextcloud_url}/ocs/v2.php/cloud/capabilities`, {
        headers: {'Accept': 'application/json', 'OCS-APIREQUEST': 'true'},
    })
        .then(response => response.json())
        .then(json => json.ocs.data.capabilities.notify_push);

    let body = new FormData();
    body.set('token', session_token);
    let response = await fetch(capabilities.endpoints.anon_pre_auth, {method: 'POST', body});
    if (!response.ok) {
        throw new Error('session token is no longer valid');
    }
    let pre_auth_token = await response.text();

    let ws = new WebSocket(capabilities.endpoints.websocket);
    ws.onopen = () => {
        ws.send("");
        ws.send(pre_auth_token);
    };
    ws.onmessage = (msg) => {
        console.log(msg.data);
    };
    return ws;
}
```

The pre-authentication token is single use and only valid for 15 seconds, so it has to be requested again for every
(re)connect. The session token itself stays valid until it expires.

## Building

The server binary is built using rust and cargo, and requires a minimum of rust `1.94`.

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
