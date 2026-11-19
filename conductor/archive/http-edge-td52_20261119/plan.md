# Plan — adopt service-basics' HTTP edge (gluon TD-52.3)

Spec: [spec.md](./spec.md). gluon-rs side: its track `http-edge-td52_20261119`.

## Phase 1: Adoption

- [x] Task 1.1: the router behind `service_basics::edge`
  - [x] Red: no `Allow` on the 405s; a refused preflight gets only `Vary`; no CORS on an
        unknown path
  - [x] Green: `edge(routes, CorsPolicy::exposing("X-Total-Count"))`; the 404 marked
        `Unrouted`; `cors.rs` gone
- [x] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`

## Archive (2026-11-19)

Done (gluon TD-52.3): the router behind `service_basics::edge::edge(routes, CorsPolicy::exposing("X-Total-Count"))`, the 404 marked `Unrouted`, `cors.rs` gone. Now as Scala: no `Allow` on 405s, a refused preflight gets only `Vary`, no CORS on the 404 (tests red before, green after). `scripts/ci.sh` 94.16%; gluon `bin/verify-port user-service` 128/128, probe 10/10. Commit `0b1bb15`.
