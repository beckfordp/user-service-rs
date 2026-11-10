# Spec — RUST-4.2: AsyncAPI JSON and HTML

## Overview
gluon PLAN.md RUST-4.2 (ADR 0028 "Docs" row, ADR 0010). purekafka
`AsyncApiDoc`/`Docs.routes`, sbt-purekafka-docs' `AsyncApiHtmlGenerator`, and
user-service's `AsyncApiChannels` and static HTML route, in the `telemetry`
crate (`docs::asyncapi`) and the service. Harness: `tests/contract`
`checks/test_docs.py` AsyncAPI checks (RUST-1.4). Sources: capture 20261109
`asyncapi.json`, `asyncapi-html/`, `http/asyncapi*.txt`; purekafka sources;
user-service `AppRoutes`, `application.conf` (`ASYNCAPI_HTML_DIR`).

## Functional Requirements
1. AsyncAPI 3.0.0 document from channel descriptions (topic, event type,
   direction, payload type): one channel per topic holding its messages
   (`name` = `title` = event type, tapir-style payload schema: draft 2020-12
   `$schema`, no `additionalProperties`, the CDC description for outbox
   events), `send` operations before `receive`. Byte-identical (compact JSON)
   to the captured `asyncapi.json` for user-service.
2. `GET /docs/asyncapi.json` → `application/json`; `GET /docs/asyncapi` → 301
   `/docs/asyncapi/`; `/docs/asyncapi/` and files under it from
   `ASYNCAPI_HTML_DIR` (default `target/asyncapi-docs`, relative to the working
   directory, as the Scala config): index for the directory, ETag
   `<hex mtime ms>-<hex length>`, Last-Modified; missing or escaping paths →
   empty 404 (the Scala request falls through to Swagger UI's resources).
3. `scripts/asyncapi-html.sh`: the spec from the `asyncapi-spec` binary,
   rendered by `@asyncapi/cli` 6.2.0 + `@asyncapi/html-template` 3.5.6 on Node
   ≥ 24.11 into `target/asyncapi-docs` (the Scala plugin's steps; cli 6.2.2 isn't
   on the sandbox's npm). Byte-identical to the captured HTML.

## Acceptance Criteria
- `scripts/verify-asyncapi.sh` PASS: CI green; HTML generated; the harness
  docs group entirely green.
