---
title: AgentEvent stream
description: Every event emitted by the agent loop, with shape and meaning.
---

`appctl` emits structured `AgentEvent`s from the agent loop. They are the single source of truth for UIs (CLI renderer, VS Code panel, web UI) and for the [WebSocket](/docs/api/websocket/) and [`POST /run`](/docs/api/http/#post-run) transports.

## Serialization

Each event is a JSON object with a `kind` discriminator:

```json
{ "kind": "<variant>", /* fields */ }
```

## Variants

### `user_prompt`

Emitted once when the user submits a prompt.

```json
{ "kind": "user_prompt", "text": "list widgets" }
```

### `assistant_delta`

Incremental assistant text. Emitted by providers that support streaming; absent otherwise.

```json
{ "kind": "assistant_delta", "text": "Looking " }
```

### `assistant_message`

A complete assistant message. Always emitted at the end of an assistant turn.

```json
{ "kind": "assistant_message", "text": "Here are your widgets." }
```

### `tool_call`

The agent chose a tool and is about to call it.

```json
{
  "kind": "tool_call",
  "id": "call_01HV...",
  "name": "list_widgets",
  "arguments": { "limit": 10 }
}
```

`id` correlates with the matching `tool_result`.

### `tool_result`

The tool returned.

```json
{
  "kind": "tool_result",
  "id": "call_01HV...",
  "result": { "items": [ /* ... */ ] },
  "status": "ok",
  "duration_ms": 120
}
```

`status` is `ok` or `error`.

### `error`

Unrecoverable error during the loop.

```json
{ "kind": "error", "message": "max iterations reached" }
```

### `done`

Loop finished. No more events will be emitted for this turn.

```json
{ "kind": "done" }
```

## Ordering guarantees

- Exactly one `user_prompt` starts the stream.
- Zero or more `tool_call` / `tool_result` pairs interleave with `assistant_delta`/`assistant_message`.
- Every `tool_call` is followed by a `tool_result` with the same `id` (unless the loop errors first).
- Exactly one `done` terminates the stream.

## Consuming events

### From `POST /run`

The response body buffers every event in the `events` array plus a final `result` object. Good for synchronous callers.

### From `WS /chat`

Frames stream live. Good for UIs.

### From the CLI

`appctl chat` and `appctl run` use the same stream internally. The terminal renderer is in [`crates/appctl/src/term.rs`](https://github.com/Esubaalew/appctl/blob/main/crates/appctl/src/term.rs) if you want to see how they are formatted.

## See also

- [HTTP endpoints](/docs/api/http/)
- [WebSocket](/docs/api/websocket/)
- [Agent loop](/docs/concepts/agent-loop/)
