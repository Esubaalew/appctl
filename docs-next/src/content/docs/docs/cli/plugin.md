---
title: plugin
description: Install or list dynamic sync plugins for appctl.
---

Manage dynamic plugins stored under `~/.appctl/plugins/`.

## Commands

- `appctl plugin list` — show the plugins the host can currently load.
- `appctl plugin install <path-or-name>` — install a compiled plugin binary into the plugin directory.

## Examples

List installed plugins:

```bash
appctl plugin list
```

Install a freshly built plugin:

```bash
cargo build --release -p appctl-airtable
appctl plugin install ./target/release/libappctl_airtable.dylib
```

Then sync with it:

```bash
appctl sync --plugin airtable --force
```

## Notes

- Plugins are loaded from `~/.appctl/plugins/`, not from the current project directory.
- The plugin name passed to `appctl sync --plugin ...` comes from the plugin manifest exposed by the shared library.
- Keep the plugin SDK version aligned with the `appctl` host minor version.

## See also

- [Plugin source guide](/docs/sources/plugins/)
- [`appctl sync`](/docs/cli/sync/)
