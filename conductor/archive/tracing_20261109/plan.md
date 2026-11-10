# Plan — RUST-3.2

## Phase 1: telemetry crate
- [x] Task: write failing tests (traces URL, traceparent text, middleware spans with an in-memory exporter: name, kind, attributes, join/root, unmatched, 5xx, sampler) `a248c3b`
- [x] Task: implement `telemetry::tracing` `61c748e`

## Phase 2: the service
- [x] Task: write failing tests (end to end against an OTLP sink: request span, outbox.write child, trace_parent column, service.name) `0efc4ca`
- [x] Task: wire it in (main, router layers, outbox span) `dc7a962`
- [x] Task: `scripts/verify-tracing.sh` — PASS; contract traces group 8/8, outbox group 5/5, config group 7/7 `a530ba2`
