---
title: appctl chat
description: Start an interactive chat session against your synced schema.
---

`appctl chat` opens a REPL that gives the active LLM provider access to every
tool in `.appctl/tools.json`. The agent plans, calls tools, observes results,
and answers in your terminal with streamed events.

## Usage

```
appctl chat [OPTIONS]
```

The command reads `.appctl/config.toml` from the current `--app-dir` (default
`.appctl`) and loads the schema + tools produced by [`appctl sync`](/docs/cli/sync/).

## Options

| Flag | What it does |
| --- | --- |
| `--provider <NAME>` | Override `default` in `.appctl/config.toml` for this session. |
| `--model <NAME>` | Override the provider's `model` field. |
| `--session <NAME>` | Attach a human label to this chat session so history and the web UI show a stable name instead of an anonymous id. |
| `--read-only` | Reject any tool whose `op` is not a safe read. |
| `--dry-run` | Plan and stream events, but skip the real HTTP / SQL call. |
| `--confirm` | Auto-approve mutating calls (default is interactive confirm on TTY). |
| `--strict` | Block tools with `provenance = "inferred"` until `appctl doctor --write` marks them verified. |

Global flags (`--app-dir`, `--log-level`) work on every subcommand.

## The prompt

The prompt shows the app label and the active provider:

```text
appctl[app · gemini]▶
```

If you override the provider with `--provider openai`, the context updates
live to `appctl[app · openai]▶`.

## Slash commands

Slash commands are handled locally — they do **not** hit the model.

| Command | Effect |
| --- | --- |
| `/exit`, `/quit` | Leave the REPL (Ctrl-D works too). |
| `/read-only on` / `/read-only off` | Toggle `--read-only` mid-session. |
| `/dry-run on` / `/dry-run off` | Toggle `--dry-run` mid-session. |
| `/provider <name>` | Switch provider without restarting. |
| `/model <name>` | Switch model without restarting. |

Anything not starting with `/` is sent to the agent.

## What you see while the agent works

`appctl chat` streams structured [`AgentEvent`](/docs/api/agent-events/) frames
to the terminal renderer. For each user turn you typically see:

1. `plan` — the tool the model picked and its arguments.
2. `call` — the HTTP / SQL / MCP call the executor issued.
3. `observation` — the truncated response the model saw.
4. `final` — the assistant's natural-language answer.

Read-only and dry-run tools short-circuit before the live call.

## Examples

```bash
# Default provider, interactive
appctl chat

# Read-only session with OpenAI
appctl chat --provider openai --read-only

# Named session
appctl chat --session incident-123

# Preview what the agent would do, no live calls
appctl chat --dry-run

# Strict mode — inferred tools blocked until you verify them
appctl chat --strict
```

## Related

- [`appctl run`](/docs/cli/run/) — one-shot, non-interactive version.
- [`appctl serve`](/docs/cli/serve/) — the same agent loop over HTTP + WebSocket.
- [Agent loop](/docs/concepts/agent-loop/) — how plan / call / observation events are produced.
- [Provenance and safety](/docs/concepts/provenance-and-safety/) — what `--strict` actually blocks.
