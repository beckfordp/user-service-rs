# Spec — RUST-6.2: Probe — consume, decode, skip, trace join, lag and event age

## Overview
gluon PLAN.md RUST-6.2 (ADR 0028 decision 8). The `events-probe` binary
consumes `PROBE_TOPIC` as notification-service's consumers do
(`UserRegisteredConsumer`, `EnvelopeHandler`), with purekafka's consumer
tracing (`KafkaTracing.traced`) and metrics (`EventAge`, `ConsumerLag`).
Judged by `tests/contract` `checks/test_probe.py` (RUST-6.5), green once
RUST-6.3 and 6.4 add the lookup's resilience and the DLQ/supervision.

## Functional Requirements
1. Config from the env contract (`gluon_contract.probe`): `PROBE_*`,
   `OTEL_*`; durations as HOCON; every problem reported at once.
2. rdkafka consumer (librdkafka 2.12, built from source), group
   `PROBE_GROUP_ID`, earliest offset, manual commit after each record.
3. Per record: null value → logged and skipped; undecodable envelope →
   poison (RUST-6.4 dead-letters it); unknown `eventType` → skipped (info log,
   no error); `UserRegistered` → payload decoded (poison if not) → the lookup
   (`GET {PROBE_LOOKUP_URL}/users/{userId}`; 200 resolved, 400/404 unresolved).
4. A CONSUMER span `<topic> process` per record (`messaging.system`,
   `messaging.destination.name`), joining the record's `traceparent`.
5. Metrics on `PROBE_METRICS_PORT`: `purekafka_consumer_event_age_seconds`
   (purekafka's 15 buckets, destination and group labels) per record;
   `purekafka_consumer_lag` gauge (destination, partition, group) from
   librdkafka's statistics every `PROBE_LAG_INTERVAL`.

## Acceptance Criteria
- CI green; `scripts/verify-probe.sh` (added here, extended by 6.3/6.4)
  reports the probe group, the checks this item satisfies green.
