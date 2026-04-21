#!/usr/bin/env bash
# validate-demos.sh
#
# Runs `appctl sync` against each static demo (those that don't need a live server)
# and verifies the command exits 0 and writes a .appctl/ schema.
#
# Usage:
#   ./scripts/validate-demos.sh           # run all static syncs
#   ./scripts/validate-demos.sh --force   # pass --force to appctl sync
#
# Requirements:
#   appctl must be installed (cargo install appctl or in PATH)

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMOS="$REPO_ROOT/examples/demos"

FORCE=""
for arg in "$@"; do
  [[ "$arg" == "--force" ]] && FORCE="--force"
done

PASS=0
FAIL=0

run_sync() {
  local label="$1"
  shift
  local dir="$1"
  shift
  local cmd=("$@")

  echo ""
  echo "── $label ──"
  echo "   dir: $dir"
  echo "   cmd: ${cmd[*]}"

  local tmp
  tmp=$(mktemp -d)
  trap "rm -rf $tmp" EXIT

  # Run sync in a temp copy so we don't pollute the repo
  cp -r "$dir/." "$tmp/"
  cd "$tmp"

  if "${cmd[@]}" $FORCE 2>&1; then
    if [[ -f ".appctl/schema.json" ]]; then
      echo "   ✓ schema written"
      PASS=$((PASS + 1))
    else
      echo "   ✗ sync succeeded but .appctl/schema.json not found"
      FAIL=$((FAIL + 1))
    fi
  else
    echo "   ✗ sync command failed"
    FAIL=$((FAIL + 1))
  fi

  cd "$REPO_ROOT"
}

# Static syncs — no running server required
run_sync "django-drf" "$DEMOS/django-drf" \
  appctl sync --django . --base-url http://127.0.0.1:8001

run_sync "rails-api" "$DEMOS/rails-api" \
  appctl sync --rails . --base-url http://127.0.0.1:3001

run_sync "laravel-api" "$DEMOS/laravel-api" \
  appctl sync --laravel . --base-url http://127.0.0.1:8002

run_sync "aspnet-api" "$DEMOS/aspnet-api" \
  appctl sync --aspnet . --base-url http://localhost:5001

run_sync "strapi" "$DEMOS/strapi" \
  appctl sync --strapi . --base-url http://localhost:1337

echo ""
echo "Results: $PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]] && exit 0 || exit 1
