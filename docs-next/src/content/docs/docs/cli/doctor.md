---
title: appctl doctor
description: Probe every HTTP tool and mark reachable ones as verified.
---

Probe every HTTP tool in `.appctl/schema.json` and report reachability. Optionally upgrade `provenance` to `verified` for routes that did not return 404.

## Usage

```
appctl doctor [OPTIONS]
```

## Options

- `--write` — mark routes that did not return 404 as `provenance=verified` in the schema.
- `--timeout-secs <N>` — per-request timeout (default `10`).

## What it does

For each tool with a `kind: "http"` transport:

1. Resolves path placeholders to dummy values.
2. Builds the URL as `base_url + path`.
3. Issues the HTTP method with the configured auth headers.
4. Records the status code and prints a verdict.

Verdict legend:

- `reachable` — status is not 404 and not a transport error.
- `not_found` — 404.
- `error` — transport error (DNS, connection refused, TLS).

## Example

```bash
appctl doctor
```

Output:

```
tool                             method path         HTTP  verdict
create_widget_widgets_post       POST   /widgets      200  reachable
```

To mark those routes as `verified`:

```bash
appctl doctor --write
```

## When to use `--strict`

After `doctor --write`, pair it with `--strict` on `appctl chat`, `run`, or `serve` so inferred tools stay blocked:

```bash
appctl doctor --write
appctl chat --strict
```

## Related

- [Provenance and safety](/docs/concepts/provenance-and-safety/)
- [`appctl sync`](/docs/cli/sync/)
