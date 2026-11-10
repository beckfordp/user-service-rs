# Spec — RUST-3.2: Tracing

## Overview
gluon PLAN.md RUST-3.2 (ADR 0028 "Traces" row, ADR 0021). Behaviour of purerest
`Tracing`/`TracingConfig`/`ServerTracing` as user-service uses them, in the
`telemetry` crate (the seed of a Rust purerest). Harness group: `tests/contract`
`checks/test_traces.py` (RUST-1.7, RUST-1.9) and the Debezium routing check of
`checks/test_outbox.py`. Sources: capture 20261109 `stdout.log` (spans),
`db-outbox-events.jsonl`; purerest tracing sources; user-service `AppRoutes`,
`UserStore.insertOutbox`.

## Functional Requirements
1. Export: `OTEL_EXPORTER_OTLP_ENDPOINT` set (non-blank) → OTLP/HTTP protobuf,
   batched, to `<endpoint>/v1/traces` (not doubled when already there);
   unset → spans printed to stderr, one line each, as the Java
   LoggingSpanExporter does. Resource `service.name` = `USER_SERVICE_NAME`;
   instrumentation scope = the service name. Sampler: parent-based, roots at
   `OTEL_TRACES_SAMPLER_ARG` (default 1). Shutdown flushes.
2. Server span per request: name `<METHOD> <raw path>`, kind SERVER, continues an
   inbound W3C `traceparent`, else a new root; attributes `http.request.method`,
   `url.path`, and `http.response.status_code` (int) when a route answered — not
   for an unmatched path; status ERROR for 5xx only. CORS answers preflights
   outside tracing (no span).
3. `outbox.write`: INTERNAL child of the request span around the outbox insert,
   attributes `outbox.event_type`, `outbox.aggregate_id`; the row's
   `trace_parent` is its W3C context (sampled flag as decided).

## Acceptance Criteria
- `scripts/verify-tracing.sh` PASS: CI green; contract traces group green; the
  outbox group's Debezium routing check green; config group's service.name
  check green.
