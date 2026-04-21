---
title: Supabase / PostgREST
description: Introspect PostgREST OpenAPI. Works with Supabase and with raw PostgREST.
---

Works with any PostgREST-based API. `appctl` reads the OpenAPI document PostgREST serves and generates REST tools for each table.

## Prerequisites

- A Supabase project URL (like `https://xyz.supabase.co`) and the anon key, **or** a local PostgREST instance.
- `appctl` installed.

## Against a hosted Supabase project

```bash
export SUPABASE_ANON_KEY='eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...'

appctl sync --supabase https://YOUR-PROJECT.supabase.co \
  --anon-key "$SUPABASE_ANON_KEY" \
  --force
```

The sync probes `/rest/v1` first (the Supabase layout). If that 404s, it falls back to the root URL (bare PostgREST). One flag works for either deployment.

## The local demo in this repo

[`examples/demos/supabase/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/supabase) runs a bare PostgREST on top of Postgres with two tables (`items`, `notes`). It does not use Supabase's auth gateway, but the API shape is identical.

### 1. Start it

Docker is required.

```bash
cd examples/demos/supabase
docker compose up -d

sleep 10
curl -s http://localhost:3010/ | head -c 80
```

Real output:

```
{"swagger":"2.0","info":{"description":"","title":"standard public schema"...
```

### 2. Sync

```bash
appctl sync --supabase http://localhost:3010 --force
```

Real output:

```
Synced Supabase: 3 resources, 9 tools written to .appctl
```

Generated tools:

```
items: list_items GET /items, create_items POST /items,
       update_items PATCH /items, delete_items DELETE /items
notes: same four tools under /notes
```

### 3. Confirm they actually work

```bash
SUPABASE_ANON_KEY=anon appctl doctor
```

Expected:

```
tool                             method path    HTTP  verdict
list_items                       GET    /items   200  reachable
create_items                     POST   /items   200  reachable
update_items                     PATCH  /items   200  reachable
delete_items                     DELETE /items   200  reachable
list_notes                       GET    /notes   200  reachable
create_notes                     POST   /notes   200  reachable
...
```

### 4. Stop

```bash
docker compose down -v
```

## What appctl reads

- PostgREST serves its schema as OpenAPI 2 (Swagger) at `/` on a bare install, or `/rest/v1` behind Supabase. `appctl` probes both and picks whichever returns a valid document.
- It sets the `apikey` header from `SUPABASE_ANON_KEY` on every generated tool.

## PostgREST quirks

- There is no `get_{table}` tool (GET by id). PostgREST uses row filters (`?id=eq.1`) instead of path segments. The agent can call `list_items` with a query filter to get one row.
- PostgREST exposes its own OpenAPI endpoint at `GET /`. This becomes a tool called `list_introspection` in the schema. Harmless; you can ignore it.
- Row-level security is enforced by Postgres. Generated tools might sync without error but return empty results or a 401 at call time. That is RLS working correctly; it is not a bug in `appctl`.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [OpenAPI source](/docs/sources/openapi/)
- [`appctl doctor`](/docs/cli/doctor/)
