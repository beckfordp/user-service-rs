# Spec — RUST-2.6: GET /users/{id}, CORS, error and rejection responses

## Overview
gluon PLAN.md RUST-2.6. The rest of the REST contract: the whole of
`checks/test_rest.py` green against the build (RUST-1.3's exit criterion).
Source: capture 20261109 (`http/users-*.txt`, `cors-*.txt`, `unknown-path.txt`,
`*-wrong-method.txt`, `docs-yaml-headers.txt` for HEAD).

## Functional Requirements
1. `GET /users/{id}`: id parsed case-insensitively; malformed → 400
   `text/plain; charset=UTF-8` `Invalid value for: path parameter id`;
   unknown → 404 `{"error": "User not found"}`; found → 200 `{"userId",
   "email", "createdAt"}`, `createdAt` as `java.time.Instant.toString` prints
   the stored instant (UTC, seconds always, fraction in groups of three).
2. Wrong method (including HEAD on a GET route) → 405 text
   `Method Not Allowed`; unknown path → 404 text `Not found`.
3. CORS (http4s `CORS.policy` as captured): a preflight (OPTIONS with
   Access-Control-Request-Method) → 200, empty, `Access-Control-Allow-Origin: *`,
   `Access-Control-Allow-Methods: PATCH, PUT, GET, POST, DELETE`,
   `Access-Control-Allow-Headers: Content-Type`,
   `Vary: Access-Control-Request-Method, Access-Control-Request-Headers`; a
   request with `Origin` gets `Access-Control-Allow-Origin: *` and
   `Access-Control-Expose-Headers: X-Total-Count`; without `Origin`, no CORS
   headers.

## Acceptance Criteria
- `scripts/verify-users-cors.sh` PASS: CI green; the whole REST group green
  (`checks/test_rest.py`, 35 checks)
