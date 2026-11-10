# Plan — RUST-6.4

## Phase 1
- [x] Task: write failing tests (the DLQ guard with a fake publisher; the supervisor's backoff and restarts) `60ffecd`
- [x] Task: implement `dead_letter.rs`, `supervisor.rs`; wire them into the loop with a DLQ producer `6f1143f`
- [x] Task: `scripts/verify-probe.sh` PASS — the whole probe group, 10/10
