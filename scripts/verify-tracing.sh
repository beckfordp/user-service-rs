#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track tracing_20261109 (gluon PLAN.md RUST-3.2): spans as purerest
# exports them, judged by the contract harness (tests/contract): the whole
# traces group, the outbox group's Debezium routing check (traceparent header)
# and the config group's service.name check. Each result is printed as found.
#   scripts/verify-tracing.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-tracing.XXXXXX)"
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

echo "== 2. Traces (contract traces group) =="
contract_checks checks/test_traces.py

echo "== 3. Debezium routing: traceparent and correlation_id headers (contract outbox group) =="
contract_checks checks/test_outbox.py -k debezium

echo "== 4. USER_SERVICE_NAME names the spans (contract config group) =="
contract_checks checks/test_config.py -k service_name

echo "== 5. No endpoint: spans on stderr, as the Java console exporter prints them =="
pg_start
port="$(free_port)"
env -i PATH="$PATH" $PG_ENV USER_SERVICE_PORT="$port" USER_SERVICE_METRICS_PORT="$(free_port)" \
  target/debug/user-service > "$SCRATCH/out.log" 2> "$SCRATCH/err.log" &
pid=$!
if wait_http "http://127.0.0.1:$port/health" 30; then
  http_code "http://127.0.0.1:$port/health" > /dev/null
  sleep 1
  line="$(grep -m1 "^'GET /health' : " "$SCRATCH/err.log")"
  if [ -n "$line" ]; then ok "stderr: ${line:0:110}…"; else bad "no span line on stderr"; fi
else
  bad "service never answered /health"
fi
kill -TERM "$pid" 2> /dev/null
wait "$pid" 2> /dev/null
pg_stop

verify_summary "RUST-3.2 tracing"
