---
title: appctl serve
description: Run appctl as an HTTP + WebSocket daemon with a bundled web UI.
---

Run `appctl` as a daemon. It exposes HTTP endpoints, streams `AgentEvent`s over WebSocket, and serves a bundled React web UI.

## Usage

```
appctl serve [OPTIONS]
```

## Options

- `--port <PORT>` — HTTP port (default `4242`).
- `--bind <HOST>` — bind address (default `127.0.0.1`; use `0.0.0.0` to accept LAN).
- `--token <TOKEN>` — require `Authorization: Bearer <TOKEN>` on every request.
- `--provider <NAME>`, `--model <NAME>` — override the default LLM.
- `--strict`, `--read-only`, `--dry-run`, `--confirm` — safety flags (same as `chat`). Note: `--confirm` defaults to `true` under `serve` since there is no human at the CLI to answer prompts.

## What you get

- `GET /` — the bundled web UI (React, works offline).
- `GET /schema` — the current schema as JSON.
- `GET /tools` — derived tools list the agent sees.
- `GET /config/public` — non-secret config snapshot, including redacted provider auth state for the UI.
- `GET /history` — list history entries.
- `POST /run` — submit a prompt, get a response with the full event trail.
- `WS /chat` — bidirectional chat with streaming `AgentEvent`s.

See [HTTP endpoints](/docs/api/http/) for payload shapes.

## Example

```bash
appctl serve --port 4242 --token $(openssl rand -hex 32)
```

Open `http://127.0.0.1:4242/` in a browser. Paste the token when prompted, or set it in the client as `Authorization: Bearer <TOKEN>`.

The bundled web UI shows:

- active provider
- redacted provider auth state
- expiry and recovery hints when known
- target URL, schema source, and daemon safety flags

## LAN / shared deployments

```bash
appctl serve --bind 0.0.0.0 --port 4242 \
  --token "$APPCTL_TOKEN" \
  --strict --read-only
```

Run behind your own TLS terminator. `appctl serve` does not terminate TLS; use Caddy, Nginx, or Cloudflare Tunnel.

## Related

- [HTTP endpoints](/docs/api/http/)
- [WebSocket](/docs/api/websocket/)
- [Provider matrix](/docs/provider-matrix/)
- [Deploy → Server](/docs/deploy/server/)
