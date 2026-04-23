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
2. **Before release-plz will open its PR**, the `live-providers` workflow
   must be green on the same `main` SHA that release-plz wants to publish.
   Trigger it manually from the Actions tab (or wait for the nightly run)
   and make sure every non-skipped case passes.
3. Once `live-providers` is green, `release-plz` opens its PR. Review and
   merge it.
4. `release-plz` will tag the commit (e.g. `v0.2.0`) and publish both crates
   to crates.io in the correct order (`appctl-plugin-sdk` first, then
   `appctl`).
5. The tag triggers `.github/workflows/release.yml`, which runs `cargo-dist`
   to produce cross-platform binaries and a GitHub Release.
6. The `vscode.yml` workflow builds and uploads the `.vsix` extension as a
   release asset.

### Required branch-protection rules

These must be configured in GitHub branch protection for `main`
(Settings → Branches → Branch protection rules):

- Required status checks:
  - `CI / all-ok`
  - `live-providers / verify-matrix`
- Require branches to be up to date before merging.
- Restrict who can push to matching branches (release-plz PRs excluded).

The `require-live-providers` job in `release-plz.yml` is a belt-and-braces
guard inside CI: even if branch protection is accidentally weakened, the
publish job will refuse to run unless a successful `live-providers` run
exists for the exact commit being released.

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
