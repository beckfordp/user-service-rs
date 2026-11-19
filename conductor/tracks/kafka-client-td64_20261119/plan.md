# Plan — events-probe adopts kafka-client's TD-64 changes

Spec: [spec.md](./spec.md). gluon-rs side: its track `kafka-client-td64_20261119`.

## Phase 1: Adoption

- [ ] Task 1.1: `LookupError` and `ProbeError`; tests compare variants and text
- [ ] Task 1.2: `Topic`/`GroupId` parsed in `from_env`; a bad one is a reported problem
- [ ] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`
