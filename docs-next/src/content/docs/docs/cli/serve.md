---
title: appctl serve
description: HTTP + WebSocket daemon for the embedded web console and custom frontends.
---

`appctl serve` starts an HTTP server that does two things:

1. Serves the **embedded web console** — an operator UI shipped inside the
   `appctl` binary (no separate install, no CDN calls).
2. Exposes the same agent loop as `appctl chat` over a JSON HTTP + WebSocket
   API so you can drive it from your own UI, IDE plugin, or script.

The server runs the same planner, executor, and safety rails as the CLI. All
tools, logs, and configuration live in the current app directory
(`--app-dir`, default `.appctl`).

## Usage

```bash
appctl serve [OPTIONS]
```

## Options

| Flag | Default | What it does |
| --- | --- | --- |
| `--bind <ADDR>` | `127.0.0.1` | Interface to listen on. Use `0.0.0.0` only with `--token`. Can be set with env `APPCTL_BIND`. |
| `--port <N>` | `4242` | TCP port. Use `0` to let the OS pick a free port (the printed URL includes the real port). Env: `APPCTL_PORT`. |
| `--no-open` | off | By default, appctl opens the local UI in your default browser after the server is listening. Pass this to skip that. |
| `--token <STRING>` | unset | Require this bearer token on every request. When set, the web UI prompts for it. |
| `--identity-header <NAME>` | `x-appctl-client-id` | Header used to tag requests with a caller identity in the activity log. |
| `--tunnel` | off | Start `cloudflared tunnel --url ...` next to the local server. |
| `--provider <NAME>` | — | Override the default provider for this server instance. |
| `--model <NAME>` | — | Override the provider's model. |
| `--read-only` | off | Block every mutating tool server-wide. |
| `--dry-run` | off | Skip real I/O; return simulated events. |
| `--strict` | off | Block `provenance = "inferred"` tools until verified. |
| `--confirm` | **on** | Auto-approve mutations. Default is on (non-interactive). Pass `--confirm=false` to require per-call approval from the web UI. |

Flags set on `appctl serve` are the **minimum** safety level for every request.
Clients may request stricter modes (for example, turning on `read_only` for one
turn), but they cannot relax server-enforced flags.

## The web console

On macOS, Linux (via `xdg-open`), and Windows, **your default browser opens automatically** to the real listening URL (after a very short delay) unless you pass `--no-open`.

Open `http://127.0.0.1:4242/` if you are using the default port. The console ships as a single-page
app with four tabs:

- **Chat** — streaming conversation with the agent. Tool calls render inline
  as collapsible cards showing arguments and truncated responses.
- **Tools** — searchable list of every tool the agent can call, with its
  `kind`, `op`, safety level, and schema.
- **History** — the activity log (same table as `appctl history`), with
  expandable rows for arguments and raw response. If clients send the identity
  header, the session label is recorded there too.
- **Settings** — provider status, sync summary, and a field for the auth token
  when `--token` is set.

The UI connects over `WS /chat` for streaming; if WebSocket is blocked it
falls back to `POST /run` for non-streaming completions. Both paths keep
multi-turn **conversation memory** in the server process using the same
`session_id`. WebSocket and HTTP requests can resume the same transcript, and
the web console keeps the id across reconnects so the displayed thread matches
the model context. If the configured history limit trims older turns, the event
stream includes a notice.

## HTTP endpoints

All endpoints honour `--token` (via `Authorization: Bearer ...` or
`x-appctl-token`) when set. Clients can also send the configured identity
header (default `x-appctl-client-id`) so requests are labeled in history and
the web activity panel.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/tools` | `.appctl/tools.json` as JSON. |
| `GET` | `/history?limit=<N>` | Last N audit rows. |
| `GET` | `/schema` | `.appctl/schema.json` as JSON. |
| `GET` | `/config/public` | Redacted configuration (provider name, model, app name, safety flags). No secrets. |
| `POST` | `/run` | One-shot prompt, returns final answer + events. |
| `WS` | `/chat` | Streaming agent events. |

See [HTTP endpoints](/docs/api/http/) and [WebSocket](/docs/api/websocket/) for
request and response shapes.

## Examples

```bash
# Local-only operator console
appctl serve

# Share on the LAN behind a token
appctl serve --bind 0.0.0.0 --token "$(openssl rand -hex 24)"

# Start a public tunnel through cloudflared
appctl serve --token "$(openssl rand -hex 24)" --tunnel

# Read-only, dry-run demo instance
appctl serve --read-only --dry-run

# Force a specific provider for a server that runs inside a CI job
appctl serve --provider openai --model gpt-4o-mini --confirm=false
```

## Security notes

- The bind address defaults to `127.0.0.1`. Changing it to `0.0.0.0` without
  also passing `--token` is a mistake — the server will still start, but
  anything on your network can use your provider credits.
- Browser requests with an `Origin` header must match the daemon host (or
  forwarded host). This blocks cross-site pages from driving a local daemon.
- The token is compared byte-for-byte. Pick a long random string.
- Static assets are embedded into the binary at build time, so there is no
  need to open any additional ports for asset delivery.

## Related

- [`appctl chat`](/docs/cli/chat/) — CLI equivalent of the chat tab.
- [HTTP endpoints](/docs/api/http/) — exact schemas for the endpoints above.
- [WebSocket](/docs/api/websocket/) — event stream format.
- [Security](/docs/security/) — hardening guidance for shared deployments.
