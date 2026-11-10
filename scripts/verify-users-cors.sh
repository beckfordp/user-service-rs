#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track users-cors_20261109 (gluon PLAN.md RUST-2.6): the rest of the
# REST contract. The whole contract REST group (tests/contract
# checks/test_rest.py) must be green against this build. Each result is
# printed as it is found.
#   scripts/verify-users-cors.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-users-cors.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

echo "== 1. CI (scripts/ci.sh) =="
if [ "${1:-}" = "--skip-ci" ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes ($(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}') of lines covered)"
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. The whole REST group (contract) =="
contract_checks checks/test_rest.py

verify_summary "RUST-2.6 GET /users/{id}, CORS, error and rejection responses"
