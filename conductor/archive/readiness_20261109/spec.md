# Spec — RUST-2.4: Baseline migration, pool, /health/ready

## Overview
gluon PLAN.md RUST-2.4, ADR 0028 decision 5. The service owns its schema
through one baseline migration equal to the Scala service's V1 + V2, applied
at startup; it keeps a connection pool; `/health/ready` says whether Postgres
answers. Judged by the contract harness: `checks/test_schema.py`
(schema equivalence) and the health checks in `checks/test_rest.py`.

## Functional Requirements
1. `crates/user-service/migrations/20261109000000_baseline.sql`: the Scala
   service's `V1__create_user_table.sql` and `V2__create_outbox_events.sql`
   verbatim, embedded (`sqlx::migrate!`), applied at startup, recorded in
   `_sqlx_migrations` (ADR 0028 decision 5: its own history table).
2. A pool of 10 connections (as the Scala service's skunk pool); acquiring a
   connection times out after 3 s so readiness can't hang.
3. `GET /health/ready`: 200 with an empty body when `select 1` succeeds within
   2 s, otherwise 503 with an empty body (capture `http/health-ready.txt`,
   openapi.yaml).
4. Database unreachable or migration failing at startup: a message on stderr,
   exit status 1, nothing bound (the Scala service stops when Flyway fails).
5. Tests: a throwaway Postgres from the local binaries (`tests/support`), no
   Docker.

## Acceptance Criteria
- `scripts/verify-readiness.sh` PASS: CI green; the contract harness's schema
  check and health/readiness checks green against the build
