---
title: appctl doctor
description: HTTP route probes for the synced schema — verify what is actually reachable.
---

`appctl doctor` takes `.appctl/schema.json` (produced by [`appctl sync`](/docs/cli/sync/))
and probes every HTTP tool against the live base URL. It is safe to run
against production: the probe uses `HEAD` and `OPTIONS` where possible and
falls back to small `GET` requests, never mutating verbs.

## Usage

```bash
appctl doctor [OPTIONS]
```

## Options

| Flag | Default | What it does |
| --- | --- | --- |
| `--timeout-secs <SECS>` | `10` | Per-request timeout. |
| `--write` | off | After probing, update `provenance = "verified"` in `.appctl/schema.json` for tools that returned anything other than `404` or a connection error. |

## What it probes

For each `Http` tool in the schema, `doctor` tries:

| Tool method | Order it tries |
| --- | --- |
| `GET` | `HEAD` → fallback `GET` |
| `DELETE` | `HEAD` → fallback `OPTIONS` |
| `POST` / `PUT` / `PATCH` | `OPTIONS` → fallback `HEAD` |

The goal is to get a status code for the route without actually executing the
side effect. A `200`, `401`, `403`, `405`, or `5xx` is still considered
"reachable" for the purposes of provenance — it only means the route exists.
Only `404` and connection errors are treated as not reachable.

Path placeholders (`{id}`, `{Id}`, `{uuid}`) are substituted with `1` or a
zero-UUID so the probe has a real URL.

## What it does NOT do

- It does not call the LLM provider. For a provider sanity check, just run
  `appctl chat` and send `hi` — the agent starts up lazily on first message.
- It does not `POST` anything unless it has to; mutating tools are probed with
  `OPTIONS` first.
- It does not write to the schema unless you pass `--write`.

## `--strict` and provenance

The chat and serve loops accept a `--strict` flag that blocks any tool with
`provenance = "inferred"`. The only way to flip that flag to `verified` today
is `appctl doctor --write`. Re-run it after every `appctl sync` in strict
deployments.

## Examples

```bash
# Probe every HTTP tool with the default 10s timeout.
appctl doctor

# Long-running routes or flaky APIs
appctl doctor --timeout-secs 30

# Record a verified snapshot after a clean run
appctl doctor --write
```

## Output

```text
┌─ doctor
│  Safe HTTP probes for each tool in the synced schema (HEAD/OPTIONS/GET)
│
│  app directory  /Users/you/project/.appctl
│  Target: base URL https://api.example.com
│
│  tool                             method path                                       HTTP  verdict
│  ────────────────────────────────────────────────────────────────────────────────────────────────
│  users.list                       GET    /users                                        200  reachable
│  users.create                     POST   /users                                        204  reachable
│  users.get                        GET    /users/1                                      200  reachable
│  orders.refund                    POST   /orders/1/refund                              404  missing (404)
│
│  Tip: Pass --write to mark reachable (non-404) routes as provenance=verified.
```

## Related

- [`appctl sync`](/docs/cli/sync/) — generate the tools `doctor` probes.
- [Provenance and safety](/docs/concepts/provenance-and-safety/) — what the
  `verified` flag unlocks.
