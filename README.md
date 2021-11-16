# Client Push

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

## Quick setup

The app comes with a setup wizard that should guide you through the setup process for most setups.

- Install the "Client Push" (`notify_push`) app from the appstore
- Run `occ notify_push:setup` and follow the provided instructions,
  If the setup wizard fails you can find manual instructions below.

## Manual setup

The setup required consists of three steps

- Install the `notify_push` app from the appstore
- Setting up the push server
- Configuring the reverse proxy
- Configuring the nextcloud app

### Push server

The push server should be setup to run as a background daemon, the recommended way is by setting it up as a system service in the init system.
If you're not using systemd than any init or process management system that runs the push server binary
with the described environment variables will work.

#### systemd


For systemd based setups, can create a systemd service by creating a file named `/etc/systemd/system/notify_push.service` with the following
content.

```ini
[Unit]
Description = Push daemon for Nextcloud clients
Documentation=https://github.com/nextcloud/notify_push

[Service]
Environment = PORT=7867 # Change if you already have something running on this port
ExecStart = /path/to/push/binary/notify_push /path/to/nextcloud/config/config.php
User=www-data

[Install]
WantedBy = multi-user.target
```

<details>
<summary>Snap configuration (click to expand)</summary>

If you have installed Nextcloud via Snap, you need to use the following file instead and replace `CHANGEME` in `DATABASE_URL` with the value of `dbpassword` from `/var/snap/nextcloud/current/nextcloud/config/config.php`

```ini
[Unit]
Description = Push daemon for Nextcloud clients

[Service]
Environment=PORT=7867 # Change if you already have something running on this port
Environment=DATABASE_URL=mysql://nextcloud:CHANGEME@localhost/nextcloud?socket=/tmp/snap.nextcloud/tmp/sockets/mysql.sock
Environment=REDIS_URL=redis+unix:///tmp/snap.nextcloud/tmp/sockets/redis.sock
ExecStart=/var/snap/nextcloud/current/nextcloud/extra-apps/notify_push/bin/x86_64/notify_push /var/snap/nextcloud/current/nextcloud/config/config.php
User=root

[Install]
WantedBy = multi-user.target
```

</details>

#### OpenRC

For OpenRC based setups, you can create a OpenRC service by creating a file named
`/etc/init.d/notify_push` with the following content.

```sh
#!/sbin/openrc-run

description="Push daemon for Nextcloud clients"

pidfile=${pidfile:-/run/notify_push.pid}

command=${command:-/path/to/push/binary/notify_push}
command_user=${command_user:-www-data:www-data}
command_args="--port 7867 /path/to/nextcloud/config/config.php"
command_background=true
depend() {
        need redis nginx php-fpm7 mariadb
}
```

Adjust the paths, ports and user as needed.


#### Configuration

The push server can be configured either by loading the config from the nextcloud `config.php`
or by setting all options through environment variables.

Re-using the configuration from nextcloud is the recommended way, as it ensures that the configuration remains in sync.

If using the `config.php` isn't possible, you can configure the push server by setting the following environment
variables:

- `DATABASE_URL` connection url for the Nextcloud database, e.g. `postgres://user:password@db_host/db_name`
- `DATABASE_PREFIX` database prefix configured in Nextcloud, e.g. `oc_`
- `REDIS_URL` connection url for redis, e.g. `redis://redis_host`
- `NEXTCLOUD_URL` url for the nextcloud instance, e.g. `https://cloud.example.com`

Or you can specify the options as command line arguments, see `notify_push --help` for information about the command line arguments.

If a config option is set in multiple sources, the values from the command line argument overwrite values from the environment
which in turns overwrites the values from the `config.php`.

The port the server listens to can only be configured through the environment variable `PORT`, or `--port` argument and defaults to 7867.
Alternatively you can configure the server to listen on a unix socket by setting the `SOCKET_PATH` environment variable or `--socket-path` argument.

Note that Nextcloud load all files matching `*.config.php` in the config directory in additional to the main config file.
You can enable this same behavior by passing the `--glob-config` option.

#### TLS Configuration

The push server can be configured to serve over TLS. This is mostly intended for securing the traffic between the push server
and the reverse proxy if they are running on different hosts, running without a reverse proxy (or load balancer) is not recommended.

TLS can be enabled by setting the `--tls-cert` and `--tls-key` arguments (or the `TLS_CERT` and `TLS_KEY` environment variables).

#### Starting the service

Once the systemd service file is set up with the correct configuration you can start it using

- systemd: `sudo systemctl start notify_push`
- OpenRc: `sudo rc-service notify_push start`

