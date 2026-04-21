# supabase demo

A local PostgREST + PostgreSQL setup that mirrors how Supabase exposes tables over HTTP.
`appctl sync --supabase` talks to the PostgREST OpenAPI endpoint, the same endpoint that
Supabase cloud uses under the hood.

## What's here

```
docker-compose.yml   postgres:16 + postgrest:12 containers
init.sql             creates items and notes tables, grants anon role
.env.example         copy to .env and fill in SUPABASE_ANON_KEY
```

## Quick start (local, no Supabase account needed)

Requires Docker.

```sh
# 1. Start postgres + postgrest
make up

# 2. Sync appctl (no API key needed for the local anon role)
appctl sync --supabase http://localhost:3010 --force

# 3. Ask something
appctl chat "list all items"
```

PostgREST exposes its OpenAPI schema at `http://localhost:3010`. `appctl sync --supabase`
fetches that schema and generates CRUD tools for every table the anon role can access.

## Against real Supabase

```sh
# Copy .env.example → .env and fill in your anon key
cp .env.example .env
# Edit .env: SUPABASE_ANON_KEY=eyJ...

export SUPABASE_ANON_KEY=$(grep SUPABASE_ANON_KEY .env | cut -d= -f2)
appctl sync --supabase https://yourproject.supabase.co \
  --supabase-anon-ref SUPABASE_ANON_KEY --force
appctl chat "list all items"
```

## What appctl syncs

`appctl sync --supabase <url>` fetches the OpenAPI document at `<url>` (PostgREST serves
it at the REST root) and builds tools for each table operation. Authentication uses the
`apikey` header with the value from the env var named by `--supabase-anon-ref`.

## Known limits

- Row Level Security is not reflected in the tool list. If a table is locked by RLS and the
  anon key lacks access, calls will fail at runtime.
- Views and functions are not discovered by the default OpenAPI scan.
