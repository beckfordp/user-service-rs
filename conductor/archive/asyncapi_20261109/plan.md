# Plan — RUST-4.2

## Phase 1: telemetry crate
- [x] Task: write failing tests (document shape, routes, static files) `3677503`
- [x] Task: implement `telemetry::docs::asyncapi` `73b24f0`

## Phase 2: the service
- [x] Task: write failing tests (config key, spec binary, end to end with a fixture HTML dir) `84ea7fc`
- [x] Task: channels, `asyncapi-spec` binary, routes, `ASYNCAPI_HTML_DIR` `4396874`
- [x] Task: `scripts/asyncapi-html.sh` `ac516ed`
- [x] Task: `scripts/verify-asyncapi.sh` — PASS; contract docs group 17/17 `8e06dac`
