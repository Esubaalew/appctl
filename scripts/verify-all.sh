#!/usr/bin/env bash
# verify-all.sh
#
# Drives scripts/verify-matrix.toml and produces verify-report.json.
# Thin bash wrapper that delegates to verify_all.py so TOML parsing and
# JSON writing live in Python.
#
# Exit code: 0 if every non-skipped case passed, non-zero otherwise.
#
# Usage:
#   scripts/verify-all.sh                # run all cases whose env is set
#   scripts/verify-all.sh --strict       # fail if any case was skipped
#   scripts/verify-all.sh --report path  # write report to a custom path
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

python3 "$repo_root/scripts/verify_all.py" "$@"
