# Plan — CI and verify scripts on gluon-rs's library, Linux and macOS (gluon TD-43.2)

## Phase 1: scripts on the shared library
- [x] Task: `ci.sh` and `verify-cargo-audit.sh` on gluon-rs's library; drop `scripts/lib/verify.sh` 71e5ab7
    - [x] Red: on a stock-macOS PATH, verify-cargo-audit.sh says "skip
          advisory database unreachable" with the database reachable
    - [x] Green: both on the library; audit runs on that PATH
- [x] Task: `verify-rust-ci.sh` portable, its scratch copy laid out as gluon is 5ef5d8a
    - [x] Red: on macOS it stops at "stat: illegal option -- c", "FAIL run as ,"
    - [x] Green: PASS on macOS and the Linux bed, every gate case stopping at its step
- [ ] Task: Conductor - User Manual Verification 'Phase 1' (Protocol in workflow.md)
