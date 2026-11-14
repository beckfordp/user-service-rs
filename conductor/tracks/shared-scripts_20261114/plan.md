# Plan — CI and verify scripts on gluon-rs's library, Linux and macOS (gluon TD-43.2)

## Phase 1: scripts on the shared library
- [ ] Task: `ci.sh` and `verify-cargo-audit.sh` on gluon-rs's library; drop `scripts/lib/verify.sh`
    - [x] Red: on a stock-macOS PATH, verify-cargo-audit.sh says "skip
          advisory database unreachable" with the database reachable
    - [ ] Green: both on the library; audit runs on that PATH
- [ ] Task: `verify-rust-ci.sh` portable, its scratch copy laid out as gluon is
    - [x] Red: on macOS it stops at "stat: illegal option -- c", "FAIL run as ,"
    - [ ] Green: PASS on macOS, every gate case stopping at its step
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
