# Plan — events-probe uses gluon-rs's user-client (gluon TD-59)

Spec: [spec.md](./spec.md). gluon-rs side: its track `user-client-td59_20261119`.

## Phase 1: Adoption

- [ ] Task 1.1: `Lookup` for `user_client::UserClient`; `lookup.rs`'s client and
      `LookupError` gone (the probe tests, unchanged but for the import, are the check)
- [ ] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`
