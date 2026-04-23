---
title: appctl run
description: Execute a single prompt and exit. Non-interactive, pipeline-friendly.
---

`appctl run` is `appctl chat` in one-shot form. It loads the same schema,
tools, and provider, runs the full plan-call-observe loop for one prompt, then
exits with the final answer on stdout.

Use it for scripts, CI jobs, and Makefile targets where you do not want a REPL.

## Usage

```
appctl run [OPTIONS] <PROMPT>
```

`<PROMPT>` is a single positional argument. Quote it or it will be split on
whitespace by your shell.

## Options

| Flag | What it does |
| --- | --- |
| `--provider <NAME>` | Override the `default` provider for this invocation. |
| `--model <NAME>` | Override the provider's model. |
| `--read-only` | Block every mutating tool. |
| `--dry-run` | Stream events, skip real I/O. |
| `--confirm` | Auto-approve mutations (on by default in non-TTY mode, off in TTY). |
| `--strict` | Require `provenance = "verified"` on every tool. |

## Exit codes

- `0` — the agent produced a final answer.
- `1` — an error was reported (bad config, provider failure, tool error, or a
  safety mode refused a call).

## Examples

```bash
# Read-only prompt, print the answer
appctl run --read-only "How many active users signed up this week?"

# Force OpenAI for this one call
appctl run --provider openai "Create a test user"

# Dry-run a destructive request — no HTTP call is issued
appctl run --dry-run "Delete all orders from staging"

# Use in a CI step
appctl run --confirm --read-only "Summarize the last 20 support tickets" \
  > summary.md
```

## Streaming output

`appctl run` uses the same event renderer as `appctl chat`. The final answer is
the last block printed; intermediate plan / call / observation frames go to the
same terminal stream. Pipe the command to a file to capture them.

To get only the final answer without any tool trace, prefer [`appctl
serve`](/docs/cli/serve/)'s `POST /run` endpoint, which returns a single JSON
response.

## Related

- [`appctl chat`](/docs/cli/chat/) — interactive REPL variant.
- [`appctl serve`](/docs/cli/serve/) — HTTP / WebSocket form for apps.
- [Agent loop](/docs/concepts/agent-loop/)
