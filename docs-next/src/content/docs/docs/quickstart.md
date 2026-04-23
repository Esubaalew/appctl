---
title: Quickstart
description: Run appctl against a demo FastAPI app end-to-end in five minutes.
---

Five minutes, one real app, real output. We will use the FastAPI demo in the repo. Any OpenAPI-capable app works the same way.

## Prerequisites

- [`appctl` installed](/docs/installation/).
- Python 3.11 or newer.
- A configured LLM provider. The fastest way is the interactive wizard:

  ```bash
  appctl init
  ```

  See [`appctl init`](/docs/init/) for the full walkthrough of what it writes
  and where secrets are stored. If you say yes to the final registration
  prompt, the demo app is also added to `~/.appctl/apps.toml` so later
  `appctl chat` / `appctl serve` commands can find it globally.

## 1. Clone and start the demo

```bash
git clone https://github.com/Esubaalew/appctl.git
cd appctl/examples/demos/openapi-fastapi
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --host 127.0.0.1 --port 8000 &
```

Confirm the OpenAPI document is being served:

```bash
curl -s http://127.0.0.1:8000/openapi.json | head -c 60
```

Output:

```
{"openapi":"3.1.0","info":{"title":"appctl demo API","ve
```

## 2. Sync

In the same folder, point `appctl` at the live document:

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force
```

Current output shape:

```
Sync complete
✔ Openapi: 1 resources, 1 tools written under .appctl
```

A new `.appctl/schema.json` describes the generated tool and `.appctl/tools.json`
holds the flattened tool list the model sees.

## 3. Verify the tool is reachable

```bash
appctl doctor
```

Output:

```
tool                             method path         HTTP  verdict
create_widget_widgets_post       POST   /widgets      200  reachable
```

## 4. Talk to it

For a one-shot answer:

```bash
appctl run "create a widget named Demo"
```

Or get structured output for scripts:

```bash
appctl run --json "create a widget named Demo"
```

Or open the interactive REPL:

```bash
appctl chat
# appctl[app · openai]▶ create a widget named Demo
```

The agent picks `create_widget_widgets_post`, calls it, and prints the
response. Every call is logged to `.appctl/history.db`.

```bash
appctl history --last 5
```

## 5. Run as a daemon (optional)

For VS Code, the web UI, or custom frontends:

```bash
appctl serve --port 4242
```

HTTP endpoints and WebSocket events are documented in [API](/docs/api/http/).

## Clean up

```bash
kill %1      # stop uvicorn
deactivate   # exit venv
```

## What just happened

1. FastAPI generated an OpenAPI document.
2. `appctl sync` turned each operation into a typed tool with JSON Schema params and marked it `provenance=declared`.
3. `appctl doctor` probed the live server to confirm each tool returns something sensible.
4. `appctl chat` fed the tool contract to your configured LLM, which picked the right one and filled in the arguments.

## Where to go next

- [Pick another source](/docs/sources/openapi/) for your real project.
- [Understand provenance](/docs/concepts/provenance-and-safety/) before deploying.
- [Embed in a server](/docs/deploy/server/) when you are ready to share.
