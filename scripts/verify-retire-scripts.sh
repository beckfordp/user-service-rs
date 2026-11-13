#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track retire-scripts_20261113 (gluon TD-46): the track scripts CI
# and the contract harness already cover are gone, nothing live still names
# them, the tools are kept, and workflow.md states the retirement rule.
# Retired itself when the track is archived. Each result is printed as found.
#   scripts/verify-retire-scripts.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-retire-scripts.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

RETIRED=(auth users-cors openapi asyncapi outbox tracing metrics logs probe
  service-start readiness yaml-content-type)
KEPT=(verify-rust-ci verify-cargo-audit asyncapi-html java-uuid-vectors vendor-swagger-ui)

echo "== 1. CI (scripts/ci.sh) =="
if [ "${1:-}" = "--skip-ci" ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes ($(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}') of lines covered)"
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. Retired scripts are gone =="
for name in "${RETIRED[@]}"; do
  if [ -e "scripts/verify-$name.sh" ]; then bad "scripts/verify-$name.sh still exists"; else ok "verify-$name.sh gone"; fi
done

echo "== 3. Nothing live names them (conductor/archive is history) =="
for name in "${RETIRED[@]}"; do
  hits="$(git grep -l "verify-$name\.sh" -- . ':!conductor/archive' ':!conductor/tracks/retire-scripts_*' ':!scripts/verify-retire-scripts.sh')"
  if [ -z "$hits" ]; then ok "verify-$name.sh: no live references"; else bad "verify-$name.sh named in: $(echo $hits)"; fi
done

echo "== 4. Tools kept =="
for name in "${KEPT[@]}"; do
  if [ -f "scripts/$name.sh" ] && bash -n "scripts/$name.sh"; then
    ok "scripts/$name.sh present, parses"
  else
    bad "scripts/$name.sh missing or does not parse"
  fi
done

echo "== 5. contract_checks gone (no callers) =="
if grep -q 'contract_checks' scripts/lib/verify.sh; then
  bad "scripts/lib/verify.sh still defines contract_checks"
else
  ok "scripts/lib/verify.sh has no contract_checks"
fi

echo "== 6. workflow.md states the rule =="
rule="$(sed -n '/^### Phase Completion/,/^### /p' conductor/workflow.md | grep -m1 'Retiring the script')"
if [ -n "$rule" ]; then ok "Phase Completion: ${rule:0:110}…"; else bad "Phase Completion has no 'Retiring the script' rule"; fi

verify_summary "retire-scripts (gluon TD-46)"
