---
title: appctl setup
description: Guided first-run setup for provider, sync source, checks, and next steps.
---

`appctl setup` is the recommended first command after installation. It guides
you through the normal appctl setup sequence without making you choose the exact
manual command up front. By default it inspects the current folder and suggests
likely sync sources before asking you to type URLs or paths.

## Usage

```bash
appctl setup
```

Run it from the project folder you want to control. If you use a **shared** app
directory at `~/.appctl`, config and tools still live there, but the automatic
**folder scan** uses your **current working directory** (so `cd` into the real
project first). For a self-contained app per repo, use a project-local
`.appctl` and `appctl --app-dir` instead.

Use `--app-dir` if you want
to point at a specific app directory:

```bash
appctl --app-dir /path/to/project/.appctl setup
```

## What it does

1. Configures an AI provider by reusing the same provider wizard as
   [`appctl init`](/docs/init/).
2. Inspects the folder for likely sources: OpenAPI/Swagger files, local SQLite
   databases, Django, Flask, Rails, Laravel, ASP.NET, and Strapi project markers.
3. Runs the matching `appctl sync` command when enough information is provided.
4. Asks only for missing values, such as a live base URL or protected-route auth
   header.
5. Runs `appctl doctor --write` for HTTP-like sources when possible.
6. Prints two next steps: `appctl chat` and `appctl serve --open`.

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
