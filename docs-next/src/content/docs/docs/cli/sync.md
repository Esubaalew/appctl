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
| `--rails <dir>` | Rails project folder | [Rails](/docs/sources/rails/) |
| `--laravel <dir>` | Laravel project folder | [Laravel](/docs/sources/laravel/) |
| `--aspnet <dir>` | ASP.NET project folder | [ASP.NET](/docs/sources/aspnet/) |
| `--strapi <dir>` | Strapi v4 project folder | [Strapi](/docs/sources/strapi/) |
| `--supabase <URL>` | Supabase or bare PostgREST | [Supabase](/docs/sources/supabase/) |
| `--db <CONN>` | Postgres/MySQL/SQLite URL | [SQL](/docs/sources/db/) |
| `--url <URL>` | site root for URL login | [URL login](/docs/sources/url/) |
| `--mcp <URL>` | MCP server URL | [MCP](/docs/sources/mcp/) |
| `--plugin <NAME>` | dynamic plugin from `~/.appctl/plugins/` | [Plugins](/docs/sources/plugins/) |

## Common options

- `--base-url <URL>` — override the `base_url` written to the schema. Include any API mount prefix (for example `http://127.0.0.1:8001/api`).
- `--force` — overwrite an existing `.appctl/schema.json` (required on re-sync).
- `--auth-header '<Header>: <Value>'` — override the inferred auth strategy.
- `--supabase-anon-ref <NAME>` — name of the secret (keychain or env var) to use as the `apikey` header for Supabase.
- `--login-url`, `--login-user`, `--login-password`, `--login-form-selector` — URL login credentials and form hints.

## Examples

```bash
# OpenAPI
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force

# Django
appctl sync --django . --base-url http://127.0.0.1:8001/api --force

# Postgres
appctl sync --db "postgres://reader:pass@db.acme.com:5432/shop" --force

# SQLite
appctl sync --db "sqlite:///tmp/shop.db" --force

# Supabase
appctl sync --supabase https://YOUR-PROJECT.supabase.co \
  --supabase-anon-ref supabase_anon --force
```

## Exit codes

- `0` — schema written.
- `1` — source unreachable or parse failure.
- `2` — schema already exists and `--force` was not passed.

## See also

- [Sync and schema](/docs/concepts/sync-and-schema/)
- [`appctl doctor`](/docs/cli/doctor/)
