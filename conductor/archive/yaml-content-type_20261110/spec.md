# Spec — RUST-5.0 follow-up: docs.yaml Content-Type

## Overview
The RUST-5.0 run (2026-11-10) found that the Scala service answers GET `/docs/docs.yaml` with `Content-Type: application/yaml`, with no charset. gluon-contract `test_docs.py` now expects that (track `scala-confirmation_20261110`, `6fa3fab`). The Rust service sends `application/yaml; charset=UTF-8`, a value inferred from tapir's source and never captured, so the docs check and gluon-contract's cluster-mode CI test fail.

## Functional Requirements
1. `crates/telemetry/src/docs/swagger.rs`: `/docs/docs.yaml` sends `Content-Type: application/yaml`.
2. Tests: `crates/telemetry/tests/docs.rs::the_spec_is_served_as_yaml` and `crates/user-service/tests/docs.rs::spec` expect `application/yaml`.
3. `scripts/verify-yaml-content-type.sh` checks, each result as it's found:
   - CI;
   - the header, by curl against a started service;
   - the full harness against the Rust build, every group including probe (Part A's contract step);
   - gluon-contract's `scripts/ci.sh` green, including its cluster-mode test.

## Non-Functional
Other docs responses are unchanged, as is the HEAD → 405 response.

## Acceptance Criteria
- `scripts/ci.sh` green.
- Harness against Rust: all groups green.
- gluon-contract CI green, which unblocks task 4 of `scala-confirmation_20261110`.

## Out of Scope
The archived openapi_20261109 spec (kept as history), and the gluon PLAN, findings and CHANGELOG ticks.
