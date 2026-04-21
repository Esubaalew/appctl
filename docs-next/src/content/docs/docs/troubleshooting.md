---
title: Troubleshooting
description: Common problems hit while syncing, calling tools, and running appctl serve.
---

Real problems seen while building and testing the demos.

## `sync` refuses to overwrite

```
error: .appctl/schema.json already exists (pass --force)
```

Pass `--force`:

```bash
appctl sync --openapi ... --force
```

## Django tools hit the wrong URL

```
appctl doctor
list_parcels  GET /parcels/    404 not_found
```

The sync wrote paths as `/parcels/` but your Django app mounts the API under `/api/`. Include the prefix in `--base-url`:

```bash
appctl sync --django . --base-url http://127.0.0.1:8001/api --force
```

## `appctl doctor` says "invalid port number"

You are on a pre-0.2.0 build. Upgrade. The fix is in [`doctor.rs`](https://github.com/Esubaalew/appctl/blob/main/crates/appctl/src/doctor.rs): non-root paths now get a slash between base URL and path.

## Supabase sync "document missing paths object"

Your PostgREST serves the OpenAPI document at `/` (bare PostgREST) but the old sync only probed `/rest/v1` (hosted Supabase layout). Upgrade to 0.2.0+ — the sync now probes both layouts automatically.

## Rails tools use singular paths

`/api/v1/post` instead of `/api/v1/posts`. Fixed in 0.2.0. Upgrade and re-sync with `--force`.

## `appctl chat` cannot find the provider

```
error: no provider named "claude"
```

Either the provider is not in `.appctl/config.toml`, or the default in `config.toml` points at a name that does not exist. Inspect:

```bash
appctl config show
```

## "API key not found for claude"

The `api_key_ref` in your config does not match any keychain entry or environment variable. Store the key:

```bash
appctl config set-secret anthropic --value "$ANTHROPIC_API_KEY"
```

Or set the env var for the current session:

```bash
export anthropic="$ANTHROPIC_API_KEY"
```

## `appctl serve` returns `401 Unauthorized`

You started serve with `--token` but the client is not sending it. Send one of:

- `Authorization: Bearer <TOKEN>`
- `X-Appctl-Token: <TOKEN>`
- `?token=<TOKEN>` (WebSocket only)

## URL login produced 0 tools

```
Synced Url: 0 resources, 0 tools written to .appctl
```

The login worked but the post-login page had no discoverable structure (no forms, no tables, no action links). This source only finds surface the crawler can understand. For SPAs or custom UIs, sync the backing API with `--openapi` instead.

## MCP sync expects a URL

`appctl sync --mcp` expects a URL. If your MCP server is stdio-only, wrap it with an HTTP or WebSocket bridge. The `examples/demos/mcp-stdio/` server is for protocol verification, not for `sync --mcp`.

## Docker-required demos do not start

The `db-postgres` and `supabase` demos need a running Docker daemon. Start Docker Desktop (or `systemctl start docker` on Linux) before `docker compose up -d`.

## Linux: secret-service not available

```
error: failed to unlock keychain
```

The keychain needs `secret-service` running (GNOME Keyring or KWallet). On headless servers, use environment variables instead:

```bash
export anthropic="$ANTHROPIC_API_KEY"
```

## Still stuck?

- Run with `--log-level debug` for verbose logs.
- Check `.appctl/history.db` for the last tool call's arguments and status.
- File an issue with the commands, the schema snippet, and the error: [github.com/Esubaalew/appctl/issues](https://github.com/Esubaalew/appctl/issues).
