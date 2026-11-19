# Plan — events-probe uses gluon-rs's user-client (gluon TD-59)

Spec: [spec.md](./spec.md). gluon-rs side: its track `user-client-td59_20261119`.

## Phase 1: Adoption

- [x] Task 1.1: `Lookup` for `user_client::UserClient`; `lookup.rs`'s client and
      `LookupError` gone (the probe tests, unchanged but for the import, are the check)
- [x] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`

## Archive (2026-11-20)

Done (gluon TD-59): the probe's lookup is `Lookup` for gluon-rs `user_client::UserClient`; its client, `LookupError` and reqwest dependency gone. `scripts/ci.sh` 96.74%; gluon `bin/verify-port user-service` 128/128, probe 10/10. Commit `39db09c`.
