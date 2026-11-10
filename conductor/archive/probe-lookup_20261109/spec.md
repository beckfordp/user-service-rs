# Spec — RUST-6.3: Probe — the resilience-wrapped lookup

## Overview
gluon PLAN.md RUST-6.3 (ADR 0028 decision 8; ADR 0025). purerest's client
side in the `telemetry` crate — `Retry`, `CircuitBreaker` (resilience4j's
count-based breaker as purerest configures it), `Resilience` (retry outside
the breaker), `ClientTracing`, `ClientMetrics` — and the probe's lookup built
on it, as notification-service's `UserClient` is. Judged by the probe group's
lookup, retry, timeout, breaker and metric-name checks.

## Functional Requirements
1. Retry: up to `max_retries` more attempts, delays `base × 2ⁿ`; for HTTP,
   only 5xx responses, connection failures and timeouts are retried (not 4xx,
   not a reset mid-response, not an open breaker); exhausted 5xx returns the
   last response. `<prefix>.retry.attempts{outcome=retried|succeeded|exhausted}`.
2. Circuit breaker: the last `failure_threshold` calls all failing (5xx or an
   error) open it; open rejects without calling for `reset_timeout`, then one
   half-open trial decides. `<prefix>.circuit_breaker.state{state}` (1 for the
   current state), `.state_transitions{from_state,to_state}`, `.calls_rejected`.
3. Client: with a span active, a CLIENT span `GET <host>` (`http.request.method`,
   `server.address`, `url.path`, `http.response.status_code`; ERROR on 5xx or
   an error) whose context goes out as `traceparent`; always
   `http.client.request.duration{http.request.method, server.address,
   http.response.status_code | error.type}`.
4. The probe's lookup: the request timeout, then client, breaker and retry as
   above with prefix `purerest`; 200 resolved, 400/404 unresolved, anything
   else raises.

## Acceptance Criteria
- CI green; `scripts/verify-probe.sh --only` the lookup/trace, retry, timeout
  and 404 checks green.
