---
title: HTTP endpoints
description: Every HTTP route exposed by appctl serve, with payloads and auth.
---

Every route exposed by [`appctl serve`](/docs/cli/serve/). Default bind is `127.0.0.1:4242`.

## Authentication

If `--token <TOKEN>` was passed, every request must carry one of:

- `Authorization: Bearer <TOKEN>`
- `Authorization: <TOKEN>` (raw)
- `X-Appctl-Token: <TOKEN>`
- `?token=<TOKEN>` query parameter (WebSocket only)

Without `--token`, auth is disabled. Do not run this on a public interface without a token.

## `GET /schema`

Returns the current `schema.json` verbatim.

```bash
curl -s http://127.0.0.1:4242/schema -H "Authorization: Bearer $T" | jq '.resources[0]'
```

## `GET /tools`

Returns the derived tool list the agent sees. This is `schema.json` flattened into a list of tools with resolved types.

## `GET /config/public`

Returns non-secret parts of the active configuration:

```json
{
  "default_provider": "ollama",
  "sync_source": "openapi",
  "base_url": "http://127.0.0.1:8000",
  "read_only": false,
  "dry_run": false,
  "strict": false,
  "confirm_default": true
}
```

No API keys, no tokens.

## `GET /history`

List past tool calls.

Query parameters:

- `limit` — max rows (default `20`).

```bash
curl -s "http://127.0.0.1:4242/history?limit=5" -H "Authorization: Bearer $T"
```

## `POST /run`

Run one prompt and return the final response plus the full event trail.

Request:

```json
{
  "message": "create a widget named Demo",
  "read_only": false,
  "dry_run": false,
  "confirm": true,
  "strict": false
}
```

Response:

```json
{
  "result": "Created widget #1",
  "events": [
    { "kind": "user_prompt", "text": "create a widget named Demo" },
    { "kind": "tool_call", "id": "call_01", "name": "create_widget", "arguments": {"name":"Demo"} },
    { "kind": "tool_result", "id": "call_01", "result": {"id":1,"name":"Demo"}, "status": "ok", "duration_ms": 42 },
    { "kind": "assistant_message", "text": "Created widget #1" },
    { "kind": "done" }
  ]
}
```

Each event is an [`AgentEvent`](/docs/api/agent-events/).

## `WS /chat`

Bidirectional streaming chat. See [WebSocket](/docs/api/websocket/).

## Static UI

Any other path returns the bundled web UI (single-page app). The SPA reads the routes above.

## Errors

- `401 Unauthorized` — missing or wrong token.
- `404 Not Found` — unknown path and no SPA fallback.
- `500 Internal Server Error` — unexpected server-side failure. Message is in the body.

## See also

- [WebSocket](/docs/api/websocket/)
- [AgentEvent stream](/docs/api/agent-events/)
- [`appctl serve`](/docs/cli/serve/)
