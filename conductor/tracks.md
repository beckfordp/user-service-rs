# Project Tracks

This file tracks all major tracks for the project.

---

## Tracks










---

## Backlog

Title-only placeholders for future tracks — not yet detailed (no spec/plan, no linked
folder), so `/conductor:implement` cannot pick these up by accident. Source of truth for
scope: gluon `PLAN.md` "Rust Spike" (ADR 0028). One track per PLAN item, worked in PLAN's
outside-in order (each harness group in `tests/contract` first), each ending with a review.
When ready, run `/conductor:newTrack <title>`.

- cargo audit green: audit.toml ignores for RUSTSEC-2026-0459 (libsqlite3-sys) and RUSTSEC-2023-0071 (rsa), lockfile-only via sqlx's sqlite/mysql drivers; RUSTSEC-2024-0449 unsafe-libyaml (dev-only via serde_yaml): replace or justify

