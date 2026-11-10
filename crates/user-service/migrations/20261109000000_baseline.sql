-- SPDX-License-Identifier: MIT
-- Baseline (RUST-2.4, ADR 0028 decision 5): the Scala service's schema today,
-- V1__create_user_table.sql and V2__create_outbox_events.sql verbatim, as one
-- migration. Gluon has no production data, so the Rust service starts from the
-- current schema rather than its history; switching implementation starts from
-- a clean schema.

-- ===== V1__create_user_table.sql =====
-- user_account, not "user": reserved PostgreSQL keyword.
CREATE TABLE "user_account" (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Opaque auth tokens (US-16): only the SHA-256 hash is stored.
CREATE TABLE "auth_tokens" (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES "user_account"(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX auth_tokens_user_id_idx ON "auth_tokens"(user_id);

-- ===== V2__create_outbox_events.sql =====
-- Transactional outbox (ADR 0008, TD-3.2): the Kafka-bound event is written
-- here in the same transaction as the state change it announces, instead of
-- a separate direct Kafka publish call. `payload` holds the complete
-- EventEnvelope JSON (eventId/eventType/timestamp/source/correlationId/
-- payload) already assembled by the app - a future Kafka Connect + Debezium
-- connector (TD-3.1) streams new rows here straight to Kafka, unchanged.
-- No sent/processed column: cleanup is time-based (a CronJob, TD-3.1), not
-- consumption-tracked. See docs/examples.md for how this ties into
-- OutboxExampleSuite, and how to remove this table if you don't adopt the
-- outbox-CDC pattern (Flyway migrations are append-only history once
-- applied anywhere real - don't delete this file after that point).
CREATE TABLE outbox_events (
    id UUID PRIMARY KEY,
    aggregate_id TEXT NOT NULL,
    payload JSONB NOT NULL,
    trace_parent TEXT,
    correlation_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
