#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track asyncapi_20261109 (gluon PLAN.md RUST-4.2): the AsyncAPI
# document at /docs/asyncapi.json and its HTML at /docs/asyncapi/, rendered by
# scripts/asyncapi-html.sh, judged by the contract harness's docs group (all of
# it: with RUST-4.1 done, the whole group must be green). Each result is
# printed as it is found.
#   scripts/verify-asyncapi.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-asyncapi.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
CAPTURE="$(realpath ../../tests/contract/contracts/user-service/20261109)"

echo "== 1. CI (scripts/ci.sh) =="
if [ "${1:-}" = "--skip-ci" ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes ($(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}') of lines covered)"
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. The spec is the Scala build's, byte for byte =="
if cargo build -q --locked -p user-service --bin asyncapi-spec > "$SCRATCH/build.log" 2>&1 \
  && cmp -s <(target/debug/asyncapi-spec) <(cat "$CAPTURE/asyncapi.json"; echo); then
  ok "asyncapi-spec output = capture asyncapi.json"
else
  bad "asyncapi-spec output differs from capture asyncapi.json"
fi

echo "== 3. The HTML (scripts/asyncapi-html.sh) is the Scala build's, byte for byte =="
if scripts/asyncapi-html.sh > "$SCRATCH/html.log" 2>&1; then
  ok "rendered: $(tail -1 "$SCRATCH/html.log" | sed 's/^asyncapi-html: //')"
  for file in index.html css/asyncapi.min.css css/global.min.css js/app.js js/asyncapi-ui.min.js; do
    if cmp -s "target/asyncapi-docs/$file" "$CAPTURE/asyncapi-html/$file"; then
      ok "$file = capture asyncapi-html/$file"
    else
      bad "$file differs from capture asyncapi-html/$file"
    fi
  done
else
  bad "scripts/asyncapi-html.sh failed:"
  tail -5 "$SCRATCH/html.log" | sed 's/^/     /'
fi

echo "== 4. The whole docs group (contract) =="
contract_checks checks/test_docs.py

verify_summary "RUST-4.2 AsyncAPI JSON and HTML"
