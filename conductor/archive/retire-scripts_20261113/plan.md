# Plan — retire-scripts (gluon TD-46)

## Phase 1: Acceptance and coverage map [checkpoint: 8800fc1]
- [x] Task: `scripts/verify-retire-scripts.sh`, red `390cb8e` — 26 of 31 failed: the 12 present and self-named, `contract_checks` defined, no rule
  - [x] Checks: the 12 gone; kept 5 present + `bash -n` clean; no `git grep` hits outside `conductor/archive`; `contract_checks` gone from `scripts/lib/verify.sh`; `workflow.md` states rule; `scripts/ci.sh` green
  - [x] Run: red for stated reasons (scripts present, rule absent)
- [x] Task: Coverage map in this plan — each section of the 12 → named covering test (Rust test / ci.sh step / harness check)
  - [x] Any section unmapped → Rust test written first (red on mutation, then green) — none unmapped

  Every script's "CI" section → `scripts/ci.sh`; every `contract_checks` section → that harness group (`checks/test_user_<group>.py`, run by gluon `bin/verify-port user-service`). The rest:

  | Script § | Covered by |
  |---|---|
  | openapi §2 webjar is Scala's | `test_user_docs.py` swagger-ui page = capture `http/docs-slash.txt`; `telemetry/tests/docs.rs` `the_page_is_swagger_ui_5_20_7s_with_tapirs_headers` |
  | asyncapi §2 spec byte-equal | `telemetry/tests/asyncapi.rs` `user_services_document_is_byte_identical_to_the_scala_one`; `user-service/tests/asyncapi.rs` `the_spec_binary_prints_the_scala_document` |
  | asyncapi §3 HTML byte-equal | `test_user_docs.py` `test_the_asyncapi_page_is_served`, `test_the_asyncapi_page_assets_are_the_scala_ones` (verify-port renders with `asyncapi-html.sh`) |
  | outbox §2 registry schema | ci.sh "gluon registry baseline" + `tests/events.rs` `the_payload_schema_is_the_latest_registered_version` |
  | tracing §5 console spans | `user-service/tests/tracing.rs` `without_an_endpoint_spans_go_to_stderr`; `telemetry/tests/tracing.rs` `the_console_exporter_prints_a_line_per_span_as_java_does` |
  | metrics §2 JDK UUID vectors | `telemetry/tests/jvm.rs` (ci.sh) |
  | metrics §5 scrape | `user-service/tests/metrics.rs` (content type, families); `test_user_metrics.py` `test_the_service_families_match_the_capture` |
  | logs §4 stdout JSON, messages | `user-service/tests/logs.rs` `json_lines` (panics on a non-JSON line), `each_outcome_logs_as_the_scala_service_does`; `test_user_logs.py` |
  | service-start §2 health, SIGTERM | `tests/startup.rs` `serves_health_on_the_configured_port_and_stops_on_sigterm` |
  | service-start §3 bad config | `tests/startup.rs` `bad_config_exits_1_listing_every_problem_and_binds_nothing`; `tests/config.rs` |
  | service-start §4 harness starts it | verify-port `contract` step; gluon-contract cluster-mode test |
  | readiness §3 unreachable DB | `tests/readiness.rs` `an_unreachable_database_at_startup_exits_1` |
  | yaml-content-type §2 header, HEAD | `telemetry/tests/docs.rs` `the_spec_is_served_as_yaml`, `head_is_not_allowed` |
- [x] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)

## Phase 2: Retire [checkpoint: b6b2911]
- [x] Task: Delete the 12 `scripts/verify-*.sh`; drop `contract_checks` from `scripts/lib/verify.sh` `8115ee3`
- [x] Task: Live references updated (README, conductor product/tech-stack); archives untouched `8115ee3` — README only; product, guidelines name the Scala repo's scripts (kept)
- [x] Task: Conductor - User Manual Verification 'Phase 2' (Protocol in workflow.md)

## Phase 3: Rule [checkpoint: dd6109e]
- [x] Task: `conductor/workflow.md` Phase Completion step 4 — retirement rule: at archive, script sections covered by CI/harness → script deleted; CI-runnable remainder → moved to CI; only non-CI-runnable kept `bbdb8bc`
- [x] Task: `verify-retire-scripts.sh` PASS; failure path proven on a copy — PASS 32 checks, ci.sh green (84.79% lines); copy with a missing kept tool: exit 1, tool named
- [x] Task: Conductor - User Manual Verification 'Phase 3' (Protocol in workflow.md)

At archive: `verify-retire-scripts.sh` itself retired (per the rule). gluon follow-up (verify-port lists, docs table, TD-46 tick) recorded, done in gluon root.

At archive: `verify-retire-scripts.sh` deleted (per the rule: every section covered by CI or one-off).
