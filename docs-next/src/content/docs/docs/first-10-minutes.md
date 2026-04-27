---
title: First 10 minutes
description: The simplest path from install to a working appctl chat or web console.
---

This is the happy path. You do not need to understand every config file before
you start.

## 1. Install

```bash
cargo install appctl
appctl --version
```

## 2. Run the guided setup

From the project folder you want appctl to control:

```bash
appctl setup
```

The setup flow inspects a folder first, asks simple questions, and then does
the normal appctl steps for you. If you use `~/.appctl` as a shared app, the
scan uses your **current directory** (run `cd` to your project first) — or use
`--app-dir path/to/project/.appctl` for a project-only app.

1. Choose an AI provider and store credentials safely.
2. Let appctl suggest likely sources from the folder, such as OpenAPI files,
   local SQLite databases, Django, Flask, Rails, Laravel, ASP.NET, or Strapi.
3. Sync tools into `.appctl/schema.json` and `.appctl/tools.json`.
4. Run `doctor` checks when the source has a live HTTP base URL.
5. Print the two next commands: terminal chat or web mode.

## 3. Chat in the terminal

```bash
appctl chat
```

Try a small read-only question first, for example:

```text
What tools do you have, and what safe first action can you take?
```

## 4. Or use the web console

```bash
appctl serve --open
```

The web console shows setup status, available tools, chat, history, and settings.
If provider or sync setup is missing, it shows the same checklist as the terminal
flow.

## If setup stops

Read the first “Run this next” line and copy that command. Most setup failures
mean one of these:

- No provider yet: run `appctl setup`.
- No tools yet: run `appctl setup`; it will inspect the folder and suggest a
  source, or you can choose one manually.
- Existing tools would be replaced: rerun sync with `--force` only when you
  really want to regenerate `.appctl/schema.json`.
- Wrong project: pass `--app-dir /path/to/.appctl`.

## Advanced manual path

If you already know your provider and source, the guided flow is just shorthand
for:

```bash
appctl init
appctl sync --openapi <url-or-file> --base-url <running-api-url>
appctl doctor --write
appctl chat
```

For databases:

```bash
appctl init
appctl sync --db "postgres://user:pass@host:5432/db"
appctl chat
```

Next: [Installation](/docs/installation/) · [Choosing a sync source](/docs/sources/choosing-a-sync-source/) · [Web console](/docs/cli/serve/)
