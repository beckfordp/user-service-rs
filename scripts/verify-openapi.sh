#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track openapi_20261109 (gluon PLAN.md RUST-4.1): the OpenAPI spec at
# /docs/docs.yaml and Swagger UI at /docs, judged by the contract harness's
# docs group (tests/contract checks/test_docs.py, the non-AsyncAPI checks:
# page, initializer, assets, spec parsed-equal to the capture, responses
# conforming as the Scala ones do) and the REST group (still green). Each
# result is printed as it is found.
#   scripts/verify-openapi.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.."
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-openapi.XXXXXX)"
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

echo "== 2. Swagger UI is the webjar the Scala service serves =="
manifest=crates/telemetry/assets/swagger-ui/MANIFEST
if head -1 "$manifest" | grep -q "swagger-ui 5.20.7 webjar, SHA-1 b21926acb43c826cb6f96fe98f9dae51609947d5"; then
  ok "vendored: $(head -1 "$manifest" | sed 's/^# //')"
else
  bad "unexpected vendored Swagger UI: $(head -1 "$manifest")"
fi

echo "== 3. Swagger UI and the OpenAPI spec (contract docs group) =="
contract_checks checks/test_docs.py -k "not asyncapi"

echo "== 4. REST group still green (contract) =="
contract_checks checks/test_rest.py

verify_summary "RUST-4.1 OpenAPI and Swagger UI"
