# Plan — RUST-3.1

## Phase 1
- [x] Task: write failing tests (envelope, payload schema vs registry, outbox row per signup, none on refusal) `77fd289`
- [x] Task: implement (`events.rs`, outbox insert in the signup transaction) `d7d945b`
- [x] Task: `scripts/verify-outbox.sh` — PASS; contract outbox group 4/5, Debezium routing waits for the traceparent header (RUST-3.2) `55668db`
