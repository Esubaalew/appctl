---
title: Plugins
description: Write a dynamic plugin in Rust to add a new sync source.
---

If none of the built-in sources fit, write a plugin. Plugins are compiled Rust `cdylib`s that implement a stable C ABI and live in `~/.appctl/plugins/`.

## When to use a plugin

- The app exposes something that is not an HTTP API, a SQL database, an MCP server, or an HTML login form.
- You want to redistribute a sync source internally without upstreaming it.
- You need custom logic to rewrite, filter, or enrich a schema before it is handed to the agent.

## SDK

The [`appctl-plugin-sdk`](https://crates.io/crates/appctl-plugin-sdk) crate exposes stable schema types and a `declare_plugin!` macro. The `appctl` host loads plugins with `libloading`, validates the ABI version, and calls your `introspect` function.

### Minimum viable plugin

```rust
use appctl_plugin_sdk::prelude::*;

fn introspect(_input: SyncInput) -> anyhow::Result<Schema> {
    let mut schema = Schema::new("airtable");
    schema.resources.push(Resource::new("record"));
    Ok(schema)
}

appctl_plugin_sdk::declare_plugin! {
    name: "airtable",
    version: "0.1.0",
    introspect,
}
```

Cargo:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
appctl-plugin-sdk = "0.2"
anyhow = "1"
```

## Build and install

```bash
cargo build --release
appctl plugin install ./target/release/libappctl_airtable.dylib
```

Verify:

```bash
appctl plugin list
```

Sync with the plugin:

```bash
appctl sync --plugin airtable --force
```

## Host lifecycle

1. `appctl` scans `~/.appctl/plugins/` at startup.
2. Each shared library is probed for an `appctl_plugin_sdk` export with a matching ABI version.
3. On sync, the host calls `introspect` once and writes the returned schema to `.appctl/schema.json`.

## Known limits

- The ABI is still early. Pin to exact SDK minor versions; rebuild plugins when `appctl` upgrades.
- Plugins run in the host process. A panic crashes `appctl`. Return `Err(...)` for recoverable failures.
- Windows shared libraries are not covered by CI yet.

## See also

- [`appctl plugin`](/docs/cli/config/) (plugin management lives under `appctl plugin`)
- [Sync and schema](/docs/concepts/sync-and-schema/)
