# user-service-rs

A wire-compatible Rust port of [user-service](../user-service), built as a
time-boxed spike per gluon
[ADR 0028](../../docs/adr/0028-rust-port-spike-user-service.md). It is a
drop-in replacement: same REST, docs, outbox, schema, config and
observability contracts, deployed under the same Helm release
(`user-service`) when chosen in gluon-local.

**Status (2026-11-10):** every contract group of the harness is green against
this service in isolated mode (119 checks in `tests/contract`), and the
consumer probe's group too. The laptop pass (the image, gluon-local, the
harness against the Scala service) is prepared but not run. How to verify it
all: gluon [`docs/ports/user-service-verification.md`](../../docs/ports/user-service-verification.md);
what was learned: [`docs/ports/user-service-findings.md`](../../docs/ports/user-service-findings.md).
Work is planned in [`conductor/tracks.md`](./conductor/tracks.md) and
sequenced in gluon [`plans/rust-port/user-service/plan.md`](../../plans/rust-port/user-service/plan.md).

## Build and check

```
cargo build                       # the toolchain is pinned in rust-toolchain.toml
cargo test
scripts/ci.sh                     # what CI runs: fmt, clippy -D warnings, tests with
                                  # the 80% line-coverage gate, cargo-audit (or a SKIP
                                  # saying why: its database is on GitHub)
scripts/verify-rust-ci.sh         # RUST-2.2 verified: CI passes, and each gate is shown
                                  # stopping a broken change on a scratch copy
scripts/asyncapi-html.sh          # the AsyncAPI HTML /docs/asyncapi/ serves (Node >= 24.11)
docker build -t user-service-rs:latest .   # the image (laptop; gluon bin/k8s-local-up
                                  # builds it with USER_SERVICE_IMPL=rust)
```

CI needs gluon's `registry/` for the payload schema test: at `../../registry`
(this repo inside a gluon checkout) or wherever `GLUON_REGISTRY` points.

The sandbox's toolchain and cargo-llvm-cov come from gluon `bin/sandbox-setup`.
Each track has a `scripts/verify-<track>.sh` that checks what it set out to do,
reporting each result as it finds it; at archive it is retired once CI and the
harness cover it (`conductor/workflow.md`). Regression: `scripts/ci.sh` here,
gluon `bin/verify-port user-service` across repos.

## How it is judged
The specified contracts are the reference: the documented contracts, the
Scala-generated OpenAPI/AsyncAPI artifacts, the Scala repo's test suites and
`verify-*.sh` scripts, and behaviour captured from the Scala service on
request. The black-box harness in gluon's `tests/contract` checks this
service against them in the sandbox, and the Scala service on the laptop;
this port is done when every group is green for both.

Developed and tested in the sandbox (Rust, Postgres binaries, no Docker);
the Docker image and the gluon-local deployment come in the laptop pass
(PLAN.md RUST-5).

## Layout
A cargo workspace (see [`conductor/tech-stack.md`](./conductor/tech-stack.md)),
members under `crates/`:
- `user-service` — the service binary
- `telemetry` — tracing, metrics, logs and error envelope; the seed of a Rust
  purerest equivalent
- `events-probe` — a throwaway consumer, run only inside the contract harness
  (ADR 0028 decision 8). Its env contract: `PROBE_KAFKA_BOOTSTRAP`,
  `PROBE_TOPIC`, `PROBE_GROUP_ID`, `PROBE_LOOKUP_URL`, `PROBE_METRICS_PORT`,
  `PROBE_SERVICE_NAME`, the resilience settings (`PROBE_LOOKUP_TIMEOUT`,
  `PROBE_LOOKUP_MAX_RETRIES`, `PROBE_LOOKUP_BASE_DELAY`,
  `PROBE_BREAKER_FAILURE_THRESHOLD`, `PROBE_BREAKER_RESET_TIMEOUT`,
  `PROBE_DLQ_MAX_RETRIES`, `PROBE_DLQ_BASE_DELAY`, `PROBE_RESTART_BASE_DELAY`,
  `PROBE_RESTART_MAX_DELAY`, `PROBE_LAG_INTERVAL`; notification-service's
  defaults) and `OTEL_*`
