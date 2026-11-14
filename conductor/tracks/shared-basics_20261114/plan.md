# Plan — On lib/gluon-rs (gluon TD-43.1)

A refactor: the existing tests are the guard, run before and after each task.

## Phase 1: onto gluon-rs
- [x] Task: Path dependencies, `config` and `main` (guard: `tests/config.rs`, `startup.rs`) e57d1cc
- [ ] Task: `db`, `server` and `responses` (guard: health, readiness, metrics, tracing, logs tests)
- [ ] Task: `tests/support` onto service-testing
- [ ] Task: Dockerfile on the gluon-rs build context
    - [ ] Red: `docker build` without the context fails on the path dependency
    - [ ] Green: with it
- [ ] Task: Backlog item: the CI workflow's gluon-rs checkout
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
    - [ ] `scripts/ci.sh` green
    - [ ] `docker build --build-context gluon-rs=../../lib/gluon-rs .`
    - [ ] gluon `bin/verify-port user-service --only build contract`: 128/128
    - [ ] after gluon's `bin/k8s-local-up` change: the Rust image deployed,
          `bin/run-acceptance` 53/53
