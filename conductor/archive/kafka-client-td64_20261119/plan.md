# Plan — events-probe adopts kafka-client's TD-64 changes

Spec: [spec.md](./spec.md). gluon-rs side: its track `kafka-client-td64_20261119`.

## Phase 1: Adoption

- [x] Task 1.1: `LookupError` and `ProbeError`; tests compare variants and text `a9edf18`
- [x] Task 1.2: `Topic`/`GroupId` parsed in `from_env`; a bad one is a reported problem `a9edf18`
- [x] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`

## Archive (2026-11-19)

Done (gluon TD-64 adoption): `LookupError`/`ProbeError` for the lookup and route,
`HttpLookup::new` returns reqwest's error, `PROBE_TOPIC`/`PROBE_GROUP_ID` parsed in
`from_env`. No behaviour change. Regression: `scripts/ci.sh` 94.38%; gluon `bin/verify-port
user-service` PASS (contract 128/128, probe 10/10; tracks 2/2; cluster skipped on macOS).
Commit `a9edf18`.
