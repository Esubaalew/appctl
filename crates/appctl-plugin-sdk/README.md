# appctl-plugin-sdk

Stable schema types and a C ABI for building dynamic plugins that extend the
[`appctl`](https://crates.io/crates/appctl) CLI.

Plugins are shipped as `cdylib`s and dropped into `~/.appctl/plugins/`. The
`appctl` host loads them with `libloading`, validates the ABI version, and
invokes the plugin's `introspect` function to obtain a normalized
[`Schema`].

## Example

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

Build with `crate-type = ["cdylib"]` and install with:

```bash
appctl plugin install ./target/release/libappctl_airtable.dylib
```

## License

MIT — see [`LICENSE`](https://github.com/esubaalew/appctl/blob/main/LICENSE).
