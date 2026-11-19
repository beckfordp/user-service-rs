# Project Tracks

This file tracks all major tracks for the project.

---

## Tracks

- [~] **Track: events-probe uses gluon-rs's user-client (gluon TD-59)**
  *Link: [./tracks/user-client-td59_20261119/](./tracks/user-client-td59_20261119/)*

---

## Backlog

Title-only placeholders for future tracks — not yet detailed (no spec/plan, no linked
folder), so `/conductor:implement` cannot pick these up by accident. Source of truth for
scope: gluon `plans/rust-port/user-service/plan.md` (ADR 0028). One track per PLAN item, worked in PLAN's
outside-in order (each harness group in `tests/contract` first), each ending with a review.
When ready, run `/conductor:newTrack <title>`.

- The trait store: store behind a `UserStore` trait, `PgUserStore` holding the `timed` calls and pool metrics, `AppState<S>` generic over it, route tests in-process against an in-memory store (`rust.md` rules 5 and 8; gluon `plans/private/ports/notification-service-design-differences.md` (private) "Decided" item 1, RPN-5 retro). Done when: no handler names `PgUserStore`; route tests run without Postgres; binary tests green; harness 128/128


- GitHub Actions green: `cargo test --test asyncapi` fails on the runner (main, 2026-11-10); work is on main only, red CI hides real failures


- CI checks out gluon-rs: crates depend on ../../lib/gluon-rs by path (gluon TD-43.1); .github/workflows/ci.yml must check out gluon-rs alongside (the repo itself at services/user-service-rs in the workspace). Needs gluon-rs's GitHub remote first
