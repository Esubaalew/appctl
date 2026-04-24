---
title: Troubleshooting
description: Common problems hit while syncing, calling tools, and running appctl serve.
---

Common issues and their fixes.

## `sync` refuses to overwrite

You see an error like:

```text
schema file already exists at /path/to/.appctl/schema.json (pass --force to replace…)
```

**Wrong directory:** `appctl` picks the **first** `.appctl` directory found walking **up** from your shell’s current path. If a **parent** folder (for example a repo or `~/` tree) already has a synced `schema.json`, you get this error even when the **current** subfolder has no local `.appctl`. Fix: run sync with an explicit app dir, e.g. `appctl sync --app-dir /absolute/path/to/this-api/.appctl --openapi …` (or `mkdir -p .appctl` in that project, then `appctl sync --app-dir .appctl …`).

**Why `--force`:** without it, the CLI will not replace an existing `schema.json` when you *do* mean to overwrite. Add `--force` when re-syncing the same app from the same source.

**Fix:** pass `--force` on the same `sync` line you use for your source:

```bash
appctl sync --openapi ... --force
```

For watch mode, include `--force` in the one long command, because the second and later re-syncs (when the OpenAPI document changes) are overwrites. See [the sync reference on when to use `--force`](/docs/cli/sync/#when-to-use-force).

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

Your PostgREST serves the OpenAPI document at `/` (bare PostgREST) but older builds only probed `/rest/v1` (hosted Supabase layout). Upgrade to 0.2.0 or newer. The sync now probes both layouts automatically.

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
appctl config set-secret ANTHROPIC_API_KEY --value "$ANTHROPIC_API_KEY"
```

Or set the env var for the current session:

```bash
export ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY"
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

`appctl sync --mcp` expects a URL. If your MCP server is stdio-only, wrap it with an HTTP or WebSocket bridge. The `examples/demos/mcp-stdio/` example shows the protocol, but it is not a direct target for `sync --mcp`.

## Docker-required demos do not start

The `db-postgres` and `supabase` demos need a running Docker daemon. Start Docker Desktop (or `systemctl start docker` on Linux) before `docker compose up -d`.

## Linux: secret-service not available

```
error: failed to unlock keychain
```

The keychain needs `secret-service` running (GNOME Keyring or KWallet). On headless servers, use environment variables instead:

```bash
export ANTHROPIC_API_KEY="$ANTHROPIC_API_KEY"
```

## Still stuck?

- Run with `--log-level debug` for verbose logs.
- Check `.appctl/history.db` for the last tool call's arguments and status.
- File an issue with the commands, the schema snippet, and the error: [github.com/Esubaalew/appctl/issues](https://github.com/Esubaalew/appctl/issues).
