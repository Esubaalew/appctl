---
title: appctl sync
description: Introspect your app and write .appctl/schema.json.
---

Introspect your app and write `.appctl/schema.json`.

## Usage

```
appctl sync [OPTIONS]
```

## Sources (pick exactly one)

| Flag | Value | Page |
| --- | --- | --- |
| `--openapi <URL\|file>` | OpenAPI 2/3 document | [OpenAPI](/docs/sources/openapi/) |
| `--django <dir>` | Django project folder | [Django](/docs/sources/django/) |
| `--flask <dir>` | Flask project folder | [SQL / datastores](/docs/sources/db/) |
| `--rails <dir>` | Rails project folder | [Rails](/docs/sources/rails/) |
| `--laravel <dir>` | Laravel project folder | [Laravel](/docs/sources/laravel/) |
| `--aspnet <dir>` | ASP.NET project folder | [ASP.NET](/docs/sources/aspnet/) |
| `--strapi <dir>` | Strapi v4 project folder | [Strapi](/docs/sources/strapi/) |
| `--supabase <URL>` | Supabase or bare PostgREST | [Supabase](/docs/sources/supabase/) |
| `--db <CONN>` | Postgres/MySQL/SQLite or MongoDB/Redis/Firestore/DynamoDB URL | [SQL / datastores](/docs/sources/db/) |
| `--url <URL>` | site root for URL login | [URL login](/docs/sources/url/) |
| `--mcp <URL>` | MCP server URL | [MCP](/docs/sources/mcp/) |
| `--plugin <NAME>` | dynamic plugin from `~/.appctl/plugins/` | [Plugins](/docs/sources/plugins/) |

## Common options

- `--base-url <URL>` — override the `base_url` written to the schema. Include any API mount prefix (for example `http://127.0.0.1:8001/api`).
- `--force` — allow overwriting an existing `schema.json` and regenerating `tools.json`. **Required** whenever a schema file is already on disk, except for the first sync in a new project. See [When to use `--force`](#when-to-use-force).
- `--watch` — keep polling an OpenAPI source and re-sync whenever the document changes.
- `--watch-interval-secs <N>` — polling interval for `--watch` (default `2`).
- `--doctor-write` — run `appctl doctor --write` immediately after a successful sync.
- `--auth-header '<Header>: <Value>'` — override the inferred auth strategy.
- `--supabase-anon-ref <NAME>` — name of the secret (keychain or env var) to use as the `apikey` header for Supabase.
- `--login-url`, `--login-user`, `--login-password`, `--login-form-selector` — URL login credentials and form hints.

### When to use `--force`

`sync` writes or refreshes two files in `.appctl/`:

| File | Role |
| --- | --- |
| `schema.json` | Resources, actions, auth. You can edit it. |
| `tools.json` | Flat list for the model; derived from the schema on each successful sync. |

If there is no `schema.json` yet, the first run only creates these files. If
`schema.json` already exists, another run would **replace** it and rebuild
`tools.json`. That is fine when the API or DB really changed, but it also
destroys any edits you made to the JSON, and a mistaken `sync` in the wrong
tree or a CI job could wipe a checked-in file. The CLI requires `--force` for
that overwrite.

Use `--force` when you are deliberately refreshing from the source: after API
or schema changes, in OpenAPI [watch mode](#examples) once the file exists, in
batch jobs, or with `appctl app add ... --openapi ...` when the app dir
[already has a contract](/docs/cli/app/). Omit it for the first sync in an
empty `.appctl/`.

`--force` is only about local files, not TLS or auth; use
[`--auth-header`](/docs/cli/sync#common-options) and friends for HTTP.

## Examples

```bash
# OpenAPI
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force

# Django
appctl sync --django . --base-url http://127.0.0.1:8001/api --force

# Flask
appctl sync --flask . --base-url http://127.0.0.1:5000 --force

# Postgres
appctl sync --db "postgres://reader:pass@db.acme.com:5432/shop" --force

# SQLite
appctl sync --db "sqlite:///tmp/shop.db" --force

# MongoDB
appctl sync --db "mongodb://127.0.0.1:27017/shop" --force

# Redis
appctl sync --db "redis://127.0.0.1:6379" --force

# OpenAPI watch mode
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --watch --doctor-write --force

# Supabase
appctl sync --supabase https://YOUR-PROJECT.supabase.co \
  --supabase-anon-ref supabase_anon --force
```

## Notes

- `--watch` currently supports OpenAPI sources.
- Firestore uses Google ADC at runtime. Run `gcloud auth application-default login` first.
- DynamoDB uses your normal AWS credential chain. For local DynamoDB, pass an endpoint in the URL such as `dynamodb://us-east-1?endpoint=http://127.0.0.1:8000`.

## Exit codes

- `0` — success; schema and tools were written.
- `1` — any failure, including: source unreachable, parse/introspection error, or **existing `schema.json` without `--force`** (the error text tells you to add `--force`).

## See also

- [Sync and schema](/docs/concepts/sync-and-schema/)
- [`appctl doctor`](/docs/cli/doctor/)
