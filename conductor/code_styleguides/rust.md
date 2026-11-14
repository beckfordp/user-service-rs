# Rust Style Guide

The Rust counterpart of the Scala services' `scala-fp.md`. The rules are the
same ideas — a pure core, typed errors, parsed inputs, explicit effects —
expressed in Rust's terms. Where a rule has no Rust equivalent it is dropped,
not imitated.

## 1. Functional core, effectful shell
- Business rules (validation, normalisation, hashing parameters, decisions)
  are **plain functions** over values: no `async`, no I/O, no logging, no
  store. They take what they need (including an already-obtained `now` or id)
  and return values (`Result<T, DomainError>`, a new value, an event to write).
- Effectful code (handlers, stores, clients, the probe's consumer) only
  obtains inputs, calls the core, then executes its result (persist, log,
  respond). It does not decide anything a pure function could.
- A rule is defined **once**. If it would appear in Rust and in SQL, compute
  its parameters in the core and pass them to the query.
- Keep the pure core in its own module (`domain`) with no `async fn` and no
  I/O imports, so purity is checkable by location.

## 2. Errors are values
- Expected, nameable failures are variants of an `enum` (with `thiserror` for
  `Display`), carried in `Result` from detection to the edge.
- No `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!` or indexing that can
  panic on the request path or in the consumer loop. They are allowed in
  tests, and in `main` for startup failures that should stop the process.
- Don't collapse distinct outcomes into `Option`/`bool` when the caller needs
  to know why; return `Result<T, DomainError>`.
- Map each error variant to an HTTP status and body explicitly at the edge
  (`IntoResponse` per error type, one arm per variant). No catch-all
  translator; `anyhow` only in `main` and test helpers.

## 3. Parse, don't validate
- Raw input (path ids, JSON fields, env vars) is parsed **once at the
  boundary** into domain types: newtypes with private fields and a smart
  constructor returning `Result` (`AccountId(Uuid)`, `Email`, `Password`).
- Inner layers accept the parsed type and never re-parse.
- `Password` implements `Debug` as redacted, so it can't leak into a log or
  error message.

## 4. Totality
- `match` on the crate's own enums is exhaustive with **no** `_` arm, so a new
  variant breaks compilation. `_` is acceptable only over an open set (raw
  strings, unknown event types), and then it logs or routes the value rather
  than silently swallowing it.
- Prefer `get`, `first`, `checked_*` and iterator adapters over indexing and
  unchecked arithmetic.

## 5. Effects and state are explicit
- Time, randomness, ids and I/O enter through small traits (`Clock`,
  `IdGen`, `UserStore`) or values passed in, so the core and handlers are
  testable with fakes. Never read the clock or RNG inside the pure core.
- Dependencies are explicit constructor parameters wired in `main` (an axum
  `State` holding the wired dependencies). No global mutable state, no
  `static` singletons for dependencies.
- Shared mutable state lives in the database or behind a clearly owned
  `Arc<Mutex<_>>`/channel, never a `static mut`.
- Blocking work (PBKDF2) runs on `spawn_blocking`, never directly on the async
  runtime.

## 6. Narrowest capability
- Functions take the narrowest thing they need: a trait with the two methods
  they call, a `&str` rather than a `String`, a value rather than the whole
  `State`.
- Prefer generics or `&dyn Trait` over passing a concrete pool or client into
  logic that only needs one operation.

## 7. Derive and attribute macros
- `serde`, `thiserror` and the docs generators' derives/attributes are
  allowed: they describe encoding or documentation, like tapir's
  `@description` in Scala.
- No macro-based dependency injection or hidden registration; wiring stays
  visible in `main`.

## 8. Tests
- The pure core is tested with plain values; rules with a wide input space
  (email normalisation, password length, hash format) get property tests
  (`proptest`) and known-answer vectors from the Scala service.
- Handlers are tested against in-memory trait implementations; integration
  tests use real dependencies (a throwaway Postgres from the local binaries;
  no Docker).
- Wire parity is proven by the gluon `tests/contract` harness, not by
  re-implementing its checks here. Exception *(2026-11-14, gluon TD-46)*: what
  the harness doesn't check byte for byte may be compared with capture files
  copied unchanged into `tests/fixtures` (the AsyncAPI document,
  `telemetry/tests/asyncapi.rs`); once the harness checks it, the copy and
  its test go.

## Review checklist
Reviewers check each change against these; any "no" is a **High** finding.
- [ ] New business rules are plain functions in `domain`, defined once?
- [ ] Failures are enum variants carried in `Result` to the edge, each mapped
      explicitly?
- [ ] Raw input parsed to domain types at the boundary, not re-parsed inside?
- [ ] No `unwrap`/`expect`/`panic!`/panicking index on the request or consume
      path; no `_` arm over the crate's own enums?
- [ ] Time, randomness, ids and I/O only through injected traits or values?
- [ ] Blocking work off the async runtime?
- [ ] No `unsafe`, no global mutable state, wiring explicit in `main`?
- [ ] Behaviour matches the Scala service, with a harness check for anything
      new?
