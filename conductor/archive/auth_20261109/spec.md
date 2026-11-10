# Spec — RUST-2.5: Signup and login

## Overview
gluon PLAN.md RUST-2.5. `POST /auth/signup` and `POST /auth/login` with the
Scala service's behaviour (accounts and tokens; the outbox row is RUST-3.1).
Judged by the contract harness: the signup/login checks in
`checks/test_rest.py` and all of `checks/test_schema.py`.

## Functional Requirements
1. **Domain (pure):** `Email` (trim characters up to U+0020 only, Unicode
   lower-case, `^[^\s@]+@[^\s@]+\.[^\s@]+$` with ASCII-only `\s`);
   `Password` (signup: at least 8 UTF-16 code units; login: non-empty;
   redacted `Debug`); email parsed before password; error messages exactly
   the capture's.
2. **Hashing:** PBKDF2-HMAC-SHA256, 16-byte salt, 32-byte key, stored
   `pbkdf2$<iterations>$<salt>$<hash>` (base64url, no padding); verify
   compares in constant time; an unknown email or a malformed stored hash
   still runs one full derivation. Hashing runs on `spawn_blocking`.
3. **Tokens:** 32 bytes from the OS RNG, base64url without padding; stored as
   lower-case hex SHA-256 with kind and expiry (TTLs from config).
4. **Signup:** body read whatever the Content-Type; undecodable →
   400 `text/plain; charset=UTF-8` `Invalid value for: body (<detail>)`;
   invalid credentials → 400 `{"error": …}`; account + access token in one
   transaction; duplicate → 409; else 201 `{"userId", "token"}`.
5. **Login:** same body handling; invalid → 400; unknown email, wrong
   password or malformed stored hash → 401 `{"error": "Email or password is
   incorrect"}`; else access + refresh token stored in one transaction and
   200 `{"userId", "token", "refreshToken"}`.

## Deviation
Handlers are tested end to end (the binary against a throwaway Postgres, plus
the contract harness) rather than against in-memory fakes; the store stays
concrete. The pure domain, hashing and tokens are unit-tested directly.

## Acceptance Criteria
- `scripts/verify-auth.sh` PASS: CI green; the contract harness's signup,
  login and schema checks green against the build
