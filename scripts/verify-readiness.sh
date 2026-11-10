#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track readiness_20261109 (gluon PLAN.md RUST-2.4): the baseline
# migration gives the Scala service's schema, readiness follows Postgres, and
# an unreachable database stops startup. Each result is printed as it is found.
#   scripts/verify-readiness.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-readiness.XXXXXX)"
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

echo "== 2. Contract checks (tests/contract) against this build =="
contract_checks "checks/test_schema.py::test_the_schema_matches_the_scala_migrations" \
  "checks/test_rest.py::test_health_endpoints_answer_200_with_no_body" \
  "checks/test_rest.py::test_readiness_is_503_while_postgres_is_down"

echo "== 3. An unreachable database stops startup =="
env -i PATH="$PATH" USER_SERVICE_PORT="$(free_port)" USER_SERVICE_METRICS_PORT="$(free_port)" \
  POSTGRES_HOST=127.0.0.1 POSTGRES_PORT="$(free_port)" target/debug/user-service > /dev/null 2> "$SCRATCH/err"
expect "exit status" 1 "$?"
grep -q "database 127.0.0.1:.*unreachable" "$SCRATCH/err" && ok "says so: $(head -c 120 "$SCRATCH/err")" \
  || bad "no database message: $(cat "$SCRATCH/err")"

verify_summary "RUST-2.4 baseline migration, pool and /health/ready"
