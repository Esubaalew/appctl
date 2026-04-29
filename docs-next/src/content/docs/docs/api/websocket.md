---
title: WebSocket (/chat)
description: Bidirectional chat with streaming AgentEvents.
---

`WS /chat` is the streaming entry point. Send a prompt, receive a stream of [`AgentEvent`](/docs/api/agent-events/) frames until a final event.

## Connect

```
ws://127.0.0.1:4242/chat
wss://your.tls.terminator/chat
```

If `--token` is set, pass it as `?token=<TOKEN>` or in the `Authorization`
header during the upgrade request.

If the client is a browser, the `Origin` header must match the daemon host (or
forwarded host). This blocks cross-site pages from opening a socket to a local
daemon.

## Send

Each client message is a JSON text frame:

```json
{
  "message": "list parcels delivered today",
  "read_only": false,
  "dry_run": false,
  "confirm": true,
  "strict": false,
  "session_id": "optional-existing-session"
}
```

Only `message` is required. Safety fields can request stricter handling for this
turn, but they cannot relax a server-enforced mode. Send `session_id` to resume
the same in-process transcript used by `POST /run`.

## Receive

Each prompt with the same `session_id` shares one conversation transcript (same model context as a multi-turn [`appctl chat`](/docs/cli/chat/) run). If you omit `session_id`, the server creates one and emits it as a `session_state` event.

The server writes one JSON text frame per [`AgentEvent`](/docs/api/agent-events/). Examples:

```json
{"kind":"user_prompt","text":"list parcels delivered today"}
{"kind":"session_state","session_id":"...","transcript_len":0,"resumed":false}
{"kind":"tool_call","id":"call_01","name":"list_parcels","arguments":{"delivered":true}}
{"kind":"tool_result","id":"call_01","result":{"count":3},"status":"ok","duration_ms":120}
{"kind":"assistant_thought_delta","text":"Preparing summary..."}
{"kind":"assistant_delta","text":"Three parcels "}
{"kind":"assistant_delta","text":"were delivered today."}
{"kind":"assistant_thought","text":"Preparing summary..."}
{"kind":"assistant_message","text":"Three parcels were delivered today."}
{"kind":"done"}
```

After `done`, the server is ready for another message on the same connection.

## Minimal client

```js
const ws = new WebSocket(`ws://127.0.0.1:4242/chat?token=${token}`);
ws.onmessage = (e) => {
  const ev = JSON.parse(e.data);
  if (ev.kind === 'assistant_message') console.log('A:', ev.text);
  else if (ev.kind === 'assistant_delta') process.stdout.write(ev.text);
};
ws.onopen = () => ws.send(JSON.stringify({ message: 'hello' }));
```

## Errors

- `401 Unauthorized` — missing or wrong token during the HTTP upgrade.
- `403 Forbidden` — browser origin did not match the daemon host.
- Server-side tool errors appear as `{"kind":"tool_result","status":"error","result":"..."}` frames, not connection closes. Unrecoverable loop errors appear as `{"kind":"error","message":"..."}` followed by `{"kind":"done"}`.

## See also

- [HTTP endpoints](/docs/api/http/)
- [AgentEvent stream](/docs/api/agent-events/)
