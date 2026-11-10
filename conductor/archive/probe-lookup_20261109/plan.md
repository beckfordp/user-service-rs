# Plan — RUST-6.3

## Phase 1
- [x] Task: write failing tests (retry, breaker state machine and metrics, traced client, resilient client against a local server) `aef38eb`
- [x] Task: implement `telemetry::resilience`, `telemetry::client`; the probe's lookup on them `b8e2c05`
- [x] Task: `scripts/verify-probe.sh --only "lookup_is_made or slow_lookup or unknown_user or unknown_event_type"` — PASS (5 checks); the transient-failure check needs 6.4's processing retry (a reset isn't retried at the HTTP layer, as in purerest)
