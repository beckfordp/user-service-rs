# Spec — events-probe uses gluon-rs's user-client (gluon TD-59)

## Overview
The probe's user-service lookup (`lookup.rs`, `LookupError`) moved to gluon-rs crate
`user-client` (track `user-client-td59_20261119`), so notification-service-rs's consumers
share it. The probe keeps its `Lookup` trait (its test seam), implemented for
`UserClient`. No behaviour change.

## Acceptance Criteria
- [ ] No `/users/` request code and no `LookupError` definition in events-probe.
- [ ] `scripts/ci.sh` green; gluon `bin/verify-port user-service` 128/128, probe 10/10.
