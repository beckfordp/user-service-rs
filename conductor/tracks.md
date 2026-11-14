# Project Tracks

This file tracks all major tracks for the project.

---

## Tracks

- [~] **Track: CI and verify scripts on gluon-rs's library, Linux and macOS (gluon TD-43.2)**
  *Link: [./tracks/shared-scripts_20261114/](./tracks/shared-scripts_20261114/)*












---

## Backlog

Title-only placeholders for future tracks — not yet detailed (no spec/plan, no linked
folder), so `/conductor:implement` cannot pick these up by accident. Source of truth for
scope: gluon `plans/rust-port/user-service/plan.md` (ADR 0028). One track per PLAN item, worked in PLAN's
outside-in order (each harness group in `tests/contract` first), each ending with a review.
When ready, run `/conductor:newTrack <title>`.

- GitHub Actions green: `cargo test --test asyncapi` fails on the runner (main, 2026-11-10); work is on main only, red CI hides real failures


- CI checks out gluon-rs: crates depend on ../../lib/gluon-rs by path (gluon TD-43.1); .github/workflows/ci.yml must check out gluon-rs alongside (the repo itself at services/user-service-rs in the workspace). Needs gluon-rs's GitHub remote first
