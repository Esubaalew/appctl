---
title: OpenAPI / Swagger
description: Turn any OpenAPI 2 or 3 document into typed tools for your agent.
---

If your app serves an OpenAPI document, this is the source to use. It produces the cleanest tool names and the most accurate parameter schemas.

## Prerequisites

- A running HTTP server that serves an OpenAPI 2 or 3 document. The document can be reachable at a URL (for example `/openapi.json`) or saved as a file on disk.
- `appctl` installed. See [Installation](/docs/installation/).

## The demo in this repo

[`examples/demos/openapi-fastapi/`](https://github.com/Esubaalew/appctl/tree/main/examples/demos/openapi-fastapi) is a FastAPI app with one endpoint. FastAPI generates the OpenAPI document automatically at `/openapi.json`.

### 1. Start the server

```bash
cd examples/demos/openapi-fastapi
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
uvicorn main:app --host 127.0.0.1 --port 8000
```

### 2. Confirm the spec is being served

```bash
curl -s http://127.0.0.1:8000/openapi.json | head -c 80
```

Real output:

```
{"openapi":"3.1.0","info":{"title":"appctl demo API","version":"1.0.0"},"paths"
```

### 3. Sync appctl

In another terminal:

```bash
appctl sync --openapi http://127.0.0.1:8000/openapi.json \
  --base-url http://127.0.0.1:8000 --force
```

Real output with `appctl 0.2.0`:

```
Synced Openapi: 1 resources, 1 tools written to .appctl
```

### 4. Inspect what was generated

```bash
cat .appctl/schema.json | python3 -c \
  "import json,sys; s=json.load(sys.stdin); \
   [print(r['name'], [a['name'] for a in r['actions']]) for r in s['resources']]"
```

Output:

```
widgets ['create_widget_widgets_post']
```

### 5. Call it through chat

```bash
appctl chat "create a widget named Demo"
```

This step requires a language model configured. See [Installation](/docs/installation/#configure-a-provider).

## Verify the tools are reachable

```bash
appctl doctor
```

Expected:

```
tool                             method path         HTTP  verdict
create_widget_widgets_post       POST   /widgets      200  reachable
```

Pass `--write` to mark reachable routes as `provenance=verified` in the schema.

## What appctl does in this mode

- Fetches the OpenAPI document, or reads it from disk.
- For each path and operation, creates one tool with JSON Schema parameters.
- Marks tools as `provenance=declared`. This is the highest trust level because the server itself published the contract.
- Reads `securitySchemes` from the spec and picks the matching `AuthStrategy`.

## Known limits

- Incomplete specs produce incomplete tools. Lint your spec with Spectral before syncing.
- If the spec is served behind login, save it to a file and pass the file path.
- Webhook and callback operations are ignored.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [`appctl doctor`](/docs/cli/doctor/)
- [Sync and schema](/docs/concepts/sync-and-schema/)
