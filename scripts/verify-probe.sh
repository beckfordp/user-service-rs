#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies the consumer probe (gluon PLAN.md RUST-6.2 to 6.4) with the contract
# harness's probe group (tests/contract checks/test_probe.py), each check's
# result printed as found. Without --only every check must be green; with
# --only '<pytest -k expression>' just those (earlier items' subsets).
#   scripts/verify-probe.sh [--skip-ci] [--only EXPR]
# Needs the CDC jars for the broker: gluon bin/sandbox-setup --only cdc.
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-probe.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
SKIP_CI=0
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --skip-ci) SKIP_CI=1 ;;
    --only) ONLY="$2"; shift ;;
  esac
  shift
done

echo "== 1. CI (scripts/ci.sh) =="
if [ "$SKIP_CI" = 1 ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes ($(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}') of lines covered)"
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. The probe group (contract)${ONLY:+, only: $ONLY} =="
if cargo build -q --locked -p events-probe > "$SCRATCH/build.log" 2>&1; then
  ok "built target/debug/events-probe"
else
  bad "cargo build failed"
  tail -5 "$SCRATCH/build.log" | sed 's/^/     /'
fi
contract="$(realpath ../../tests/contract)"
(cd "$contract" && uv run --no-sync pytest checks/test_probe.py -p no:cacheprovider -rA --tb=line -q \
  ${ONLY:+-k "$ONLY"} --probe-cmd "$PWD/../../services/user-service-rs/target/debug/events-probe" \
  ) > "$SCRATCH/run.log" 2>&1
count=0
while read -r outcome test rest; do
  count=$((count + 1))
  if [ "$outcome" = PASSED ]; then ok "probe: ${test##*::}"; else bad "probe: ${test##*::} $rest"; fi
done < <(grep -E '^(PASSED|FAILED|ERROR) [^ ]+::' "$SCRATCH/run.log")
[ "$count" -gt 0 ] || { bad "no probe checks ran:"; tail -5 "$SCRATCH/run.log" | sed 's/^/     /'; }

verify_summary "events-probe${ONLY:+ ($ONLY)}"
