---
title: Datastores (SQL, MongoDB, Redis, Firestore, DynamoDB)
description: Skip the API. Point at a datastore URL and get CRUD tools for tables, collections, keys, or documents.
---

Skip the API. Point `appctl` at a datastore URL and it gives the agent CRUD
tools directly against tables, collections, keys, or documents.

## Prerequisites

- A datastore connection string.
- `appctl` installed.

## Run it

```bash
appctl sync --db "postgres://reader:pass@db.acme.com:5432/shop" --force

# or
appctl sync --db "mysql://reader:pass@db.acme.com:3306/shop" --force

# or
appctl sync --db "sqlite:///Users/you/dev/shop.db" --force

# or
appctl sync --db "mongodb://127.0.0.1:27017/shop" --force

# or
appctl sync --db "redis://127.0.0.1:6379" --force

# or
appctl sync --db "firestore://my-gcp-project" --force

# or
appctl sync --db "dynamodb://us-east-1" --force
```

For every supported backend, the sync produces the same five logical tools:
`list_*`, `get_*`, `create_*`, `update_*`, and `delete_*`.

- **SQL** sources generate typed CRUD tools per table and execute them as prepared statements. Table and column names that clash with **reserved words** (for example SQLite’s `order`, `user`, or `group`) are **quoted** in generated SQL so `list_order` and similar tools work.
- **MongoDB** generates CRUD tools per collection, keyed by `_id`.
- **Redis** generates a generic `redis_key` resource backed by key/value operations.
- **Firestore** generates CRUD tools per top-level collection and uses Google ADC at runtime.
- **DynamoDB** generates CRUD tools per table and uses the normal AWS credential chain.

## The local demo in this repo

[`examples/demos/db-postgres/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/db-postgres) starts Postgres 16 in Docker with a single `widgets` table and a seed row.

### 1. Start it

```bash
cd examples/demos/db-postgres
docker compose up -d
sleep 4
docker exec db-postgres-db-1 psql -U appctl -d appctl_demo -c "SELECT * FROM widgets;"
```

Real output:

```
 id | name |          created_at
----+------+-------------------------------
  1 | demo | 2026-04-21 11:34:11.526835+00
(1 row)
```

### 2. Sync

```bash
appctl sync --db "postgres://appctl:appctl@127.0.0.1:5433/appctl_demo" --force
```

Real output:

```
Synced Db: 1 resources, 5 tools written to .appctl
```

Generated tools:

```
widget: list_widgets, get_widget, create_widget, update_widget, delete_widget
```

### 3. Inspect a generated tool

```json
{
  "name": "list_widgets",
  "verb": "list",
  "transport": {
    "kind": "sql",
    "database_kind": "postgres",
    "table": "widgets",
    "operation": "select",
    "primary_key": "id"
  },
  "safety": "read_only",
  "provenance": "declared"
}
```

### 4. Stop

```bash
docker compose down -v
```

## Auth notes

- **Firestore**: run `gcloud auth application-default login` first. `appctl` uses Google ADC to call the Firestore REST API.
- **DynamoDB**: export AWS credentials, use an AWS profile, or point at local DynamoDB with `dynamodb://us-east-1?endpoint=http://127.0.0.1:8000`.
- **MongoDB / Redis**: standard URI auth works through the connection string.

## Staying safe

- **Use a read-only account.** Create a Postgres role with only `SELECT` on the tables you want to expose.
- **SQLite is local by nature.** Prefer a copy of the DB for experiments if you do not want agent mutations to hit your live file.
- **Or run with `--read-only`.** That flag blocks the `create_*`, `update_*`, and `delete_*` tools at the `appctl` layer even if your DB role could perform them.

## Known limits

- The SQL tools use prepared statements against the table and primary key. There is no free-form raw SQL tool in this source.
- Only single-column primary keys are modeled today.
- Views and materialized views are not introspected.
- Stored procedures are not auto-registered.
- Multi-schema databases only expose the default schema unless the connection string selects one.
- Redis support is generic key/value access, not a schema-aware hash/stream model.
- Firestore and DynamoDB use document/item payloads rather than per-column typing.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [Provenance and safety](/docs/concepts/provenance-and-safety/)
