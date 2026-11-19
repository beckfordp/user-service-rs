# Plan — adopt service-basics' HTTP edge (gluon TD-52.3)

Spec: [spec.md](./spec.md). gluon-rs side: its track `http-edge-td52_20261119`.

## Phase 1: Adoption

- [ ] Task 1.1: the router behind `service_basics::edge`
  - [ ] Red: no `Allow` on the 405s; a refused preflight gets only `Vary`; no CORS on an
        unknown path
  - [ ] Green: `edge(routes, CorsPolicy::exposing("X-Total-Count"))`; the 404 marked
        `Unrouted`; `cors.rs` gone
- [ ] Task: Conductor - User Manual Verification 'Phase 1: Adoption' (Protocol in workflow.md)
      — `scripts/ci.sh` and gluon `bin/verify-port user-service`
