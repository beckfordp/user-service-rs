# Spec — RUST-2.2: CI in scripts/ci.sh

## Overview
Give `user-service-rs` its CI (gluon PLAN.md RUST-2.2, ADR 0028 decision 4):
one script, `scripts/ci.sh`, that the sandbox runs directly and GitHub Actions
will call once the repo is pushed (RUST-5.6). It needs something to build, so
this track also lays down the cargo workspace skeleton ADR 0028 names — three
members, each with a placeholder test — which RUST-2.3 fills in.

## Functional Requirements
1. **Toolchain:** `rust-toolchain.toml` pins the Rust the sandbox has
   (1.99.1) with `rustfmt`, `clippy` and `llvm-tools-preview`, profile
   minimal.
2. **Workspace skeleton:** a root `Cargo.toml` (resolver 3, edition 2024,
   shared `[workspace.lints]`: `unsafe_code = "forbid"`, clippy `all` and
   `pedantic` warnings, `unwrap_used`/`expect_used`/`panic` denied outside
   tests) with members `user-service` (binary), `telemetry` (library),
   `events-probe` (binary). Each has one placeholder function and a test.
   `Cargo.lock` committed.
3. **`scripts/ci.sh`**, stopping at the first failure, in this order:
   `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked
   -- -D warnings`; `cargo llvm-cov --workspace --locked --fail-under-lines 80`
   (runs the tests once, with coverage); `cargo audit` when `cargo-audit` is
   installed and the RustSec advisory database is reachable, otherwise an
   explicit `SKIP` line saying why.
4. **GitHub Actions:** `.github/workflows/ci.yml` installs the pinned toolchain,
   cargo-llvm-cov and cargo-audit, and runs `scripts/ci.sh` (deferred with the
   remote to RUST-5.6; written now so the script stays the single definition).
5. **Sandbox setup:** gluon `bin/sandbox-setup` installs `llvm-tools-preview`
   and `cargo-llvm-cov`; `docs/sandbox.md` records them and why
   `cargo-audit` isn't installed there.
6. **`scripts/verify-rust-ci.sh`** (the phase verification, per workflow.md):
   runs `scripts/ci.sh`, then proves each gate fails when it should, on a
   scratch copy of the workspace: misformatted code, a clippy warning, a
   failing test, coverage below 80%, and an `unwrap()` outside a test. Reports
   each result as it finds it.

## Non-Functional Requirements
- No Docker, no network beyond crates.io; everything runs in the sandbox
- The gates are exactly the ones in `product-guidelines.md` and `workflow.md`

## Acceptance Criteria
- `scripts/ci.sh` green in the sandbox, with the audit `SKIP` line
- `scripts/verify-rust-ci.sh` PASS: every gate shown to fail on a broken copy

## Out of Scope
- Real crate contents: config, `/health`, Postgres (RUST-2.3 onwards)
- Pushing and running Actions (RUST-5.6)

## Deviation recorded
`cargo-audit` can't run in the sandbox: the RustSec advisory database is on
GitHub (`github.com`, `raw.githubusercontent.com` and `rustsec.org` are
unreachable, checked 2026-11-09). Actions runs it; ADR 0013 (dependency
scanning) is still undecided, so this remains the interim check.
