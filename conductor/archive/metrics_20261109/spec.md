# Spec — RUST-3.3: Metrics on 9090, and ids parsed as the JVM parses them

## Overview
gluon PLAN.md RUST-3.3 (ADR 0028 "Metrics" row, ADR 0027). purerest
`Metrics.oteljava`, `ServerMetrics`, `PoolMetrics`, `LatencyBuckets` and
user-service's `UserStore` timing and `UserJourneyMetrics`, in the `telemetry`
crate and the service. Harness: `tests/contract` `checks/test_metrics.py`
(RUST-1.7, RUST-1.9) and `test_rest.py`'s Java-UUID check. Sources: capture
20261109 `metrics.txt`; purerest and user-service sources; the JDK
(`java.util.UUID.fromString`, checked with known-answer vectors).

## Functional Requirements
1. A Prometheus text endpoint on `0.0.0.0:<USER_SERVICE_METRICS_PORT>`
   (`/metrics`, also `/`), as the OTel Java Prometheus exporter writes it:
   OTel names converted (dots → `_`, unit `s` → `_seconds`, counters
   `_total`), `# TYPE` lines, no `# HELP` for undescribed instruments,
   families and labels sorted, `otel_scope_name` = the service name, families
   with no data omitted.
2. `http_server_request_duration_seconds` (histogram, the 24 latency buckets,
   5 ms–10 s): per request, `http_request_method`, `http_route` (the raw path
   with every segment `UUID.fromString` accepts as `{id}`) and
   `http_response_status_code` — absent for an unmatched path; measured
   outside tracing, inside CORS (a preflight isn't recorded).
3. `db_client_operation_duration_seconds` (same buckets): `db_system`
   `postgresql`, `db_operation` `ping|insert|select` per store call (signup and
   login tokens are `insert`, lookups `select`), timed around acquisition and
   the work; a failure adds `error_type`.
4. Pool: `db_client_connection_wait_time_seconds` (same buckets),
   `db_client_connection_count{state=used}` and
   `db_client_connection_pending_requests` (gauges), pool name `user`.
5. `users_registrations_total{outcome=created|duplicate}`; `users_logins_total`
   (successful logins, counted when their tokens are stored).
6. `GET /users/{id}` parses the id as `java.util.UUID.fromString` does
   (lenient groups, `+` signs, Unicode digits; no undashed/braced/URN forms).

## Acceptance Criteria
- `scripts/verify-metrics.sh` PASS: CI green; contract metrics group green;
  the REST group's Java-UUID check green.
