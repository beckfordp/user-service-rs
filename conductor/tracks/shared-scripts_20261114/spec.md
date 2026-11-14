# Spec: CI and verify scripts on gluon-rs's library, Linux and macOS (gluon TD-43.2)

## Overview
The scripts copy what gluon-rs's `scripts/lib/` now holds, and two break on
macOS:
- `verify-rust-ci.sh` stops at `stat -c %U` (GNU only), so gluon
  `bin/verify-port`'s tracks step is red on the laptop. Its scratch copy in
  `/tmp` also can't build since the crates depend on `../../lib/gluon-rs`
  by path (TD-43.1).
- `ci.sh` and `verify-cargo-audit.sh` use coreutils `timeout`: on a stock Mac
  they say the advisory database is unreachable when it isn't.

## Functional Requirements
- FR-1 `scripts/ci.sh` on `${GLUON_RS:-../../lib/gluon-rs}/scripts/lib/ci.sh`:
  `ci_fmt`, `ci_clippy`, the gluon registry baseline (its own), `ci_coverage`,
  `ci_audit rsa libsqlite3-sys`.
- FR-2 `verify-cargo-audit.sh` on gluon-rs's `verify.sh`, `with_timeout`.
- FR-3 `verify-rust-ci.sh` on gluon-rs's `verify.sh` (`verify_as_owner`,
  `ok`/`bad`, `verify_summary`); its scratch copy laid out as gluon is
  (`services/user-service-rs`, with `lib/gluon-rs` and `registry` linked).
- FR-4 `scripts/lib/verify.sh` removed; workflow.md's TD-46 rule points at
  gluon-rs's.

## Acceptance Criteria
- `scripts/ci.sh` green on the normal PATH and a stock-macOS PATH, cargo
  audit running on both.
- `verify-cargo-audit.sh` and `verify-rust-ci.sh` PASS on macOS.
- shellcheck clean.

## Out of Scope
- The GitHub workflow's gluon-rs checkout (backlog; needs gluon-rs's remote).
