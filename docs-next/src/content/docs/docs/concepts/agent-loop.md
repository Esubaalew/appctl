---
title: Agent loop
description: How appctl drives the LLM between user input and a final response.
---

The agent loop runs inside `appctl chat`, `appctl run`, and `appctl serve`. Each iteration either calls a tool or returns a final message.

## Steps per iteration

1. **Compose context.** The runtime builds a request with:
   - System prompt (project description + current schema summary).
   - Conversation history (capped by `behavior.history_limit`).
   - Tool definitions derived from `.appctl/schema.json`.
2. **Call the LLM.** Uses the provider configured in `.appctl/config.toml` (or `--provider` override).
3. **Branch on response.**
   - If the LLM returns a tool call, the runtime executes it, captures the result, and loops.
   - If the LLM returns a final message, the loop stops.
4. **Emit AgentEvents.** Every step produces an event (`tool_call_start`, `tool_call_end`, `llm_chunk`, `final`). Consumers (VS Code, web UI, `serve` clients) render them in real time.

## Iteration cap

`behavior.max_iterations` (default 8) bounds the loop. If it is hit, the runtime returns an explicit `max_iterations_reached` event instead of silently stopping.

## Safety gates

Between steps, the runtime applies:

- `--read-only`: drops mutation tools from the definitions list entirely.
- `--dry-run`: returns a synthetic response describing what would have happened.
- `--confirm` (CLI): prompts y/n before each mutation.
- `--strict`: blocks tools with `provenance: "inferred"` until doctor has verified them.

## Where AgentEvents come from

See [AgentEvent stream](/docs/api/agent-events/) for the full list. Each event is:

```json
{
  "type": "tool_call_start",
  "tool": "create_widget",
  "args": { "name": "Demo" },
  "ts": "2026-04-21T12:30:45Z"
}
```

The stream is append-only and replayable.

## History

Every loop writes to `.appctl/history.db` (SQLite). Inspect with:

```bash
appctl history --last 20
```

## Next

- [Provenance and safety](/docs/concepts/provenance-and-safety/)
- [AgentEvent stream](/docs/api/agent-events/)
