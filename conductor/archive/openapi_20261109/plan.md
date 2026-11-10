# Plan — RUST-4.1

## Phase 1: telemetry crate
- [x] Task: vendor swagger-ui 5.20.7 (`scripts/vendor-swagger-ui.sh`) `fcfb6c0`
- [x] Task: write failing tests (YAML emitter, OpenAPI builder, Swagger UI routes) `484767d`
- [x] Task: implement `telemetry::docs` `bdd3580`

## Phase 2: the service
- [x] Task: write failing tests (end to end: docs paths, spec content) `2ea8c96`
- [x] Task: describe the API (DTOs with schemars docs, operations) and serve it `79300d2`
- [x] Task: `scripts/verify-openapi.sh` — PASS; harness Swagger/OpenAPI checks 8/8, REST 42/42 `8edb2f5`
