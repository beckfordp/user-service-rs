#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track yaml-content-type_20261110 (gluon PLAN.md RUST-5.0 follow-up):
# /docs/docs.yaml served as application/yaml with no charset, as the Scala
# service sends it; the whole contract harness against this build (every
# group, probe and CDC included: the CDC jars from gluon
# bin/sandbox-setup --only cdc); and gluon-contract's own CI, whose cluster-mode
# test starts this build. Each result is printed as it is found.
#   scripts/verify-yaml-content-type.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-yaml-content-type.XXXXXX)"
trap 'pg_stop; rm -rf "$SCRATCH"' EXIT
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

echo "== 2. The header, from a started service =="
if cargo build -q --locked -p user-service > "$SCRATCH/build.log" 2>&1; then
  pg_start
  port="$(free_port)"
  env -i PATH="$PATH" $PG_ENV USER_SERVICE_PORT="$port" USER_SERVICE_METRICS_PORT="$(free_port)" \
    target/debug/user-service > "$SCRATCH/out.log" 2> "$SCRATCH/err.log" &
  pid=$!
  if wait_http "http://127.0.0.1:$port/health" 30; then
    curl -s -o /dev/null -D "$SCRATCH/headers" "http://127.0.0.1:$port/docs/docs.yaml"
    expect "GET /docs/docs.yaml Content-Type" "application/yaml" \
      "$(grep -i '^content-type:' "$SCRATCH/headers" | cut -d' ' -f2- | tr -d '\r')"
    expect "HEAD /docs/docs.yaml" 405 "$(http_code -I "http://127.0.0.1:$port/docs/docs.yaml")"
  else
    bad "the service didn't answer on $port:"
    tail -5 "$SCRATCH/err.log" | sed 's/^/     /'
  fi
  kill "$pid" 2> /dev/null
  wait "$pid" 2> /dev/null
  pg_stop
else
  bad "cargo build failed:"
  tail -5 "$SCRATCH/build.log" | sed 's/^/     /'
fi

echo "== 3. The whole contract harness against this build =="
if cargo build -q --locked -p events-probe > "$SCRATCH/probe-build.log" 2>&1; then
  contract_checks checks --probe-cmd "$PWD/target/debug/events-probe"
else
  bad "cargo build -p events-probe failed:"
  tail -5 "$SCRATCH/probe-build.log" | sed 's/^/     /'
fi

echo "== 4. gluon-contract's CI (its cluster-mode test starts this build) =="
if (cd "$CONTRACT" && scripts/ci.sh) > "$SCRATCH/contract-ci.log" 2>&1; then
  ok "tests/contract scripts/ci.sh passes ($(grep -Eo '[0-9]+ passed' "$SCRATCH/contract-ci.log" | tail -1))"
else
  bad "tests/contract scripts/ci.sh failed:"
  tail -8 "$SCRATCH/contract-ci.log" | sed 's/^/     /'
fi

verify_summary "RUST-5.0 follow-up: docs.yaml Content-Type"
