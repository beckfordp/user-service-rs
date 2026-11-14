#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Verifies track rust-ci_20261109 (gluon PLAN.md RUST-2.2): CI for user-service-rs.
#   1. Prerequisites: the pinned toolchain, cargo-llvm-cov, scripts/ci.sh
#   2. scripts/ci.sh passes on the repo, step by step
#   3. Each gate stops a broken change, shown on a scratch copy: formatting, a
#      clippy warning, an unwrap() outside tests, a missing doc comment, unsafe
#      code, a failing test, coverage under 80%
#   4. cargo-audit: runs where its advisory database is reachable, otherwise a
#      SKIP line that says why
# Each result is printed as it is found; the exit status is 0 only if all pass.
#
#   scripts/verify-rust-ci.sh
#
# Runs as the repo's owner; from the sandbox's root desktop terminal it re-runs
# itself as that user, so no root-owned files land in the repo or its target/.
# Linux and macOS (gluon TD-43.2).
# shellcheck source-path=SCRIPTDIR
set -uo pipefail
SELF="$(realpath "${BASH_SOURCE[0]}")"
cd "$(dirname "$SELF")/.." || exit 1
REPO="$PWD"
GLUON_RS="$(realpath "${GLUON_RS:-../../lib/gluon-rs}")"
# shellcheck source=../../../lib/gluon-rs/scripts/lib/verify.sh
. "$GLUON_RS/scripts/lib/verify.sh" || exit 1
verify_as_owner "$SELF" "$@"

SCRATCH="$(mktemp -d /tmp/verify-rust-ci.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT

echo "== Prerequisites =="
toolchain="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"
rustc_version="$(rustc --version 2> /dev/null)"
if [[ "$rustc_version" == "rustc $toolchain "* ]]; then
  ok "toolchain from rust-toolchain.toml: $rustc_version"
else
  bad "toolchain: expected $toolchain, got '$rustc_version'"
fi
llvm_cov="$(cargo llvm-cov --version 2> /dev/null)"
if [ -n "$llvm_cov" ]; then ok "$llvm_cov"; else bad "cargo-llvm-cov not installed (gluon bin/sandbox-setup)"; fi
if [ -x scripts/ci.sh ]; then ok "scripts/ci.sh present"; else bad "scripts/ci.sh missing or not executable"; fi

echo "== 1. scripts/ci.sh on the repo =="
echo "     (the first run compiles the workspace twice: once plain, once instrumented)"
scripts/ci.sh > "$SCRATCH/ci.log" 2>&1
code=$?
if [ "$code" = 0 ]; then
  while read -r step; do ok "step passed: ${step#== }"; done < <(grep '^== ' "$SCRATCH/ci.log")
  total="$(grep -E '^TOTAL' "$SCRATCH/ci.log" | awk '{print $(NF-3)}')"
  if [ -n "$total" ]; then ok "line coverage: $total"; else bad "no coverage summary in the output"; fi
else
  bad "scripts/ci.sh failed (exit $code); its last lines:"
  tail -15 "$SCRATCH/ci.log" | sed 's/^/     /'
fi

echo "== 2. Each gate stops a broken change (on a scratch copy) =="
export CARGO_TARGET_DIR="$SCRATCH/target" # shared by the copies, so only changes rebuild
# The copies sit in gluon's layout, so the crates' path dependency on
# ../../lib/gluon-rs and ci.sh's ../../registry resolve as they do here.
mkdir -p "$SCRATCH/lib" "$SCRATCH/services"
ln -s "$GLUON_RS" "$SCRATCH/lib/gluon-rs"
ln -s "$(realpath "${GLUON_REGISTRY:-../../registry}")" "$SCRATCH/registry"

