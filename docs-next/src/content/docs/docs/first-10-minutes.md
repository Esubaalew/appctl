---
title: First 10 minutes
description: The simplest path from install to a working appctl chat or web console.
---

Start here if you want a working terminal chat or web console without editing
`.appctl/config.toml` by hand.

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

If your HTTP API needs credentials, configure target auth outside chat. Pick the
shape your API actually uses:

```bash
# Header API key
appctl auth target set-header 'X-Api-Key: env:API_KEY'

# Bearer token
appctl auth target set-bearer --env API_TOKEN

# Cookie/session
appctl auth target set-header 'Cookie: env:APP_SESSION_COOKIE'

# OAuth/OIDC browser login
appctl auth target login api --client-id <id> --auth-url <url> --token-url <url>
```

You can also set a header while syncing:

```bash
export API_TOKEN='...'

appctl sync \
  --openapi https://api.example.com/openapi.json \
  --base-url https://api.example.com \
  --auth-header 'Authorization: Bearer env:API_TOKEN' \
  --force
```

Changing credentials later does not require sync:

```bash
export API_TOKEN='new-token'
appctl auth target set-bearer --env API_TOKEN
appctl doctor --write
```

Use `--force` only when `.appctl/schema.json` already exists and you intend to
regenerate the tools. See [`appctl sync`](/docs/cli/sync/) and
[OpenAPI](/docs/sources/openapi/) for details.

For databases:

```bash
appctl init
appctl sync --db "postgres://user:pass@host:5432/db"
appctl chat
```

Next: [Installation](/docs/installation/) · [Choosing a sync source](/docs/sources/choosing-a-sync-source/) · [Web console](/docs/cli/serve/)
