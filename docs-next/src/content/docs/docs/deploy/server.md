---
title: Server deployment
description: Run appctl serve behind your TLS terminator for a team or a product.
---

`appctl serve` is an HTTP + WebSocket daemon. Any client that can send an HTTP request or open a WebSocket can talk to it.

## Minimum viable deployment

```bash
appctl serve \
  --bind 0.0.0.0 \
  --port 4242 \
  --token "$(openssl rand -hex 32)" \
  --strict \
  --confirm
```

Put TLS termination in front (Caddy, Nginx, Cloudflare Tunnel). `appctl serve` does not terminate TLS.

`appctl serve` prints the local URL, network URL when applicable, token status,
and tunnel/production hints on startup. In production, prefer loopback binding
with a reverse proxy:

```bash
appctl serve --bind 127.0.0.1 --port 4242 --token "$APPCTL_TOKEN" --strict
```

## systemd unit

`/etc/systemd/system/appctl.service`:

```ini
[Unit]
Description=appctl serve
After=network-online.target

[Service]
Type=simple
User=appctl
WorkingDirectory=/srv/appctl
Environment="APPCTL_TOKEN=replace-me"
ExecStart=/usr/local/bin/appctl serve \
  --bind 127.0.0.1 \
  --port 4242 \
  --token ${APPCTL_TOKEN} \
  --strict
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
```

Reload and start:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now appctl
```

## Caddy in front

```
appctl.internal.example.com {
  encode zstd gzip
  reverse_proxy 127.0.0.1:4242
}
```

Caddy handles TLS. `appctl` stays on loopback.

## Embed in a product

```js
// POST /run from a browser or server
const res = await fetch("https://appctl.internal/run", {
  method: "POST",
  headers: {
    "Content-Type": "application/json",
    "Authorization": `Bearer ${token}`,
  },
  body: JSON.stringify({ message: "show me the 5 latest orders" }),
});
const { result, events } = await res.json();
```

For streaming UIs, open a WebSocket to `/chat` instead. See [WebSocket](/docs/api/websocket/).

## Safety posture

For any network beyond localhost:

- Always set `--token`.
- Default to `--strict`. Only allow `provenance=verified` tools.
- Consider `--read-only` for the main deployment and a separate write-enabled instance on a different port or host.
- Run under a dedicated low-privilege user (`appctl` above).
- Keep target app credentials separate from the serve token. The serve token
  controls who can open appctl; `[target]` auth controls what appctl can do
  inside your app.

## Scaling

`appctl serve` is a single process with in-memory state. For redundancy, front multiple instances with a sticky-session load balancer (WebSocket connections must land on the same instance). Shared state is limited to the SQLite audit log; point multiple instances at different `--app-dir` directories.

## See also

- [HTTP endpoints](/docs/api/http/)
- [Secrets and keys](/docs/deploy/secrets-and-keys/)
- [Security](/docs/security/)
