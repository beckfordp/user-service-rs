# Plan — RUST-5.0 follow-up: docs.yaml Content-Type

## Phase 1: Content-Type
- [x] Task: `/docs/docs.yaml` sends `application/yaml` `4297774`
  - [x] Red: change both tests' expectation to `application/yaml`; `cargo test` shows them failing
  - [x] Green: change `swagger.rs`; both tests pass; clippy and fmt clean
- [ ] Task: `scripts/verify-yaml-content-type.sh`
  - [ ] CI; curl the header from a started service; full harness against Rust (all groups incl. probe); gluon-contract `scripts/ci.sh`
  - [ ] Failure path proven once on a broken copy
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
