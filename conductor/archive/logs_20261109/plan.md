# Plan — RUST-3.4

## Phase 1: telemetry crate
- [x] Task: write failing tests (line shape and order, levels, timestamp format, escaping, trace ids inside a span) `ac5a9ee`
- [x] Task: implement `telemetry::logging` `cac0b06`

## Phase 2: the service
- [x] Task: write failing tests (end to end: startup line, outcomes, canonical path, nothing for undecodable requests, no secrets) `640e119`
- [x] Task: wire it in (startup line, handlers' log calls, errors) `73bbf62`
- [x] Task: `scripts/verify-logs.sh` — PASS; contract logs 16/16 `1c55588`
