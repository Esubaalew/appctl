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
| `--bind <ADDR>` | `127.0.0.1` | Interface to listen on. Use `0.0.0.0` only with `--token`. |
| `--port <N>` | `4242` | TCP port. |
| `--token <STRING>` | unset | Require this bearer token on every request. When set, the web UI prompts for it. |
| `--provider <NAME>` | — | Override the default provider for this server instance. |
| `--model <NAME>` | — | Override the provider's model. |
| `--read-only` | off | Block every mutating tool server-wide. |
| `--dry-run` | off | Skip real I/O; return simulated events. |
| `--strict` | off | Block `provenance = "inferred"` tools until verified. |
| `--confirm` | **on** | Auto-approve mutations. Default is on (non-interactive). Pass `--confirm=false` to require per-call approval from the web UI. |

Flags set on `appctl serve` apply to **every** request — a web UI client cannot
override them. Use this to enforce safety in shared deployments.

## The web console

Open `http://127.0.0.1:4242/` in a browser. The console ships as a single-page
app with four tabs:

- **Chat** — streaming conversation with the agent. Tool calls render inline
  as collapsible cards showing arguments and truncated responses.
- **Tools** — searchable list of every tool the agent can call, with its
  `kind`, `op`, safety level, and schema.
- **History** — the audit log (same table as `appctl history`), with
  expandable rows for arguments and raw response.
- **Settings** — provider status, token usage (if the provider reports
  billing info), and a field for the auth token when `--token` is set.

The UI connects over `WS /chat` for streaming; if WebSocket is blocked it
falls back to `POST /run` for non-streaming completions.

## HTTP endpoints

All endpoints honour `--token` (via `Authorization: Bearer ...` or
`x-appctl-token`) when set.

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

# Read-only, dry-run demo instance
appctl serve --read-only --dry-run

# Force a specific provider for a server that runs inside a CI job
appctl serve --provider openai --model gpt-4o-mini --confirm=false
```

## Security notes

- The bind address defaults to `127.0.0.1`. Changing it to `0.0.0.0` without
  also passing `--token` is a mistake — the server will still start, but
  anything on your network can use your provider credits.
- The token is compared byte-for-byte. Pick a long random string.
- Static assets are embedded into the binary at build time, so there is no
  need to open any additional ports for asset delivery.

## Related

- [`appctl chat`](/docs/cli/chat/) — CLI equivalent of the chat tab.
- [HTTP endpoints](/docs/api/http/) — exact schemas for the endpoints above.
- [WebSocket](/docs/api/websocket/) — event stream format.
- [Security](/docs/security/) — hardening guidance for shared deployments.
