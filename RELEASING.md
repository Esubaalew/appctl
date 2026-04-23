# Releasing

`appctl` ships from a Cargo workspace. All releases are driven by git tags and
the GitHub Actions workflows under `.github/workflows/`.

## Crates published to crates.io

| Crate                | Path                             |
| -------------------- | -------------------------------- |
| `appctl-plugin-sdk`  | `crates/appctl-plugin-sdk/`      |
| `appctl`             | `crates/appctl/`                 |

`examples/plugins/appctl-airtable` is **not** published; it exists purely as an
integration-test fixture and as a template for third-party plugin authors.

## Version bumping

Versions are controlled from the workspace root:

```toml
# Cargo.toml
[workspace.package]
version = "X.Y.Z"
```

Every member crate inherits that version via `version.workspace = true`. Bump
the version in a single commit together with the matching `CHANGELOG.md`
entry.

`release-plz` (see `.github/workflows/release-plz.yml`) automates this process
by opening a PR whenever the `main` branch has unreleased commits.

## Cutting a release

1. Merge all release-worthy PRs into `main`.
2. `release-plz` opens a release PR automatically on every push to `main`.
   Review and merge it.
3. `release-plz` will tag the commit (e.g. `v0.2.0`) and publish both crates
   to crates.io in the correct order (`appctl-plugin-sdk` first, then
   `appctl`).
4. The tag triggers `.github/workflows/release.yml`, which runs `cargo-dist`
   to produce cross-platform binaries and a GitHub Release.
5. The `vscode.yml` workflow builds and uploads the `.vsix` extension as a
   release asset.

### Optional: live-providers as a release gate

The `live-providers` workflow runs the verify matrix against real provider
credentials. It is currently **informational only** — nightly + manually
triggered. When the team is ready to make it a hard release gate, restore
the `require-live-providers` job in `release-plz.yml`. That job queries the
GitHub API for a successful `live-providers` run on the exact commit
release-plz is about to publish and fails the workflow otherwise.

## Manual release (fallback)

If you must release by hand:

```bash
# 1. Make sure the tree is clean and tests pass.
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# 2. Publish the SDK first.
cargo publish -p appctl-plugin-sdk

# 3. Wait ~30s for crates.io indexing, then publish the CLI.
cargo publish -p appctl

# 4. Tag and push.
git tag vX.Y.Z
git push origin vX.Y.Z
```

The tag push triggers the release workflow exactly as it would for an
automated release.

## Checklist

- [ ] `CHANGELOG.md` has an entry for the new version.
- [ ] `Cargo.toml` `[workspace.package].version` is bumped.
- [ ] `cargo test --workspace` is green.
- [ ] `cargo package --no-verify -p appctl-plugin-sdk` inspects cleanly.
- [ ] `cargo package --no-verify -p appctl` inspects cleanly.
- [ ] Extension builds: `(cd extensions/vscode && npm run compile && npx vsce package)`.
