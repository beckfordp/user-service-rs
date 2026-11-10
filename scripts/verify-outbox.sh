#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track outbox_20261109 (gluon PLAN.md RUST-3.1): the UserRegistered
# outbox row and envelope, judged by the contract harness's outbox group
# (tests/contract checks/test_outbox.py), and the payload schema checked
# against gluon's registry. The Debezium routing check needs the traceparent
# header, which comes with tracing (RUST-3.2); it is run from
# scripts/verify-tracing.sh. Each result is printed as it is found.
#   scripts/verify-outbox.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-outbox.XXXXXX)"
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

echo "== 2. Payload schema against gluon's registry =="
registry="$(realpath ../../registry)"
if GLUON_REGISTRY="$registry" cargo test -q --locked -p user-service --test events \
  the_payload_schema_is_the_latest_registered_version > "$SCRATCH/schema.log" 2>&1; then
  ok "generated UserRegistered schema equals the latest version in $registry/user-service/registry.json"
else
  bad "payload schema differs from the registry:"
  grep -E "left|right|panicked" "$SCRATCH/schema.log" | head -6 | sed 's/^/     /'
fi

echo "== 3. Outbox and envelope (contract outbox group) =="
contract_checks checks/test_outbox.py -k "not debezium"

verify_summary "RUST-3.1 outbox row and envelope"
