#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track metrics_20261109 (gluon PLAN.md RUST-3.3): purerest's metrics
# on the metrics port and ids parsed as java.util.UUID.fromString parses them,
# judged by the contract harness (tests/contract): the whole metrics group and
# the REST group's Java-UUID check; plus a scrape read here, family by family,
# against the capture's shapes. Each result is printed as it is found.
#   scripts/verify-metrics.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-metrics.XXXXXX)"
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

echo "== 2. JVM UUID parsing agrees with the JDK's known answers =="
if cargo test -q --locked -p telemetry --test jvm > "$SCRATCH/jvm.log" 2>&1; then
  ok "telemetry::jvm::parse_uuid = java.util.UUID.fromString on $(($(wc -l < crates/telemetry/tests/fixtures/java-uuid-vectors.jsonl) - 1)) vectors ($(head -1 crates/telemetry/tests/fixtures/java-uuid-vectors.jsonl))"
else
  bad "JDK vectors disagree:"
  grep -E "jdk|panicked" "$SCRATCH/jvm.log" | head -5 | sed 's/^/     /'
fi

echo "== 3. Metrics (contract metrics group) =="
contract_checks checks/test_metrics.py

echo "== 4. Ids parse as the JVM parses them (contract REST group) =="
contract_checks checks/test_rest.py -k "java_uuid or malformed_user_id or uppercase_id or unknown_user"

echo "== 5. A scrape after some traffic =="
pg_start
port="$(free_port)"
mport="$(free_port)"
env -i PATH="$PATH" $PG_ENV USER_SERVICE_PORT="$port" USER_SERVICE_METRICS_PORT="$mport" AUTH_PBKDF2_ITERATIONS=1000 \
  target/debug/user-service > "$SCRATCH/out.log" 2> "$SCRATCH/err.log" &
pid=$!
if wait_http "http://127.0.0.1:$port/health" 30; then
  body='{"email":"verify-metrics@example.com","password":"correct-horse-1"}'
  curl -s -o /dev/null -H 'Content-Type: application/json' -d "$body" "http://127.0.0.1:$port/auth/signup"
  curl -s -o /dev/null -H 'Content-Type: application/json' -d "$body" "http://127.0.0.1:$port/auth/login"
  curl -s -o /dev/null "http://127.0.0.1:$port/health/ready"
  curl -s -D "$SCRATCH/headers" -o "$SCRATCH/scrape.txt" "http://127.0.0.1:$mport/metrics"
  expect "scrape Content-Type" "text/plain; version=0.0.4; charset=utf-8" \
    "$(grep -i '^content-type:' "$SCRATCH/headers" | cut -d' ' -f2- | tr -d '\r')"
  for family in http_server_request_duration_seconds db_client_operation_duration_seconds \
    db_client_connection_count db_client_connection_pending_requests \
    db_client_connection_wait_time_seconds users_registrations_total users_logins_total; do
    line="$(grep -m1 -E "^${family}(_count|_bucket)?\{" "$SCRATCH/scrape.txt")"
    if [ -n "$line" ]; then ok "$family: ${line:0:120}"; else bad "$family missing from the scrape"; fi
  done
else
  bad "service never answered /health"
fi
kill -TERM "$pid" 2> /dev/null
wait "$pid" 2> /dev/null
pg_stop

verify_summary "RUST-3.3 metrics"
