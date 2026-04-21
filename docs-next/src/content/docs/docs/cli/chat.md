---
title: appctl chat
description: Start an interactive chat session against the synced schema.
---

Start an interactive chat session. The agent has access to every tool in `.appctl/schema.json`.

## Usage

```
appctl chat [OPTIONS]
```

## Options

- `--provider <NAME>` — override the default provider from `.appctl/config.toml`.
- `--model <NAME>` — override the provider's default model.
- `--read-only` — block every mutation tool for this session.
- `--dry-run` — plan calls but do not execute them.
- `--confirm` — auto-approve mutations (default is to prompt).
- `--strict` — block `provenance=inferred` tools until verified by `appctl doctor --write`.

## Examples

```bash
# Default provider, interactive
appctl chat

# Read-only session with OpenAI
appctl chat --provider openai --read-only

# Dry-run to preview what the agent would do
appctl chat --dry-run
```

## Inside the session

Type a prompt and press Enter. The agent runs the tool loop and prints either a final answer or a tool-call trace.

Special commands:

- `:history` — show the last 20 entries.
- `:quit` or Ctrl-D — leave.

## Related

- [`appctl run`](/docs/cli/run/) — one-shot prompt.
- [`appctl serve`](/docs/cli/serve/) — run as a daemon for UIs.
- [Agent loop](/docs/concepts/agent-loop/)
