#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track cargo-audit_20261110: cargo audit green wherever the RustSec
# advisory database is reachable (the laptop, GitHub Actions). The two ignored
# advisories (.cargo/audit.toml) stay justified only while their crates are in
# Cargo.lock alone, never built; serde_yaml, and with it the unmaintained
# unsafe-libyaml, is gone. Each result is printed as it is found.
#   scripts/verify-cargo-audit.sh [--skip-ci]
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.." || exit 1
. scripts/lib/verify.sh
verify_as_owner "$SELF" "$@"
SCRATCH="$(mktemp -d /tmp/verify-cargo-audit.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
ADVISORY_DB="https://github.com/rustsec/advisory-db"
reachable=false
timeout 20 git ls-remote --exit-code "$ADVISORY_DB" HEAD > /dev/null 2>&1 && reachable=true

echo "== 1. CI (scripts/ci.sh) =="
if [ "${1:-}" = "--skip-ci" ]; then
  echo "skip --skip-ci given"
elif scripts/ci.sh > "$SCRATCH/ci.log" 2>&1; then
  ok "scripts/ci.sh passes"
  if $reachable && grep -q '^SKIP cargo-audit' "$SCRATCH/ci.log"; then
    bad "scripts/ci.sh skipped cargo audit with the advisory database reachable:"
    grep '^SKIP cargo-audit' "$SCRATCH/ci.log" | sed 's/^/     /'
  fi
else
  bad "scripts/ci.sh failed:"
  tail -8 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. The ignored advisories' crates are never built =="
for crate in rsa libsqlite3-sys; do
  if tree="$(cargo tree -i "$crate" -e all --target all --locked -q 2> "$SCRATCH/tree.err")"; then
    expect "cargo tree -i $crate (no path)" "" "$(head -1 <<< "$tree")"
  else
    bad "cargo tree -i $crate failed: $(tail -1 "$SCRATCH/tree.err")"
  fi
done

echo "== 3. serde_yaml is gone =="
for crate in serde_yaml unsafe-libyaml; do
  check "Cargo.lock has no $crate" bash -c "! grep -q '^name = \"$crate\"$' Cargo.lock"
done

echo "== 4. cargo audit =="
if ! $reachable; then
  echo "skip advisory database unreachable ($ADVISORY_DB)"
elif cargo audit --deny warnings > "$SCRATCH/audit.log" 2>&1; then
  ok "cargo audit --deny warnings passes"
else
  bad "cargo audit --deny warnings failed:"
  grep -E '^(ID|Crate):' "$SCRATCH/audit.log" | paste - - | sed 's/^/     /'
fi

verify_summary "cargo audit green"
