---
title: Sync and schema
description: What appctl sync produces and how .appctl/schema.json is structured.
---

`appctl sync` is the only command that writes the schema. Every other command reads it.

## The schema file

Every project has a `.appctl/schema.json` after a successful sync. It looks like:

```json
{
  "version": 1,
  "source": "openapi",
  "base_url": "http://127.0.0.1:8000",
  "auth": { "kind": "none" },
  "resources": [
    {
      "name": "widgets",
      "actions": [
        {
          "name": "create_widget_widgets_post",
          "verb": "create",
          "transport": {
            "kind": "http",
            "method": "POST",
            "path": "/widgets"
          },
          "params_schema": { "type": "object", "properties": { "name": { "type": "string" } } },
          "safety": "mutation",
          "provenance": "declared"
        }
      ]
    }
  ]
}
```

The schema is deterministic for a given source. Re-running `sync` produces the same file unless the upstream changed.

## Fields every action has

- `name` — the tool name the LLM sees.
- `verb` — `list`, `get`, `create`, `update`, `delete`, or `action`.
- `transport` — how to call it. `http` with `method` + `path`, or `sql` with `table` + `operation`.
- `params_schema` — JSON Schema for arguments.
- `safety` — `read_only` or `mutation`.
- `provenance` — `declared`, `inferred`, or `verified`.

## Auth strategies

The `auth` block at the top of the schema tells the runtime how to authenticate:

- `none` — no auth.
- `bearer` — `Authorization: Bearer <env_ref>`.
- `api_key` — custom header (`header: "apikey"` for Supabase).
- `oauth_flow` — token stored via `appctl auth login`.

Override the inferred strategy at sync time with `--auth-header '<header>: <value>'`.

## Re-syncing and `--force`

`appctl sync` **rebuilds** the contract from the source. It does not merge with
an existing `schema.json`. A second run replaces that file and regenerates
`tools.json`, which is what you want after the backend changed — and **not**
what you want if you edited the JSON on purpose, ran the command in the wrong
directory, or a job overwrote a committed file. Hence `--force` for any
second write.

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json --base-url http://127.0.0.1:8000
# after the spec changes, same line plus --force
```

[More detail](/docs/cli/sync/#when-to-use-force) in the `sync` reference.

## Manual edits

The schema is plain JSON. You can:

- Rename a tool (update `name`).
- Narrow a parameter schema (add `required`, restrict `enum`).
- Remove a tool you do not want exposed.
- Add an `oauth_flow` after running `appctl auth login`.

Keep a copy in version control alongside your app. CI can re-sync and diff.

## SQL support tiers

`appctl sync --db` supports three SQL engines in depth today:

- Postgres
- MySQL
- SQLite

Other database engines are not supported by `sync --db` yet. In that case,
consider:

- sync from an OpenAPI layer in front of the database
- expose tools through MCP
- add a dynamic plugin for the engine-specific behavior you need

## Next

- [Tools and actions](/docs/concepts/tools-and-actions/)
- [`appctl sync`](/docs/cli/sync/)
