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

The setup flow starts by showing the exact `.appctl` directory it will write to.
For most users this should be a project-local folder such as `my-app/.appctl`.
`~/.appctl` is treated as a global app only when you explicitly use it from your
home directory or with `--app-dir`.

1. Confirm the app directory.
2. Choose or keep an AI provider.
3. Let appctl inspect the project and recommend a source, or choose OpenAPI /
   database / advanced manually.
4. Give target app access if needed: public API, bearer env var, cookie/session
   env var, OAuth browser login, or an existing target profile.
5. Sync tools, run checks, then print terminal and web next steps.

App access and model access are separate:

- **AI provider access** lets appctl talk to the language model.
- **Target app access** lets appctl call your API as a user or service account.
- **Serve token** protects the appctl web console when you share it.

For OAuth-backed target apps, login happens outside chat:

```bash
appctl auth target login esubalew --client-id <id> --auth-url <url> --token-url <url>
appctl auth target use esubalew
```

After that, tools use the stored token automatically. The AI only sees the
target profile name/status.

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
- Auth rejected: fix `[target] oauth_provider`, `[target] auth_header`, its env
  var, or the target API permissions, then rerun `appctl doctor --write`.

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
