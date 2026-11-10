# Plan — RUST-3.3

## Phase 1: telemetry crate
- [x] Task: JDK known-answer vectors for `UUID.fromString` (`scripts/java-uuid-vectors.sh`) `582a2f1`
- [x] Task: write failing tests (Java UUID parse, route template, Prometheus naming and text, server middleware, pool instrument) `b376f66`
- [x] Task: implement `telemetry::jvm` and `telemetry::metrics` `af7d506`

## Phase 2: the service
- [x] Task: write failing tests (end to end: the families and labels after traffic, journey counters, db operations, Java-UUID ids) `d9d2749`
- [x] Task: wire it in (metrics server, middleware order, timed store, pool, counters, id parsing) `b9c1aea`
- [x] Task: `scripts/verify-metrics.sh` — PASS; contract metrics 7/7, REST 42/42 `56a9d4c`
