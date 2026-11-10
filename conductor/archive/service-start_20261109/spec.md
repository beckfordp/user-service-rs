# Spec — RUST-2.3: Config contract and /health

## Overview
The service starts the way the Scala service does (gluon PLAN.md RUST-2.3):
it reads the same environment variables with the same defaults, refuses to
start on values the Scala service refuses, and serves `/health`. That makes it
a process the contract harness (RUST-1.2) can start, so the REST checks
(RUST-1.3) can be written against it.

## Functional Requirements
1. **Config (pure):** from an environment map. Source: user-service
   `application.conf`, `UserServiceConfig.validate`, purerest
   `TracingConfig.fromEnv`; gluon `environments/local/user-service.values.yaml`.

   | Variable | Default | Rule |
   |---|---|---|
   | `USER_SERVICE_PORT` | 8080 | integer 0–65535 |
   | `USER_SERVICE_METRICS_PORT` | 9090 | integer 0–65535 |
   | `USER_SERVICE_NAME` | `user-service` | |
   | `POSTGRES_HOST` / `_PORT` / `_DB` / `_USER` / `_PASSWORD` | `localhost` / 5432 / `user` / `user` / `user` | port an integer |
   | `AUTH_ACCESS_TOKEN_TTL` | `1h` | HOCON duration, at least one second |
   | `AUTH_REFRESH_TOKEN_TTL` | `30d` | HOCON duration, at least one second |
   | `AUTH_PBKDF2_ITERATIONS` | 210000 | integer, at least 1 |
   | `OTEL_EXPORTER_OTLP_ENDPOINT` | none | blank means none |
   | `OTEL_TRACES_SAMPLER_ARG` | 1.0 | a number in [0, 1]; malformed fails startup |

   HOCON durations: a number with an optional unit (`ns`, `us`, `ms`, `s`,
   `m`, `h`, `d` and their long forms, spaces allowed); no unit means
   milliseconds. Every invalid value is reported at once.
2. **Startup:** invalid config → one message per problem on stderr, exit
   status 1, nothing bound. Valid → HTTP on `0.0.0.0:<port>`.
3. **`GET /health`:** 200, empty body, no Content-Type (capture
   `http/health.txt`).
4. **Graceful stop** on SIGTERM/SIGINT (the harness stops targets with SIGTERM).

## Non-Functional Requirements
- Config parsing is pure and unit-tested; startup is tested by running the binary
- `scripts/ci.sh` green (coverage ≥ 80%)

## Acceptance Criteria
- `scripts/verify-service-start.sh` PASS: ci.sh green; the binary serves
  `/health` on the configured port; bad config exits 1 listing every problem;
  the contract harness starts it (`CONTRACT_READINESS_PATH=/health`) and stops it

## Out of Scope
- `/health/ready`, Postgres, migrations (RUST-2.4); logs, metrics, tracing (Rust Spike 3)
