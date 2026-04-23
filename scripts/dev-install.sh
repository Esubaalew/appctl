#!/usr/bin/env bash
# Install the in-tree appctl binary so the installed `appctl` on PATH is always
# the local git tree, never a released version. Stamps the installed binary with
# the current git SHA in its output so you can always tell you're on a dev build.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

sha="$(git rev-parse --short HEAD)"
branch="$(git rev-parse --abbrev-ref HEAD)"
dirty=""
if [[ -n "$(git status --porcelain)" ]]; then
  dirty="+dirty"
fi

version="0.0.0-dev-${sha}${dirty}"

echo "==> Installing appctl from $repo_root"
echo "    branch: $branch"
echo "    sha:    $sha$dirty"
echo "    tag:    $version"

APPCTL_BUILD_INFO="$version" \
  cargo install --path crates/appctl --locked --force

installed="$(command -v appctl || true)"
if [[ -z "$installed" ]]; then
  echo "!! appctl not on PATH after install. Add \$HOME/.cargo/bin to PATH." >&2
  exit 1
fi

echo "==> Installed binary: $installed"
"$installed" --version || true
echo
echo "==> Sanity check: running a non-provider command"
"$installed" config provider-sample >/dev/null
echo "    ok"
echo
echo "Dev install complete. Run \`appctl init\` to configure a provider."
