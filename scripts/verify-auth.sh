#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track auth_20261109 (gluon PLAN.md RUST-2.5): signup and login as the
# Scala service answers them, judged by the contract harness (tests/contract):
# the whole schema and stored-format group, and the REST group's signup and
# login checks. Each result is printed as it is found.
#   scripts/verify-auth.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-auth.XXXXXX)"
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

echo "== 2. Schema and stored formats (contract) =="
contract_checks checks/test_schema.py

echo "== 3. Signup and login (contract REST group) =="
contract_checks checks/test_rest.py -k \
  "(signup or login or password or email or content_type or undecodable) and not trims and not ascii_whitespace and not wrong_method and not get_user"

verify_summary "RUST-2.5 signup and login"
