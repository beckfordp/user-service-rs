# Plan — RUST-2.2: CI in scripts/ci.sh

## Phase 1: Toolchain and workspace skeleton [checkpoint: 5a8b866]

- [x] Task: Conductor - User Manual Verification 'Phase 1' — user reviewed with `sr rust` and approved (2026-11-09)

*Deviations (2026-11-09): `clippy.toml` allows unwrap/expect/panic in tests (instead of per-test `cfg_attr`); `missing_docs` added to the shared lints (workflow.md: public items documented); sandbox-setup now installs the pinned 1.99.1 toolchain, not "stable". cargo-llvm-cov counts `main.rs` through the binary tests: 100% lines.*

- [x] Task: tech stack first — toolchain pin, workspace lints, cargo-llvm-cov, the cargo-audit deviation; gluon `bin/sandbox-setup` + `docs/sandbox.md` install and record `llvm-tools-preview` and `cargo-llvm-cov` 604bd99
- [x] Task: write failing placeholder tests in each member (red: no workspace) 711ac41
- [x] Task: workspace — root `Cargo.toml` (members, shared lints), `rust-toolchain.toml`, the three members, `Cargo.lock`; `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` green e2f0daa

## Phase 2: CI script and verification [checkpoint: c613596]

*Order changed to be test-first: the verify script is the test for `ci.sh`, so it came first and failed (10 of 12, `scripts/ci.sh` missing). Findings: cargo-llvm-cov 0.9.1 exits 1 below `--fail-under-lines` with no message, so `ci.sh` prints the covered share and the gate; the audit step checks the database first, so the sandbox's SKIP gives the fundamental reason.*

- [x] Task: `scripts/verify-rust-ci.sh` — written first (red: `scripts/ci.sh` missing); ci.sh green on the repo, then each gate shown stopping a broken change on a scratch copy (formatting, a clippy warning, `unwrap()` outside a test, a missing doc comment, unsafe code, a failing test, coverage under 80%), and the audit step runs or says why not bcebea3
- [x] Task: `scripts/ci.sh` (fmt, clippy `-D warnings`, llvm-cov with the 80% line gate and its own failure message, audit or an explicit SKIP) — verify: PASS, 16 checks 9ac5684
- [x] Task: `.github/workflows/ci.yml` calling it (toolchain from `rust-toolchain.toml`, cargo-llvm-cov 0.9.1 and cargo-audit via install-action); README "Build and check" 070bd22
- [x] Task: Conductor - User Manual Verification 'Phase 2' (Protocol in workflow.md) — `scripts/verify-rust-ci.sh` PASS; the user's review is deferred to the end of the run (2026-11-09 instruction)
