# Spec — retire-scripts: retire track scripts covered by CI and the harness (gluon TD-46)

## Overview
Every track left a `scripts/verify-<track>.sh` behind. Twelve are now covered by CI (`scripts/ci.sh`, Rust tests) and the gluon `tests/contract` harness, and since the RPN-4.1 rename they reference old check module names (broken). Retire them and record the rule so future tracks don't accumulate scripts.

## Functional Requirements
- **FR1:** delete `scripts/verify-{auth,users-cors,openapi,asyncapi,outbox,tracing,metrics,logs,probe,service-start,readiness,yaml-content-type}.sh`. Each section is mapped in `plan.md` to its covering test; any section without cover gets a Rust test first.
- **FR2:** remove `contract_checks` from `scripts/lib/verify.sh` (no callers left).
- **FR3:** keep `verify-rust-ci.sh`, `verify-cargo-audit.sh`, `asyncapi-html.sh`, `java-uuid-vectors.sh`, `vendor-swagger-ui.sh`.
- **FR4:** amend the Phase Completion section of `conductor/workflow.md` with the retirement rule.
- **FR5:** update live references (README, conductor product/tech-stack); archives untouched.

## Non-Functional Requirements
- `scripts/ci.sh` green.

## Acceptance Criteria
`scripts/verify-retire-scripts.sh` PASS:
- the 12 gone; kept scripts present and `bash -n` clean
- no `git grep` hits for the 12 outside `conductor/archive`
- `contract_checks` gone
- `workflow.md` states the rule
- `scripts/ci.sh` green
- failure path proven on a copy

The script itself is retired at archive (per the rule). gluon-side follow-up (`bin/verify-port` lists, `docs/ports/user-service-verification.md` table, TD-46 tick) recorded.

## Out of Scope
- tests/contract scripts and checks (own track)
- TD-43.2
- full `bin/verify-port user-service` run (next sandbox session)
