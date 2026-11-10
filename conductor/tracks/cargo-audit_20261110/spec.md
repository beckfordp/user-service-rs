# Spec — cargo audit green

## Overview
`scripts/ci.sh`'s `cargo audit --deny warnings` fails wherever the advisory DB is reachable (laptop run 2026-11-10; GitHub Actions). The sandbox SKIPs it, so it was never seen there. Three advisories:
- RUSTSEC-2023-0071 `rsa` 0.9.10 and RUSTSEC-2026-0459 `libsqlite3-sys` 0.30.1: in `Cargo.lock` only, because `sqlx-macros-core` 0.8.6 lists `sqlx-mysql`/`sqlx-sqlite`; only the `postgres` feature is enabled and `cargo tree -i <crate> -e all --target all` finds no path, so neither is compiled.
- RUSTSEC-2024-0449 `unsafe-libyaml` (unmaintained): via deprecated `serde_yaml` 0.9, a dev-dependency used once in `crates/user-service/tests/docs.rs`.

## Functional Requirements
1. `.cargo/audit.toml`: ignore RUSTSEC-2023-0071 and RUSTSEC-2026-0459, each with a comment giving the lockfile-only reason.
2. `crates/user-service`: dev-dependency `serde_yaml` → `serde_norway` 0.9.42; `tests/docs.rs` parses `/docs/docs.yaml` with it. `unsafe-libyaml` leaves `Cargo.lock`.
3. `scripts/verify-cargo-audit.sh`, each result as found:
   - CI (`scripts/ci.sh`, unless `--skip-ci`);
   - `cargo tree -i` finds no path to `rsa` or `libsqlite3-sys` (the ignores stay justified);
   - `serde_yaml` and `unsafe-libyaml` absent from `Cargo.lock`;
   - `cargo audit --deny warnings` passes; SKIP (not counted) when the advisory DB is unreachable.

## Non-Functional
No runtime dependency changes; service behaviour unchanged.

## Acceptance Criteria
- On the laptop: `scripts/ci.sh` green with cargo audit actually running (not SKIP).
- `scripts/verify-cargo-audit.sh` PASS; failure path proven once on a broken copy.

## Out of Scope
sqlx 0.9 upgrade; gluon PLAN/CHANGELOG ticks; the gluon `bin/verify-complete-RPN-0.sh` setup fix (separate gluon commit).
