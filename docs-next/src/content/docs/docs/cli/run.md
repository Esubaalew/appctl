---
title: appctl run
description: Run a single prompt non-interactively.
---

Run one prompt, print the result, exit. Useful for scripts, CI, and cron.

## Usage

```
appctl run [OPTIONS] <PROMPT>
```

## Options

Same as [`appctl chat`](/docs/cli/chat/):

- `--provider <NAME>`, `--model <NAME>`
- `--read-only`, `--dry-run`, `--confirm`, `--strict`

## Examples

```bash
appctl run "list all parcels delivered today"
appctl run --read-only --strict "describe the schema"
appctl run --provider openai --model gpt-4o "create a widget named Demo"
```

## Output

Stdout receives the final agent message. Tool traces go to stderr so you can redirect them separately:

```bash
appctl run "..." 2> trace.log
```

Exit code is `0` on success, non-zero if the agent errored or hit `max_iterations`.

## Related

- [`appctl chat`](/docs/cli/chat/) — interactive.
- [Agent loop](/docs/concepts/agent-loop/)
