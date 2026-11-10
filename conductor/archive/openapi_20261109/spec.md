# Spec — RUST-4.1: OpenAPI at /docs/docs.yaml, Swagger UI at /docs

## Overview
gluon PLAN.md RUST-4.1 (ADR 0028 "Docs" row). purerest `Docs.routes` (tapir
`SwaggerInterpreter`): the OpenAPI spec generated from the same definitions as
the routes, and Swagger UI, in the `telemetry` crate (`docs` module); the
service describes its operations and DTOs. Harness: `tests/contract`
`checks/test_docs.py` (RUST-1.4), the Swagger/OpenAPI checks. Sources: capture
20261109 `openapi.yaml`, `http/docs*.txt`; tapir 1.11.x `SwaggerUI`,
`tapir-files` (content types, ETag, 404), sttp-model's tapir MIME table;
swagger-ui webjar 5.20.7 (Maven Central).

## Functional Requirements
1. OpenAPI 3.1.0 document built from operation descriptions (tags, summary,
   description, tapir's default `operationId`, path parameters, JSON request
   body with example, responses) and component schemas derived (schemars)
   from the DTO types; several error variants on one status merge as tapir
   merges them (first variant's description and example, `anyOf` of every
   variant's schema). Served as YAML at `/docs/docs.yaml`,
   `application/yaml; charset=UTF-8`; every string quoted so YAML 1.1
   readers (PyYAML, js-yaml) read the same values. Parsed equal to the
   captured `openapi.yaml`.
2. Swagger UI 5.20.7 (vendored from the webjar by
   `scripts/vendor-swagger-ui.sh`): `/docs` → 308 `./docs/` (query kept);
   `/docs/` and `/docs/index.html` the page; `/docs/swagger-initializer.js`
   rewritten as tapir does (`./docs.yaml`, `showExtensions: false`,
   `text/javascript; charset=UTF-8`); the runtime files under `/docs/` with
   tapir's content types, `ETag` (`<hex mtime ms>-<hex length>`),
   `Last-Modified`, `Accept-Ranges: bytes`, 304 on a matching
   `If-None-Match`; an unknown `/docs/…` file → 404 with no body. Not served:
   the webjar's source maps and ES bundles (no page loads them).
3. HEAD on these GET routes → 405, as tapir answers.

## Acceptance Criteria
- `scripts/verify-openapi.sh` PASS: CI green; the harness docs group's Swagger
  UI and OpenAPI checks green, including responses conforming as the Scala
  ones do; REST group still green.
