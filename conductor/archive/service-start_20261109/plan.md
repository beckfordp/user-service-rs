# Plan — RUST-2.3: Config contract and /health

## Phase 1: Config and startup

*Findings (2026-11-09): integration tests must pass `LLVM_PROFILE_FILE` through `env_clear()` or the spawned binary reports 0% coverage; clippy's allow-*-in-tests covers only `#[test]` functions, so integration test files allow them file-wide. Verification: the script PASS; the user's review is deferred to the end of the run.*

- [x] Task: write failing config tests (defaults, overrides, HOCON durations, every invalid value reported) fe0693b
- [x] Task: implement `config` (pure) beea489
- [x] Task: write failing startup tests (the binary serves /health on the configured port; bad config exits 1 listing problems; SIGTERM stops it) 76d7491
- [x] Task: implement `main` and the HTTP server; retire the placeholder binary test a49d39c
- [x] Task: `scripts/verify-service-start.sh` (with `scripts/lib/verify.sh`); run it — PASS, 15 checks 3c787a8
