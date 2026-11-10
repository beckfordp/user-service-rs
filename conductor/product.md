# Initial Concept

A wire-compatible Rust port of user-service, built as a time-boxed spike per
gluon ADR 0028 and PLAN.md "Rust Spike". It is a drop-in replacement: same
contracts, same Helm release, chosen in gluon-local by an implementation
switch.

# Product Guide — user-service-rs

## Context
Full cross-repo vision lives in the `gluon` platform repo, not here:
[`../../../docs/adr/0028-rust-port-spike-user-service.md`](../../../docs/adr/0028-rust-port-spike-user-service.md)
(the decision this repo exists for),
[`../../../docs/system-design.md`](../../../docs/system-design.md),
[`../../../docs/user-stories.md`](../../../docs/user-stories.md),
[`../../../PLAN.md`](../../../PLAN.md) ("Rust Spike").

The reference is the specified contracts (ADR 0028 decision 2): the
documented contracts, the Scala-generated OpenAPI/AsyncAPI artifacts, the
Scala service's test suites and `verify-*.sh` scripts in
[`../../user-service`](../../user-service), and behaviour captured from it on
the laptop on request. Where this service differs, this repo changes, never
the contract.

## Purpose
Measure what it costs to rebuild purerest's and purekafka's cross-cutting
behaviour (tracing, metrics, logs, docs, resilience, consumer supervision) in
another language, by porting the smallest service and checking every wire
contract with the black-box harness in `tests/contract`. The findings feed a
follow-up ADR: stop, keep as a showcase service, or plan further ports.

## Users
- The consultant (solo): builds the spike and runs it in gluon-local in place
  of the Scala service.
- Every existing user-service client, unchanged: gshop (signup, login, the
  Admin console's `GET /users/{id}`), notification-service (recipient lookup,
  `UserRegistered` welcome email), `bin/seed-demo-data`, the acceptance suite.

## What this service does
Same as the Scala user-service (US-16.1–16.3, ADM-3):
- `POST /auth/signup` — 201 `{userId, token}`; 400 invalid email or password
  under 8 characters; 409 duplicate email. Writes the account, its access
  token and the `UserRegistered` outbox row in one transaction.
- `POST /auth/login` — 200 `{userId, token, refreshToken}`; 400 invalid email
  or empty password; 401 with one body for an unknown email or a wrong
  password.
- `GET /users/{id}` — 200 `{userId, email, createdAt}`; 400 malformed id; 404
  unknown id.
- `/health`, `/health/ready` (503 when Postgres doesn't answer).
- The same docs: `/docs` (Swagger UI), `/docs/docs.yaml`,
  `/docs/asyncapi.json`, `/docs/asyncapi/`.

## Domain model
- **User account** — `id`, unique normalised `email`, password hash, `createdAt`.
- **Auth token** — opaque, 32 random bytes base64url; stored only as hex
  SHA-256 in `auth_tokens`; access TTL 1 hour, refresh TTL 30 days.
- **Password hash** — `pbkdf2$<iterations>$<salt>$<hash>`, PBKDF2-HMAC-SHA256,
  210000 iterations by default, 16-byte salt, 256-bit key, base64url without
  padding, over the UTF-8 password.

## Events
- Publishes: `user-events` (`UserRegistered`), in the shared envelope
  (ADR 0009), through the transactional outbox and Debezium (ADR 0008).
  Payload schema: gluon `registry/user-service/registry.json` (checked, never
  registered from here).
- Consumes: none. The consumer probe (a separate binary in this workspace)
  consumes `user-events` only inside the contract harness, never in
  gluon-local.

## Sequencing (gluon `PLAN.md` "Rust Spike")
Items owned by this repo: RUST-2.2–2.6 (scaffold, REST, Postgres, hashing),
3.1–3.4 (outbox and the telemetry crate), 4.1–4.2 (docs parity) and 6.2–6.4
(consumer probe), plus the Dockerfile and fixes in the laptop pass
(RUST-5.1). Worked one item at a time, outside-in: the harness group for a
contract is written in `tests/contract` and seen failing just before the
items here that turn it green (RUST-2.2/2.3 go with the runner, RUST-1.2,
which needs a process to start). Developed and tested in the sandbox; the
laptop pass (Rust Spike 5) deploys it to gluon-local. Time box: about 10
working days for the whole spike; each item ends with a review.

## Out of scope
- Any change to a contract, or to another service.
- The performance comparison (a follow-up, ADR 0028 "Out of scope").
- Features the Scala service doesn't have (refresh endpoint, profiles,
  password reset, roles).
- Migrating data between implementations: switching starts from a clean
  schema and reseeds.
