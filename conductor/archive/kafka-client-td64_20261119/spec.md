# Spec — events-probe adopts kafka-client's TD-64 changes

## Overview
gluon-rs track `kafka-client-td64_20261119` (gluon TD-64.1–64.5) changed the Kafka crates'
API: `Routes` declares its own `type Error`, and `ConsumerSettings` takes `GroupId` and
`Topic`. The events-probe adopts them, with no behaviour change.

## Functional Requirements
1. `Lookup` fails with a typed `LookupError` (client, body, unexpected status); the
   route with `ProbeError` (payload, lookup). Each reads as the message it replaced.
2. `HttpLookup::new` returns reqwest's error, not `String`.
3. `PROBE_TOPIC` and `PROBE_GROUP_ID` are parsed into `Topic` and `GroupId` in
   `from_env`; an invalid one is reported with the other config problems.

## Acceptance Criteria
- [ ] `scripts/ci.sh` green; gluon `bin/verify-port user-service` (probe 10/10, contract
      128/128; cluster not run on macOS).
- [ ] No `Result<_, String>` in the probe's handler or lookup.

## Out of Scope
`from_env`'s `Vec<String>` problem list (a report, not an error path); behaviour changes.
