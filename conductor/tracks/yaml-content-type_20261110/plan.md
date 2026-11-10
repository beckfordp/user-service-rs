# Plan — RUST-5.0 follow-up: docs.yaml Content-Type

## Phase 1: Content-Type [checkpoint: 8042f72]
- [x] Task: `/docs/docs.yaml` sends `application/yaml` `4297774`
  - [x] Red: change both tests' expectation to `application/yaml`; `cargo test` shows them failing
  - [x] Green: change `swagger.rs`; both tests pass; clippy and fmt clean
- [x] Task: `scripts/verify-yaml-content-type.sh` `bad7586` — `--skip-ci` PASS (132 checks: harness 129/129, gluon-contract CI green); failure path shown (charset expectation in a copy: exit 1, header named); also `verify_as_owner` on macOS. Full CI red on `cargo audit` only (pre-existing, lockfile-only sqlx drivers + dev-only `unsafe-libyaml`): separate track
  - [x] CI; curl the header from a started service; full harness against Rust (all groups incl. probe); gluon-contract `scripts/ci.sh`
  - [x] Failure path proven once on a broken copy
- [x] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)

## Phase: Review Fixes
- [x] Task: Apply review suggestions eeba869
