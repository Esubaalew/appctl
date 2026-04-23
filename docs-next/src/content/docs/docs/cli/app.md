---
title: appctl app
description: Manage registered app contexts and the global active app.
---

`appctl app` manages the global registry of known apps so you never have to
type `--app-dir /long/path/.appctl` again. It works the same way
`kubectl config`, `aws` profiles, and `git` worktrees solve their own
context-switching problems.

## Usage

```bash
appctl app <COMMAND>
```

## Commands

| Command | What it does |
| --- | --- |
| `appctl app add [NAME]` | Register the current directory's `.appctl/` (or the one passed via `--app-dir`) under a name and mark it as the global active app. `NAME` defaults to the parent directory name. |
| `appctl app list` | Show every registered app, which one is active, and the absolute path to its `.appctl` directory. The active app is marked with `*`. |
| `appctl app use <NAME>` | Switch the globally active app by name. |
| `appctl app remove <NAME>` | Unregister an app. Files on disk are not touched. |

The registry lives at `~/.appctl/apps.toml`:

```toml
active = "backend"

[apps]
backend = "/Users/you/projects/api-backend/.appctl"
dashboard = "/Users/you/projects/admin-dashboard/.appctl"
```

## How appctl picks an app

Every command (`chat`, `run`, `sync`, `doctor`, `serve`, `history`, `config`,
`plugin`, `auth`, `mcp`) resolves the active app in this exact order and stops
at the first match:

1. `--app-dir <PATH>` — explicit per-command override.
2. **Auto-detect** — walks up from the current working directory looking for
   an `.appctl/` folder. If you are anywhere inside a project that was
   initialized with [`appctl init`](/docs/init/), it is picked automatically.
3. **Global active app** — `active` in `~/.appctl/apps.toml`.
4. Otherwise the command errors out with a hint to run `appctl init` or
   `appctl app use <name>`.

`appctl init` now offers to register the project globally as part of setup, so
for most projects you no longer need to run `appctl app add` manually unless
you skipped that prompt or want a custom name.

The resolved app label shows up in the chat prompt so you always know which
context you are in:

```text
appctl[backend · gemini]▶
```

## Typical workflow

Initialize two projects:

```bash
cd ~/projects/api-backend
appctl init
# answer "yes" when init asks to register globally

cd ~/projects/admin-dashboard
appctl init
# answer "yes" when init asks to register globally
```

Inspect the registry:

```bash
$ appctl app list
* backend   -> /Users/you/projects/api-backend/.appctl
  dashboard -> /Users/you/projects/admin-dashboard/.appctl
```

Switch globally and chat from anywhere:

```bash
cd ~
appctl app use dashboard
appctl chat   # uses dashboard
```

Or rely on auto-detect:

```bash
cd ~/projects/api-backend/src
appctl chat   # auto-detects backend, ignores the global active app
```

Explicit override for one command:

```bash
appctl --app-dir /tmp/experiment/.appctl chat
```

## Removing an app

`appctl app remove` only unregisters the app from `~/.appctl/apps.toml`. Your
`.appctl/` directory, history, and secrets are untouched. Delete the folder
manually if you want to wipe the app.

```bash
appctl app remove dashboard
```

## Related

- [`appctl init`](/docs/init/) — create a new `.appctl/` in the current folder.
- [`appctl chat`](/docs/cli/chat/) — the prompt that displays the active app.
- [`appctl config`](/docs/cli/config/) — inspect per-app configuration.
