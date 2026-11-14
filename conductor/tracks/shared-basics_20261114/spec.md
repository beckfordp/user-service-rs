# Spec: On lib/gluon-rs (gluon TD-43.1)

## Overview
user-service-rs drops its copies of the shared pieces and uses `lib/gluon-rs`
by path. No behaviour change; its image still builds.

## Functional Requirements
- FR-1 `main.rs`: `environment::variables`, `runtime::multi_thread`,
  `shutdown::signal("user-service")`; tracing setup and shutdown unchanged.
- FR-2 `config.rs` keeps `Config` (metrics port, auth, tracing, AsyncAPI dir)
  and its validations; uses `config::port`, `config::postgres(env, "user")`,
  and `ConfigError`/`PostgresConfig` (re-exported).
- FR-3 `db.rs` keeps `migrate`; `connect(config, POOL_SIZE)` (10, citing the
  Scala skunk pool, as today).
- FR-4 `server.rs`: `health::routes`. `AppState` implements `Readiness`
  through `Store::ping`, which stays as it is: it acquires through the pool
  metrics and is timed, so `db.operation=ping` and the pool metrics are
  unchanged. This departs from the TD plan, which had it use service-basics'
  ping. `method_not_allowed` comes from service-basics; `not_found` keeps its
  `Unmatched` marker around the shared `TextError`.
- FR-5 `responses.rs`: `TextError` and `TEXT_PLAIN` from service-basics
  (re-exported for `auth.rs`, `users.rs`); `json_error` and `JsonBody` stay.
- FR-6 `tests/support/mod.rs`: a `Service` newtype over
  `service_testing::Service` (`Deref`) with `metrics_port`, `start`/`spawn(extra)`
  setting both ports; `Postgres` and `free_port` re-exported; `otlp.rs` stays.
  The test files are unchanged.
- FR-7 Docker: the Dockerfile builds in `/src/services/user-service-rs`, with
  gluon-rs's `Cargo.toml`, `Cargo.lock` and `crates/` copied from a `gluon-rs`
  named build context to `/src/lib/gluon-rs` (no `target/`); the binaries are
  copied from the new target path.

## Acceptance Criteria
- `scripts/ci.sh` green; no copy of a moved piece left.
- The image builds with `docker build --build-context gluon-rs=…`.

## Out of Scope
- gluon's `bin/k8s-local-up` and `bin/verify-laptop-prep` changes (in gluon,
  after this track, with approval).
- The GitHub Actions checkout of gluon-rs (backlog: gluon-rs has no GitHub
  remote yet).
