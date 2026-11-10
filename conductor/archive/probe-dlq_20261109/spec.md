# Spec — RUST-6.4: Probe — dead-lettering and stream supervision

## Overview
gluon PLAN.md RUST-6.4 (ADR 0028 decision 8). purekafka's `DeadLetter`
(processing retried with `dlq-retry`, then the record's own key and value
published to `<topic>-dlq` through `Resilience` — retry and breaker, prefix
`purekafka` — and `purekafka.dlq.published` counted; if that publish fails
too, the stream fails) and `StreamSupervisor` (the consumer rebuilt after a
failure, the delay doubling from `PROBE_RESTART_BASE_DELAY` to
`PROBE_RESTART_MAX_DELAY`, forever; `purekafka.stream.restarts`), as
notification-service's consumers use them. Judged by the whole probe group.

## Functional Requirements
1. Poison (undecodable envelope or payload) and lookup failures go through
   the DLQ guard; skipped and unresolved records don't.
2. The consume loop fails on librdkafka's "all brokers down" and on a failed
   commit; the supervisor restarts it with capped doubling backoff, logging
   `Consumer stream failed (attempt N), restarting after D`.
3. A DLQ publish is logged `Routed unprocessable message to DLQ after
   exhausting retries` with `topic`, `dlq_topic`, `key`.

## Acceptance Criteria
- CI green; `scripts/verify-probe.sh` (no `--only`): the whole probe group green.
