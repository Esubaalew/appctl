---
title: appctl setup
description: Guided first-run setup for provider, sync source, checks, and next steps.
---

`appctl setup` is the recommended first command after installation. It is the
public onboarding path: confirm the app directory, choose or keep an AI provider,
connect appctl to your API/database/app, verify access, then start chat or the
web console.

The goal is that a normal user does not need to understand `init`, `sync`,
`doctor`, config files, or the global registry before the first useful run.

## Usage

```bash
appctl setup
```

Run it from the project folder you want to control. Setup prints the exact
`.appctl` directory, `config.toml`, and `tools.json` it will use before it asks
provider or API questions. A project-local `.appctl` is the default mental
model. `~/.appctl` is a global app only when you run from `$HOME` or pass it
explicitly with `--app-dir`.

Use `--app-dir` if you want
to point at a specific app directory:

```bash
appctl --app-dir /path/to/project/.appctl setup
```

## What it does

1. Shows the app context and where files will be written.
2. Configures or keeps the AI provider. This is only for talking to the model.
3. Recommends app sources from the project. The first menu stays short:
   inspect, OpenAPI, database, manual/advanced, or skip.
4. Asks for target app access for HTTP-like sources: public API, bearer env var,
   cookie/session env var, OAuth browser login, or an existing target profile.
5. Syncs tools and runs `doctor` checks for HTTP-like sources.
6. Prints next steps: `appctl chat` and `appctl serve --open`.

## Three kinds of auth

| Auth | Purpose | Where to configure |
| --- | --- | --- |
| AI provider auth | Lets appctl call the model | `appctl setup` / `appctl init` provider step |
| Target app auth | Lets tools call your app/API as a user or service account | `appctl setup`, `[target] auth_header`, `appctl auth target login/use` |
| Serve token | Lets browsers/users access the appctl web console | `appctl serve --token ...` |

The model should not collect passwords, tokens, client secrets, or cookies in
chat. If a tool returns `401`/`403`, fix target app auth outside the chat and
rerun `appctl doctor --write`.

For OAuth-backed apps, use a target profile:

```bash
appctl auth target login esubalew --client-id <id> --auth-url <url> --token-url <url>
appctl auth target use esubalew
```

The token is stored in the OS keychain. The model only sees that appctl is using
profile `esubalew`; it does not see the token, password, cookie, or client
secret.

If no source is obvious, setup explains what was missing and lets you choose a
source manually.

## When to use manual commands instead

Use manual commands when you are scripting or already know the exact source:

```bash
appctl init
appctl sync --openapi <url-or-file> --base-url <running-api-url>
appctl doctor --write
appctl chat
```

For databases:

```bash
appctl init
appctl sync --db "sqlite:///absolute/path/app.db"
appctl chat
```

## Related

- [First 10 minutes](/docs/first-10-minutes/)
- [`appctl init`](/docs/init/)
- [`appctl sync`](/docs/cli/sync/)
- [`appctl serve`](/docs/cli/serve/)
