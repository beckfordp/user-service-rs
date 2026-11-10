# Spec — RUST-3.1: UserRegistered outbox row and envelope; CI payload schema check

## Overview
gluon PLAN.md RUST-3.1 (ADR 0028 "Outbox and envelope" row). A successful
signup writes one `outbox_events` row in the same transaction as the account;
a refused one writes nothing. Harness group: `tests/contract`
`checks/test_outbox.py` (RUST-1.6). Sources: ADR 0008, ADR 0009,
docs/system-design.md "Envelope" and "UserRegistered", gluon
`registry/user-service/registry.json`, capture 20261109 `db-outbox-events.jsonl`.

## Functional Requirements
1. Row: `id` a fresh UUID; `aggregate_id` and `correlation_id` the userId;
   `payload` the envelope (JSONB); `trace_parent` NULL until RUST-3.2;
   `created_at` the database default.
2. Envelope: `eventId` (fresh UUID, not the row id), `eventType`
   `UserRegistered`, `timestamp` (the instant as Java prints it),
   `source` `user-service`, `correlationId` the userId, `payload`
   `{"userId", "email"}` (the stored, normalised address).
3. The payload type derives its JSON Schema (draft-07, closed); a test in
   CI compares it with the latest version of `user-events-UserRegistered`
   in gluon's `registry/user-service/registry.json` (path from
   `GLUON_REGISTRY`, default `../../registry`) and fails if they differ or
   the file is missing. Nothing is registered (contracts are frozen).

## Acceptance Criteria
- `scripts/verify-outbox.sh` PASS: CI green (including the registry check);
  the harness outbox group green except the Debezium routing check, which
  needs the `traceparent` header (RUST-3.2).
