---
title: Quickstart
description: End-to-end flow using the OpenAPI + FastAPI demo in this repository.
---

This walkthrough uses `examples/demos/openapi-fastapi`. The same `sync` flow
applies to any app that exposes an OpenAPI document.

## Prerequisites

- [`appctl` installed](/docs/installation/)
- Python 3.11+
- For the simplest setup, run [`appctl setup`](/docs/cli/setup/) from the demo folder after starting the app. The manual commands below are for users who want each step shown explicitly.

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

## 2. Guided setup or manual sync

Recommended:

```bash
appctl setup
```

Choose “OpenAPI document,” then enter:

- OpenAPI URL: `http://127.0.0.1:8000/openapi.json`
- Base URL: `http://127.0.0.1:8000`

Manual equivalent:

In the same folder, point `appctl` at the live document:

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force
```

Use `--force` if this directory already has a `schema.json` (e.g. you ran
this before or copied the folder). [Details](/docs/cli/sync/#when-to-use-force).

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

```bash
appctl serve --port 4242
```

See [HTTP API](/docs/api/http/) for routes and WebSocket events.

## Clean up

```bash
kill %1      # stop uvicorn
deactivate   # exit venv
```

## Sequence summary

1. The demo app serves an OpenAPI document.
2. `appctl setup` guides provider setup and sync; manual `appctl sync` maps operations to tools in `.appctl/schema.json` (here `provenance=declared`).
3. `appctl doctor` checks reachability against the live base URL.
4. `appctl chat` / `appctl run` send the tool list to the model and execute the calls it requests.

**Next:** [other sync sources](/docs/sources/openapi/) · [provenance and safety](/docs/concepts/provenance-and-safety/) · [deploy / serve](/docs/deploy/server/)
