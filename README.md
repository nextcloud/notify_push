# notify_push

Update notifications for nextcloud clients

## About

This app attempts to solve the issue where Nextcloud clients have to periodically check the server if any files have been changed.
In order to keep sync snappy, clients wants to check for updates often, which increases the load on the server.

With many clients all checking for updates a large portion of the server load can consist of just these update checks.

By providing a way for the server to send update notifications to the clients,
the need for the clients to make these checks can be greatly reduced.

Update notifications are provided on a "best effort" basis, updates might happen without a notification being send and
a notification can be send even if no update has actually happened. Clients are advised to still perform periodic checks
for updates on their own, although these can be run on a much lower frequency.

## Configuring

### Push server

The push server can be configured either by loading the config from the nextcloud `config.php`

```bash
notify_push /var/www/html/nextcloud/config/config.php
```

By re-using the configuration from nextcloud, it is ensured that the configuration remains in sync.

If using the `config.php` isn't possible, you can also configure the push server by setting the following environment variables:

- `DATABASE_URL` connection url for the Nextcloud database, e.g `postgres://user:password@db_host/db_name`
- `REDIS_URL` connection url for redis, e.g. `redis://redis_host`
- `NEXTCLOUD_URL` url for the nextcloud instance, e.g. `https://cloud.example.com`
- `TRUSTED_PROXIES` comma separated list of trusted proxies, e.g. `127.0.0.1,192.168.1.10`

If both the `config.php` and environment variable is provided, the environment variable will overwrite the value from config.php

The port the push server listens to can be controller by the `PORT` environment variable and defaults to 80.

### Nextcloud app

- enable the app `occ app:enable notify_push`
- setup a reverse proxy with ssl in front of the push server
- set the url of the push server `occ notify_push:setup https://push.example.com`

Because user credentials will be send to the push server, it's **strongly** recommended to setup an ssl proxy in front of the push server.  

The app will automatically run some tests to verify that the push server is configured correctly.

## Usage

Once the push server is setup and running and the nextcloud app is configured, clients can get notifications using the following steps.

- Get the push server url from the `notify_push` capability by sending an authenticated request to `https://cloud.example.com/ocs/v2.php/cloud/capabilities`
- Open a websocket connection to the provided websocket url
- Send the username over the websocket connection
- Send the password over the websocket connection
- If the credentials are correct, the server will return with "authenticated"
- The server will send a "notify_file" message every time a file for the user has been changed

### Example

An example javascript implementation would be

```javascript
function discover_endpoint (nextcloud_url, user, password) {
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

function listen (url, user, password) {
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

### Test client

For development purposes a test client is provided which can be downloaded from the [github actions](https://github.com/icewind1991/notify_push/actions) page.

```bash
test_client https://cloud.icewind.me username password
```