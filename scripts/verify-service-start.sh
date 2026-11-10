#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track service-start_20261109 (gluon PLAN.md RUST-2.3): the service
# reads the config contract, serves /health, refuses bad config, stops on
# SIGTERM, and the contract harness (tests/contract) can start it as a target.
# Each result is printed as it is found.
#   scripts/verify-service-start.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"

SCRATCH="$(mktemp -d /tmp/verify-service-start.XXXXXX)"
PID=""
cleanup() {
  [ -n "$PID" ] && kill "$PID" 2> /dev/null
  pg_stop
  rm -rf "$SCRATCH"
}
trap cleanup EXIT
BIN="$PWD/target/debug/user-service"
CONTRACT="$(realpath ../../tests/contract)"

echo "== 1. CI (scripts/ci.sh) =="
if [ "${1:-}" = "--skip-ci" ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes ($(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}') of lines covered)"
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. The binary =="
check "cargo build" cargo build --locked -p user-service
pg_start && ok "throwaway Postgres for it (the service needs one since RUST-2.4)" || bad "Postgres didn't start"
PORT="$(free_port)"
# shellcheck disable=SC2086 # PG_ENV is a list of assignments
env -i PATH="$PATH" USER_SERVICE_PORT="$PORT" USER_SERVICE_METRICS_PORT="$(free_port)" $PG_ENV \
  "$BIN" > "$SCRATCH/out.log" 2>&1 &
PID=$!
wait_http "http://127.0.0.1:$PORT/health" && ok "serving on the configured port ($PORT)" || bad "never served on $PORT"
curl -s -m 5 -D "$SCRATCH/headers" -o "$SCRATCH/body" "http://127.0.0.1:$PORT/health"
expect "GET /health status" 200 "$(head -1 "$SCRATCH/headers" | awk '{print $2}')"
expect "GET /health body (empty, as captured)" "" "$(cat "$SCRATCH/body")"
grep -qi '^content-type' "$SCRATCH/headers" && bad "GET /health has a Content-Type" || ok "GET /health has no Content-Type (as captured)"
kill -TERM "$PID"
wait "$PID"
expect "SIGTERM stops it; exit status" 0 "$?"
PID=""

echo "== 3. Bad config =="
PORT="$(free_port)"
env -i PATH="$PATH" USER_SERVICE_PORT=notaport AUTH_PBKDF2_ITERATIONS=0 AUTH_ACCESS_TOKEN_TTL=500ms \
  USER_SERVICE_METRICS_PORT="$PORT" "$BIN" > /dev/null 2> "$SCRATCH/err.log"
expect "exit status" 1 "$?"
for variable in USER_SERVICE_PORT AUTH_PBKDF2_ITERATIONS AUTH_ACCESS_TOKEN_TTL; do
  grep -q "$variable" "$SCRATCH/err.log" && ok "reported: $(grep "$variable" "$SCRATCH/err.log" | sed 's/^- //')" \
    || bad "$variable not reported"
done
port_open "$PORT" && bad "something is listening on $PORT" || ok "nothing bound"

echo "== 4. The contract harness starts it =="
mkdir -p "$SCRATCH/suite"
cp "$CONTRACT/checks/conftest.py" "$SCRATCH/suite/"
cat > "$SCRATCH/suite/test_probe.py" << 'PY'
import urllib.request


def test_health(harness):
    with urllib.request.urlopen(harness.base_url + "/health") as response:
        assert response.status == 200
PY
out="$(cd "$CONTRACT" && CONTRACT_READINESS_PATH=/health uv run --no-sync pytest -p no:cacheprovider \
  "$SCRATCH/suite" --target-cmd "$BIN" --target-name user-service-rs 2>&1)"
code=$?
grep -q "gluon-contract: target user-service-rs ($BIN) as user-service" <<< "$out" \
  && ok "the harness names it as the target" || bad "no harness header"
expect "a check against it passes (pytest exit status)" 0 "$code"
[ "$code" = 0 ] || tail -8 <<< "$out" | sed 's/^/     /'
[ -z "$(pgrep -f -- "$BIN" || true)" ] && ok "stopped afterwards" || bad "still running: $(pgrep -af -- "$BIN")"

verify_summary "RUST-2.3 config contract and /health"
