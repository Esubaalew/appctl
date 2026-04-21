---
title: SQL databases (Postgres, MySQL)
description: Skip the API. Point at a database URL and get SQL tools per table.
---

Skip the API. Point `appctl` at a Postgres or MySQL URL and it gives the agent SQL tools for your tables, executed as prepared statements.

## Prerequisites

- A Postgres or MySQL connection string. A read-only account is strongly recommended.
- `appctl` installed.

## Run it

```bash
appctl sync --db "postgres://reader:pass@db.acme.com:5432/shop" --force

# or
appctl sync --db "mysql://reader:pass@db.acme.com:3306/shop" --force
```

For each table, the sync produces five tools: `list_{table}`, `get_{table}`, `create_{table}`, `update_{table}`, `delete_{table}`. Each tool uses a prepared statement against the table and primary key. There is no free-form SQL tool.

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

## Staying safe

- **Use a read-only account.** Create a Postgres role with only `SELECT` on the tables you want to expose.
- **Or run with `--read-only`.** That flag blocks the `create_*`, `update_*`, and `delete_*` tools at the `appctl` layer even if your DB role could perform them.

## Known limits

- The SQL tools use prepared statements against the table and primary key. There is no free-form raw SQL tool in this source.
- Views and materialized views are not introspected.
- Stored procedures are not auto-registered.
- Multi-schema databases only expose the default schema unless the connection string selects one.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [Provenance and safety](/docs/concepts/provenance-and-safety/)