and enable it to automatically start on boot using

- systemd: `sudo systemctl enable notify_push`
- OpenRc: `sudo rc-update add notify_push` 


Every time this app receives an update you should restart the systemd service using

- systemd: `sudo systemctl restart notify_push`
- OpenRc: `sudo rc-service notify_push restart`


<details>
<summary>Alternatively, you can do this automatically via systemctl by creating the following systemd service and path (click to expand)</summary>

First create a oneshot service to trigger the daemon restart

`/etc/systemd/system/notify_push-watcher.service`
```ini
[Unit]
Description=Restart Push daemon for Nextcloud clients when it receives updates
Documentation=https://github.com/nextcloud/notify_push
Requires=notify_push.service
After=notify_push.service
StartLimitIntervalSec=10
StartLimitBurst=5

[Service]
Type=oneshot
ExecStart=/usr/bin/systemctl restart notify_push.service

[Install]
WantedBy=multi-user.target
```

Then create a `path` job to trigger the restart whenever the push binary is changed

`/etc/systemd/system/notify_push-watcher.path`
```ini
[Unit]
Description=Restart Push daemon for Nextcloud clients when it receives updates
Documentation=https://github.com/nextcloud/notify_push
PartOf=notify_push-watcher.service

[Path]
PathModified=/path/to/push/binary/notify_push
Unit=notify_push-watcher.service

[Install]
WantedBy=multi-user.target
```

Adjusting the path as needed.

Finally, enable it with 

```bash
sudo systemctl enable notify_push-watcher.path
```

</details>

### Reverse proxy

It is **strongly** recommended to set up the push service behind a reverse proxy, this both removes the need to open
a new port to the internet and handles the TLS encryption of the connection to prevent sending credentials in plain text.

You can probably use the same webserver that you're already using for your nextcloud

#### Nginx

If you're using nginx, add the following `location` block to the existing `server` block of the nextcloud server.

```nginx
location ^~ /push/ {
    proxy_pass http://127.0.0.1:7867/;
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
sudo a2enmod proxy_wstunnel
```

Then add the following lines to the `<VirtualHost>` block used for the Nextcloud server.

```apacheconf
ProxyPass /push/ws ws://127.0.0.1:7867/ws
ProxyPass /push/ http://127.0.0.1:7867/
ProxyPassReverse /push/ http://127.0.0.1:7867/
```

Afterwards you can restart apache using

```bash
sudo systemctl restart apache2
```

#### Caddy v2

```Caddyfile
route /push/* {
    uri strip_prefix /push
    reverse_proxy http://127.0.0.1:7867/
}
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

Where level is `error`, `warn`, `info`, `debug` or `trace`, or restore the log level to the previous value using

```bash
occ notify_push:log --restore
```

Alternatively you can set the log level of the push server in the `LOG` environment variable.

### Metrics

The push server can expose some basic metrics about the number of connected clients and the traffic flowing through the server
by setting the `METRICS_PORT` environment variable.

Once set the metrics are available in a prometheus compatible format at `/metrics` on the configured port.

### Self-signed certificates

If your nextcloud is using a self-signed certificate then you either need to set the `NEXTCLOUD_URL` to a non-https, local url,
or disable certificate verification by setting `ALLOW_SELF_SIGNED=true`.

## Troubleshooting

When running into issues you should always first ensure that you're on the latest release, as your issue might either
already be fixed or additional diagnostics might have been added.

### "push server is not a trusted proxy"

- Ensure you haven't added a duplicate `trusted_proxies` list to your `config.php`.
- If you're modified your `forwarded_for_headers` config, ensure that `HTTP_X_FORWARDED_FOR` is included.
- If your nextcloud hostname resolves do a dynamic ip you can try setting the `NEXTCLOUD_URL` to the internal ip of the server.
  
  Alternatively, editing the `/etc/hosts` file to point your nextcloud domain to the internal ip can work in some setups.
- If you're running your setup in docker and your containers are linked, you should be able to use the name of the nextcloud container as hostname in the `NEXTCLOUD_URL`


## Developing

For information about how to use the push server in your own app or client, see [DEVELOPING.md](./DEVELOPING.md)

## Test client

For development and testing purposes a test client is provided which can be downloaded from
the [github actions](https://github.com/nextcloud/notify_push/actions/workflows/rust.yml) page.<br>
(Click on a run from the list, scroll to the bottom and click on `test_client` to download the binary.)<br>
Please note: the Test client is only build for x86_64 Linux currently.

```bash
test_client https://cloud.example.com username password
```

Note that this does not support two-factor authentication of non-default login flows, you can use an app-password in those cases.
