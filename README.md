# notify_push

Update notifications for nextcloud clients

## About

This app attempts to solve the issue where Nextcloud clients have to periodically check the server if any files have
been changed. In order to keep sync snappy, clients wants to check for updates often, which increases the load on the
server.

With many clients all checking for updates a large portion of the server load can consist of just these update checks.

By providing a way for the server to send update notifications to the clients, the need for the clients to make these
checks can be greatly reduced.

Update notifications are provided on a "best effort" basis, updates might happen without a notification being send and a
notification can be send even if no update has actually happened. Clients are advised to still perform periodic checks
for updates on their own, although these can be run on a much lower frequency.

## Requirements

This app requires a redis server to be setup and for nextcloud to be configured to use the redis server.

## Setup

The setup required consists of three steps

- Setting up the push server
- Configuring the reverse proxy
- Configuring the nextcloud app

### Push server

#### Setting up the service

The push server should be setup to run as a background daemon, the recommended way is by setting up a systemd service to
run the server.

You can create a systemd service by creating a file named `/etc/systemd/system/notify_push.service` with the following
content.

```ini
[Unit]
Description = Push daemon for Nextcloud clients

[Service]
Environment = PORT=3030 # Change if you already have something running on this port
ExecStart = /path/to/push/binary/notify_push /path/to/nextcloud/config/config.php

[Install]
WantedBy = multi-user.target
```

Adjusting the paths and ports as needed.

#### Configuration

The push server can be configured either by loading the config from the nextcloud `config.php`
or by setting all options trough environment variables.

Re-using the configuration from nextcloud is the recommended way, as it ensures that the configuration remains in sync.

If using the `config.php` isn't possible, you can configure the push server by setting the following environment
variables:

- `DATABASE_URL` connection url for the Nextcloud database, e.g `postgres://user:password@db_host/db_name`
- `REDIS_URL` connection url for redis, e.g. `redis://redis_host`
- `NEXTCLOUD_URL` url for the nextcloud instance, e.g. `https://cloud.example.com`

If both the `config.php` and environment variable is provided, the environment variable will overwrite the value from
config.php

The port the server listens to can only be configured trough the environment variable `PORT` and defaults to 80.

#### Starting the service

Once the systemd service file is setup with the correct configuration you can start it using

`sudo systemctl start notify_push`

and enable it to automatically start on boot using

`sudo systemctl enable notify_push`

### Reverse proxy

It is **strongly** recommended to setup the push service behind a reverse proxy, this both removes the need to open
a new port to the internet and handles the TLS encryption of the connection to prevent sending credentials in plain text.

You can probably use the same webserver that you're already using for your nextcloud

#### Nginx

If you're using nginx, add the following `location` block the the existing `server` block of the nextcloud server.

```nginx
location /push/ {
    proxy_pass http://localhost:3030/; # Replace the port if you picked a different port for the push service
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "Upgrade";
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
}
```

Note that both trailing slashes are required.

Once the nginx configuration is edit you can reload nginx using.

```bash
sudo nginx -s reload
```

#### Apache

To use apache as a reverse proxy you first need to enable the proxy modules using

```bash
sudo a2enmod proxy
sudo a2enmod proxy_http
```

Then add the following lines to the `<VirtualHost>` block used for the Nextcloud server.

```dotenv
ProxyPass /push/ http://localhost:3030/
ProxyPassReverse /push/ http://localhost:3030/
```

Afterwards you can restart apache using

```bash
sudo systemctl restart apache2
```

### Nextcloud app

Once the push server is configured and the reverse proxy setup, you can enable the `notify_push` app and tell it where
the push server is listening.

- enable the app `occ app:enable notify_push`
- set the url of the push server `occ notify_push:setup https://cloud.example.com/push`

The app will automatically run some tests to verify that the push server is configured correctly.

### Logging

By default, the push server only logs warnings, you can temporarily change the log level with an occ command

```bash
occ notify_push:log <level>
```

Where level is `error`, `warn`, `info`, debug` or `trace`, or restore the log level to the previous value using

```bash
occ notify_push:log --restore
```

Alternatively you can set the log level of the push server in the `LOG` environment variable.

### Metrics

The push server can expose some basic metrics about the number of connected clients and the traffic flowing trough the server
by setting the `METRICS_PORT` environment variable.

Once set the metrics are available in a prometheus compatible format at `/metrics` on the configured port.

## Developing

As developer of a Nextcloud app or client you can use the `notify_push` app to receive real time notifications from the
Nextcloud server.

### Nextcloud web interface

If you want to listen to incoming events from the web interface of your Nextcloud app,
you can use the [`@nextcloude/notify_push`](https://www.npmjs.com/package/@nextcloud/notify_push) javascript library.
Which will handle all the details for authenticating and connecting to the push server.

### Clients

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

#### Example

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

### Test client

For development purposes a test client is provided which can be downloaded from
the [github actions](https://github.com/nextcloud/notify_push/actions) page.

```bash
test_client https://cloud.example.com username password
```
