# Plan — RUST-2.4: Baseline migration, pool, /health/ready

## Phase 1
- [x] Task: `tests/support` (throwaway Postgres, service launcher); startup tests moved onto it
- [x] Task: write failing readiness tests (schema migrated, restart on a migrated database, 200/503/200, unreachable database exits 1)
- [x] Task: implement the migration, pool, readiness and startup failure
- [x] Task: `scripts/verify-readiness.sh` (contract checks run from `scripts/lib/verify.sh`) — PASS
