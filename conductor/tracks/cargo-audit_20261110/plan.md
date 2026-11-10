# Plan — cargo audit green

## Phase 1: Audit green
- [ ] Task: `scripts/verify-cargo-audit.sh`, written first
  - [ ] Red: CI; `cargo tree -i` no path to `rsa`/`libsqlite3-sys`; `serde_yaml`/`unsafe-libyaml` absent from `Cargo.lock`; `cargo audit --deny warnings` (SKIP when DB unreachable). Run: lockfile + audit checks FAIL for the stated reasons
- [ ] Task: `serde_yaml` → `serde_norway`
  - [ ] Swap the dev-dependency in `crates/user-service/Cargo.toml`, the `use` in `tests/docs.rs`; `cargo test -p user-service --test docs` green; lockfile check turns green (and confirm `serde_norway`'s own `unsafe-libyaml-norway` isn't flagged)
- [ ] Task: `.cargo/audit.toml` ignores
  - [ ] RUSTSEC-2023-0071, RUSTSEC-2026-0459, each with the lockfile-only reason; `cargo audit --deny warnings` green
- [ ] Task: `scripts/ci.sh` green on the laptop with cargo audit running (not SKIP); verify script `--skip-ci` PASS; failure path proven once on a broken copy
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