# broken DESCRIPTION EXPECTED_STEP EXPECTED_TEXT MUTATION_FUNCTION
broken() {
  local description="$1" step="$2" text="$3" mutation="$4"
  local copy="$SCRATCH/services/user-service-rs"
  rm -rf "$copy"
  mkdir -p "$copy"
  tar -C "$REPO" --exclude=./target --exclude=./.git -cf - . | tar -C "$copy" -xf -
  (cd "$copy" && "$mutation")
  local out code stopped_at
  out="$(cd "$copy" && scripts/ci.sh 2>&1)"
  code=$?
  stopped_at="$(grep '^== ' <<< "$out" | tail -1)"
  if [ "$code" = 0 ]; then
    bad "$description: CI passed, but it should have failed"
  elif [[ "$stopped_at" != "== $step"* ]]; then
    bad "$description: stopped at '${stopped_at#== }', expected '$step'"
  elif ! grep -q -- "$text" <<< "$out"; then
    bad "$description: stopped at '$step' but without '$text'; last lines:"
    tail -6 <<< "$out" | sed 's/^/     /'
  else
    ok "$description: stopped at '$step' ($text)"
  fi
}

LIB=crates/telemetry/src/lib.rs
unformatted() { printf '\n/// Badly formatted.\npub fn   badly_formatted( ) {}\n' >> "$LIB"; }
clippy_warning() {
  printf '\n/// Compares a bool with true.\n#[must_use]\npub fn is_true(flag: bool) -> bool {\n    flag == true\n}\n' >> "$LIB"
}
unwrap_outside_tests() {
  printf '\n/// The first byte.\n#[must_use]\npub fn first_byte(bytes: &[u8]) -> u8 {\n    *bytes.first().unwrap()\n}\n' >> "$LIB"
}
missing_doc() { printf '\npub fn undocumented() {}\n' >> "$LIB"; }
unsafe_code() {
  printf '\n/// Reads through a raw pointer.\n#[must_use]\npub fn read(value: &u8) -> u8 {\n    unsafe { *std::ptr::from_ref(value) }\n}\n' >> "$LIB"
}
failing_test() {
  printf '//! A test that fails.\n\n#[test]\nfn fails() {\n    assert_eq!(1 + 1, 3);\n}\n' > crates/telemetry/tests/failing.rs
}
low_coverage() {
  # Enough uncovered lines to pull the whole workspace under 80% (it grows), in
  # small functions so clippy's too_many_lines doesn't stop it first.
  for n in $(seq 1 80); do
    printf '\n/// Never called by any test.\n#[must_use]\npub fn uncovered_%d(seed: u64) -> u64 {\n    let mut value = seed;\n' "$n"
    for i in $(seq 1 8); do printf '    value = value.wrapping_mul(%d).wrapping_add(1);\n' "$i"; done
    printf '    value\n}\n'
  done >> "$LIB"
}

broken "badly formatted code" "cargo fmt --check" "Diff in" unformatted
broken "a clippy warning" "cargo clippy" "bool_comparison" clippy_warning
broken "unwrap() outside a test" "cargo clippy" "unwrap_used" unwrap_outside_tests
broken "a public item without a doc comment" "cargo clippy" "missing documentation" missing_doc
broken "unsafe code" "cargo clippy" "usage of an \`unsafe\` block" unsafe_code
broken "a failing test" "tests with coverage" "test result: FAILED" failing_test
broken "coverage under 80%" "tests with coverage" "below the 80% gate" low_coverage

echo "== 3. cargo-audit =="
audit="$(grep -E '^(SKIP cargo-audit|Crate|Success|    Scanning)' "$SCRATCH/ci.log" | head -1)"
if grep -q '^SKIP cargo-audit' <<< "$audit"; then
  ok "skipped, with the reason: ${audit#SKIP cargo-audit: }"
elif grep -q '^== cargo audit' "$SCRATCH/ci.log"; then
  ok "ran (advisory database reachable)"
else
  bad "neither ran nor said why it was skipped"
fi

verify_summary "RUST-2.2 CI"
