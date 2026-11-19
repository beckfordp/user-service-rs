# Spec — adopt service-basics' HTTP edge (gluon TD-52.3)

## Overview
gluon-rs `service-basics` now has the Scala services' HTTP edge (track
`http-edge-td52_20261119`): http4s's CORS policy and tapir's Allow-less 405, behind
`edge(routes, policy)`. This service's own `cors.rs` is looser than Scala's; adopting the
shared edge brings it in line (gluon TD-52):
- a wrong method's 405 has no `Allow` (axum's was sent)
- a preflight the policy refuses gets only `Vary` (the Allow-* headers were sent)
- an unknown path's 404 gets no CORS headers (http4s's CORS never sees it)
- a routed `OPTIONS` that isn't a preflight gets http4s's `Vary`

`X-Total-Count` stays exposed (`CorsPolicy::exposing`).

## Acceptance Criteria
- [ ] Tests for the four differences, red before and green after.
- [ ] No `cors.rs`, no CORS or `Allow` code; `scripts/ci.sh` green; gluon
      `bin/verify-port user-service` 128/128 (cluster not run on macOS).

## Out of Scope
TD-52.1/52.2 (harness checks, a Scala capture of the refused preflight): after the export.
