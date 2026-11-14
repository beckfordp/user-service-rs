# Plan — On lib/gluon-rs (gluon TD-43.1)

A refactor: the existing tests are the guard, run before and after each task.

## Phase 1: onto gluon-rs
- [x] Task: Path dependencies, `config` and `main` (guard: `tests/config.rs`, `startup.rs`) e57d1cc
- [x] Task: `db`, `server` and `responses` (guard: health, readiness, metrics, tracing, logs tests) e88f040
- [x] Task: `tests/support` onto service-testing e202f2e
- [x] Task: Dockerfile on the gluon-rs build context c35a833
    - [x] Red: `docker build` without the context fails on the path dependency
    - [x] Green: with it
- [x] Task: Backlog item: the CI workflow's gluon-rs checkout c4127dc
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
    - [ ] `scripts/ci.sh` green
    - [ ] `docker build --build-context gluon-rs=../../lib/gluon-rs .`
    - [ ] gluon `bin/verify-port user-service --only build contract`: 128/128
    - [ ] after gluon's `bin/k8s-local-up` change: the Rust image deployed,
          `bin/run-acceptance` 53/53
