#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# CI for user-service-rs, defined once: the sandbox runs this script directly
# and GitHub Actions calls it (.github/workflows/ci.yml). Stops at the first
# failing step; each step announces itself with a "== " line. The steps are
# gluon-rs's scripts/lib/ci.sh (gluon TD-43.2), Linux and macOS, plus the
# registry baseline.
# shellcheck source-path=SCRIPTDIR
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# The crates depend on gluon-rs by path (../../lib/gluon-rs in gluon's layout);
# elsewhere, set GLUON_RS.
GLUON_RS="${GLUON_RS:-$PWD/../../lib/gluon-rs}"
if [ ! -f "$GLUON_RS/scripts/lib/ci.sh" ]; then
  echo "FAIL no scripts/lib/ci.sh under $GLUON_RS (set GLUON_RS)"
  exit 1
fi
# shellcheck source=../../../lib/gluon-rs/scripts/lib/ci.sh
. "$GLUON_RS/scripts/lib/ci.sh"

ci_fmt
ci_clippy

# tests/events.rs checks the UserRegistered payload schema against gluon's
# registry (ADR 0028 decision 4): the sandbox layout puts it at ../../registry;
# GitHub Actions checks it out and sets GLUON_REGISTRY.
step "gluon registry baseline"
registry="${GLUON_REGISTRY:-$PWD/../../registry}"
if [ -f "$registry/user-service/registry.json" ]; then
  echo "using $(realpath "$registry")/user-service/registry.json"
else
  echo "FAIL no user-service/registry.json under $registry (set GLUON_REGISTRY)"
  exit 1
fi
export GLUON_REGISTRY="$registry"

ci_coverage
# .cargo/audit.toml's ignores hold only while these are lockfile-only
ci_audit rsa libsqlite3-sys
