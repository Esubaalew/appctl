---
title: OpenAPI / Swagger
description: Turn any OpenAPI 2 or 3 document into typed tools for your agent.
---

If your app serves an OpenAPI document, this is the source to use. It produces the cleanest tool names and the most accurate parameter schemas. For frameworks without a dedicated `appctl sync --…` target (e.g. Nest, Spring, Next with a spec), this is the usual path—see [Choosing a sync source](/docs/sources/choosing-a-sync-source/).

## Fetching the document

- **File path** — pass a local `.json` or `.yaml` path; no network.
- **http(s) URL** — `appctl` uses a normal GET with a clear `User-Agent` (`appctl/<version>`), follows redirects, and sends `Accept: application/json, application/yaml, …`.
- **Authenticated spec URL** — pass the header needed to download the spec:  
  `appctl sync --openapi https://api.example.com/v3/api-docs --auth-header 'Authorization: Bearer env:STAGING_API_KEY'`  
  The header is used to download the spec and is stored in `schema` metadata for runtime HTTP tools unless you override target auth in config. Supported forms:
  - `Header-Name: literal`
  - `Header-Name: env:VAR` (value is read from the environment at sync time)
  - `Authorization: Bearer env:VAR` (expands to `Bearer <value>`)
- **Root base URL** — if you pass a **site root** (path `/` or empty) and the first GET returns 404, `appctl` also tries: `/openapi.json`, `/v3/api-docs`, `/v2/api-docs`, `/api-docs`, `/swagger.json`, `/api/openapi.json` (common Spring / gateway layouts).

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
- Reads `securitySchemes` and top-level `security` from the spec and picks the matching `AuthStrategy`.

## OpenAPI and protected (authenticated) APIs

`appctl sync` turns operations into **tools** with the parameters your spec advertises. It does **not** “log in as admin” to your app. You wire credentials into **HTTP** requests the executor makes.

### Header-based auth (typical: Bearer, API key)

- **`[target] auth_header` in [`.appctl/config.toml`](../cli/config/)** — a header sent on **every** HTTP tool call. It can be either a raw `Authorization` value (`Bearer env:API_TOKEN`) or a full `Header-Name: value` line (`X-Api-Key: env:API_KEY`).
- **`appctl sync ... --auth-header "Authorization: Bearer env:API_TOKEN"`** — stores the same header line in the synced schema’s `metadata.auth_header`; the executor resolves `env:` again at call time. Avoid committing long-lived tokens in the repo; use `auth_header` in config for production.
- **Security schemes** in the spec set `schema.auth` where possible. `appctl` maps bearer, basic, and API-key auth in `header`, `query`, or `cookie` locations and reads secrets from the env or keychain.

If your backend accepts **`Authorization: Bearer <token>`**, set **`[target] auth_header`** (or a mapped `schema.auth` bearer) so the **LLM** does not need the secret. It only chooses tools and body/query **arguments the spec still exposes as parameters**.

### Query-based tokens (e.g. `?access_token=`)

If your OpenAPI file declares an `apiKey` security scheme with `in: query`,
`appctl` now injects that query parameter from env/keychain at runtime. If the
spec lists `access_token` (or similar) as a normal **query** parameter instead,
it becomes a normal tool parameter.

To supply a value **without** typing it in chat, use **`[target.default_query]`** in `config.toml`: a map of query **names to values** merged into every relevant HTTP call. Tool arguments from the model **override** these defaults for the same key. Values can be a literal or `env:VAR` to read from the environment, for example:

```toml
[target.default_query]
access_token = "env:MY_API_ACCESS_TOKEN"
```

Use `appctl doctor` to check reachability after you set the base URL and auth.

### What the model does vs what appctl adds (so the agent can use auth)

The **language model** only chooses tool names and JSON **arguments** that match the generated tool schema. It does **not** “log in” to your app by itself.

| Layer | Who supplies it |
| --- | --- |
| **HTTP headers** (`Authorization`, API keys, etc.) | **appctl** — from `[target] auth_header`, `schema.metadata.auth_header` (from sync), or `schema.auth` + env/keychain. Injected on every HTTP tool request by the appctl process before the call is sent. The model is **not** given your raw bearer string in the system prompt; it should **call the tool** and let appctl attach headers. |
| **Query parameters** in the spec | **Model** if it passes them in the tool call; **appctl** fills gaps from `[target.default_query]` (e.g. `env:VAR`) so optional tokens need not be typed in chat. If you configure `default_query` for a name, the model can omit that key and the runtime still sends it. |
| **What to tell users** | Configure `target` (and `default_query` for query-style APIs) *before* chat; then ask the model in natural language to **list/call** endpoints. If it still asks for a token, ensure `default_query` or header auth is set, or the tool will return 401/403 in the result. |

In short: **auth for the target API is not “in the model’s head”** — it is in **appctl config and the OS env/keychain**. The model’s job is to **use tools**; the executor’s job is to **add configured credentials** to each HTTP request.

## Known limits

- Incomplete specs produce incomplete tools. Lint your spec with Spectral before syncing.
- If the spec is served behind an HTML login flow, either save it to a file and pass the file path, or configure cookies/session auth separately. `appctl` does not complete arbitrary browser logins during OpenAPI sync.
- Operation-specific security is represented best when the OpenAPI document uses clear global or per-operation `security` entries. If different endpoints need unrelated credentials, prefer `[target] auth_header` for the server instance or split schemas per auth domain.
- Webhook and callback operations are ignored.

## See also

- [`appctl sync`](/docs/cli/sync/)
- [`appctl doctor`](/docs/cli/doctor/)
- [Sync and schema](/docs/concepts/sync-and-schema/)
