---
title: Tools and actions
description: How resources map to actions, and how each action is dispatched.
---

A resource groups a set of actions. Each action is one tool the LLM can call.

## Vocabulary

- **Resource**: a domain object in your app (`users`, `orders`, `widgets`).
- **Action**: one operation on a resource (`list`, `create`, `delete`).
- **Tool**: the action as the LLM sees it, with a name and a JSON Schema for arguments.

## Standard verbs

Most sources produce the same five actions per resource:

| Verb | HTTP | Safety |
| --- | --- | --- |
| `list` | `GET /items` | read_only |
| `get` | `GET /items/{id}` | read_only |
| `create` | `POST /items` | mutation |
| `update` | `PATCH /items/{id}` | mutation |
| `delete` | `DELETE /items/{id}` | mutation |

OpenAPI-based sources may produce more (for example, `POST /items/{id}/archive` becomes an action with `verb: action`).

## Transport kinds

### HTTP

```json
"transport": {
  "kind": "http",
  "method": "POST",
  "path": "/widgets"
}
```

The runtime substitutes path parameters from the call arguments and sends the body as JSON.

### SQL

```json
"transport": {
  "kind": "sql",
  "database_kind": "postgres",
  "table": "widgets",
  "operation": "select",
  "primary_key": "id"
}
```

The runtime executes a prepared statement. No free-form SQL is sent; the LLM never writes raw SQL.

### MCP

```json
"transport": {
  "kind": "mcp",
  "url": "ws://localhost:5555/mcp",
  "tool": "search_tickets"
}
```

The runtime opens a WebSocket to the MCP server and calls `tools/call`.

## Safety

`safety: "read_only"` means the tool does not mutate state. It is always allowed.

`safety: "mutation"` means it might. In CLI mode, `appctl` prompts before every mutation unless `--confirm` is set. In `appctl serve`, mutations auto-approve by default.

The global `--read-only` flag blocks every mutation tool regardless of configuration.

## Naming

By default action names combine the verb and the resource: `list_widgets`, `create_order`. You can rename any tool by editing `.appctl/schema.json`. The LLM sees whatever is in `name`.

## Next

- [Agent loop](/docs/concepts/agent-loop/)
- [Provenance and safety](/docs/concepts/provenance-and-safety/)
