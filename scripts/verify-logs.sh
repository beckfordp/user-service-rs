#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track logs_20261109 (gluon PLAN.md RUST-3.4): logstash-style JSON
# log lines as the Scala service writes them, judged by the contract harness
# (tests/contract): the whole logs group and the config group's logger-name
# check; plus a look at a real run's stdout here. Each result is printed as
# it is found.
#   scripts/verify-logs.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-logs.XXXXXX)"
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

echo "== 2. Logs (contract logs group) =="
contract_checks checks/test_logs.py

echo "== 3. USER_SERVICE_NAME names the logger (contract config group) =="
contract_checks checks/test_config.py -k logger

echo "== 4. A real run's stdout =="
pg_start
port="$(free_port)"
env -i PATH="$PATH" $PG_ENV USER_SERVICE_PORT="$port" USER_SERVICE_METRICS_PORT="$(free_port)" AUTH_PBKDF2_ITERATIONS=1000 \
  target/debug/user-service > "$SCRATCH/out.log" 2> "$SCRATCH/err.log" &
pid=$!
password="verify-logs-secret-1"
if wait_http "http://127.0.0.1:$port/health" 30; then
  body="{\"email\":\"verify-logs@example.com\",\"password\":\"$password\"}"
  token="$(curl -s -H 'Content-Type: application/json' -d "$body" "http://127.0.0.1:$port/auth/signup" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["token"])')"
  curl -s -o /dev/null -H 'Content-Type: application/json' -d "$body" "http://127.0.0.1:$port/auth/login"
  sleep 0.5
  total="$(wc -l < "$SCRATCH/out.log")"
  json="$(grep -c '^{"@timestamp":' "$SCRATCH/out.log")"
  expect "stdout lines that Filebeat decodes (start {\"@timestamp\")" "$total of $total" "$json of $total"
  for message in "user-service starting" "Received request" "Account created" "Login succeeded"; do
    line="$(grep -m1 "\"message\":\"$message\"" "$SCRATCH/out.log")"
    if [ -n "$line" ]; then ok "${line:0:150}…"; else bad "no \"$message\" line"; fi
  done
  if grep -q -e "$password" -e "$token" "$SCRATCH/out.log" "$SCRATCH/err.log"; then
    bad "a password or token was logged"
  else
    ok "neither the password nor the token appears in stdout or stderr"
  fi
else
  bad "service never answered /health"
fi
kill -TERM "$pid" 2> /dev/null
wait "$pid" 2> /dev/null
pg_stop

verify_summary "RUST-3.4 logs"
