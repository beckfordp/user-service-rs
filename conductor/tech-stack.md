# Tech Stack — user-service-rs

Status (2026-11-08): proposed at conductor setup. Crate versions are pinned in
`Cargo.lock` by the scaffold track (Rust Spike 2). Items marked **Open** are
decided, and recorded here with a dated note, in the track that first needs
them, before any code depends on them.

## Environment (2026-11-08)
Developed and tested in the sandbox (ADR 0028): Rust via rustup, PostgreSQL 16
binaries, Node 22 (Node 24 from npm's `node-linux-x64` for the AsyncAPI HTML),
crates.io and npm reachable; no Docker, no GitHub. Every tool below runs
there, except the Dockerfile, which is written and built on the laptop.
gluon `docs/sandbox.md` records the sandbox; a tool added here is added to
gluon `bin/sandbox-setup` in the same change.

## Language / runtime
- Rust stable, edition 2024 (rustc 1.99.1 in the sandbox, 2026-11-08);
  *(2026-11-09, RUST-2.2)* pinned to **1.99.1** in `rust-toolchain.toml`
  with `rustfmt`, `clippy`, `llvm-tools-preview`, profile minimal
- Workspace-wide lints in the root `Cargo.toml` (`[workspace.lints]`, every
  member opts in with `lints.workspace = true`): `unsafe_code = "forbid"`;
  clippy `all` and `pedantic` as warnings (CI denies warnings); clippy
  `unwrap_used`, `expect_used`, `panic` denied — `rust.md` rule 2 enforced by
  the compiler; `clippy.toml` allows all three in `#[test]` functions, which
  fail by panicking, and integration test files allow them file-wide
  (`#![allow(...)]`) for their helpers *(2026-11-09)*. `missing_docs` warns too (workflow.md: every public item
  documented)
- tokio (multi-threaded runtime)
- `#![forbid(unsafe_code)]` in every crate

## Workspace layout (ADR 0028 decision 4)
- `user-service` — the service binary
- `telemetry` — tracing middleware, metrics, log format, error envelope;
  the seed of a Rust purerest equivalent, not extracted into gluon `lib/`
  unless the follow-up ADR decides to go further
- `events-probe` — the consumer probe, run only inside the contract harness
  (Rust Spike 6)

## HTTP
- axum + tower / tower-http (CORS layer with the Scala service's exact policy:
  any origin, methods GET/POST/PATCH/PUT/DELETE, allowed header
  `Content-Type`, exposed header `X-Total-Count`, no credentials)
- serde / serde_json for bodies. axum's default extractor rejections (415,
  422) differ from tapir's; rejections are mapped to what the Scala service
  answers, as recorded by the harness.

## API docs
- OpenAPI at `/docs/docs.yaml` generated from the Rust route and DTO
  definitions, Swagger UI at `/docs`. **Open:** generator crate (utoipa is the
  candidate) and how to match the Scala spec's OpenAPI version.
- AsyncAPI 3.0 at `/docs/asyncapi.json`, with the `UserRegistered` payload
  schema derived from the Rust event type (**Open:** schemars is the
  candidate; the output must equal the registry's draft-07, closed schema).
  HTML at `/docs/asyncapi/`, rendered by `@asyncapi/html-template` 3.5.6 (as
  the Scala build) through `@asyncapi/cli` 6.2.0 on Node 24. The Scala build
  pins cli 6.2.2, which the sandbox's npm registry lacks; the template, which
  produces the HTML, is the same version. Generated as a build step in the
  sandbox, and in the Dockerfile's Node stage on the laptop.

## Persistence
- sqlx (Postgres, tokio runtime), with a connection pool of 10 (as the Scala
  service)
- One baseline migration equal to the Scala service's schema today (V1 + V2),
  embedded and applied at startup, with its own history table (ADR 0028
  decision 5). **Open:** sqlx's migrator or a minimal runner; either way gluon
  `bin/reset-data` must keep the history table (PLAN.md RUST-5.3).
- One Postgres database (`user`); tables `user_account`, `auth_tokens`,
  `outbox_events`, identical to the Scala service's.

## Build profiles
- *(2026-11-09, RUST-2.5)* Dev builds optimise dependencies (`opt-level = 3`)
  and this crate lightly (`opt-level = 1`): `pbkdf2_hmac::<Sha256>` is generic,
  so its loop compiles in our crate. One 210000-round hash: 945 ms at
  opt-level 0, 48 ms at 1 (sandbox). The contract harness runs the dev build,
  so this keeps its runs fast; coverage is still measured (96% lines).

## Auth
- PBKDF2-HMAC-SHA256 from RustCrypto (`pbkdf2`, `hmac`, `sha2`), over the
  UTF-8 password bytes, with stored form `pbkdf2$<iterations>$<salt>$<hash>`
  (base64url, no padding). Verified against Java's `PBKDF2WithHmacSHA256`
  with ASCII, accented, CJK and emoji passwords (2026-11-08, ADR 0028 review).
- Random bytes from the OS CSPRNG (`rand`/`getrandom`); token storage as hex
  SHA-256.
- Parity rules from the Scala service: password length counted in UTF-16 code
  units (`encode_utf16().count()`); email trimmed of characters up to U+0020
  only, lower-cased, and matched by a pattern whose `\s` is ASCII-only.

## Observability (the `telemetry` crate)
- Tracing: `tracing` + `tracing-opentelemetry` + the `opentelemetry` crates,
  OTLP/HTTP export configured from `OTEL_EXPORTER_OTLP_ENDPOINT` and
  `OTEL_TRACES_SAMPLER_ARG` (malformed → startup fails); W3C `traceparent`
  propagation; `service.name = user-service`. The OpenTelemetry crates are
  pre-1.0: pin exact versions.
- Metrics: Prometheus exposition on the metrics port (9090) with the names,
  labels and bucket boundaries the gluon dashboards query. **Open:** the
  opentelemetry Prometheus exporter or a direct Prometheus client crate.
- Logs: JSON lines compatible with logstash-logback-encoder, `@timestamp`
  first (Filebeat only decodes lines starting `{"@timestamp"`), with
  `trace_id`/`span_id` inside a span. Built as a custom `tracing-subscriber`
  layer.

## Messaging
- None at runtime: `UserRegistered` goes through the outbox table (ADR 0008).
- `events-probe` only: rdkafka (librdkafka) for the consumer and its
  dead-letter producer, reqwest for the resilience-wrapped HTTP call.

## Config
- The Scala service's env vars: `USER_SERVICE_PORT`,
  `USER_SERVICE_METRICS_PORT`, `USER_SERVICE_NAME`, `POSTGRES_HOST`,
  `POSTGRES_PORT`, `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD`,
  `AUTH_ACCESS_TOKEN_TTL`, `AUTH_REFRESH_TOKEN_TTL`, `AUTH_PBKDF2_ITERATIONS`,
  plus the OTel vars above. Same defaults; the same startup validation
  (bindable port, iterations ≥ 1, positive TTLs). **Open:** the TTL format
  (HOCON durations such as `1h`/`30d` in Scala) is matched exactly.

## Testing
- `cargo test`; Postgres-backed tests against a throwaway PostgreSQL 16
  cluster started from the local binaries (`initdb`/`pg_ctl`) by a test
  helper. No Testcontainers: no Docker in the sandbox.
- Coverage with **cargo-llvm-cov** (version pinned in gluon
  `bin/sandbox-setup`), gate `--fail-under-lines 80` (workflow.md); it runs
  the tests once, instrumented, so CI doesn't run them twice. 0.9.1 exits 1
  below the gate without any message, so `scripts/ci.sh` prints the covered
  share and the gate itself *(2026-11-09)*
- The black-box harness in gluon `tests/contract` (isolated mode) at each
  phase checkpoint

## Lint / format / audit
- rustfmt (`cargo fmt --check`), clippy with `-D warnings`
- cargo-audit in CI (interim for ADR 0013, which is still undecided).
  *(2026-11-09, RUST-2.2)* **Not in the sandbox:** the RustSec advisory
  database it reads is on GitHub, and `github.com`,
  `raw.githubusercontent.com` and `rustsec.org` are unreachable there.
  `scripts/ci.sh` runs it when it is installed and the database reachable
  (GitHub Actions), and otherwise prints an explicit `SKIP` line saying why

## Packaging / local deploy
- *(Laptop, Rust Spike 5)* Multi-stage Dockerfile → `user-service-rs:latest`
  in the local Docker
  daemon (OrbStack), exposing 8080 and 9090
- Deployed by gluon's generic Helm chart under the release name
  `user-service`, using `environments/local/user-service-rs.values.yaml` and
  the `bin/k8s-local-up` implementation switch (PLAN.md Rust Spike 5)
- CI: defined in `scripts/ci.sh`, run directly in the sandbox; GitHub Actions
  calls the same script once the repo is pushed (laptop). The payload schema
  check reads gluon's `registry/` (sibling checkout in the sandbox, sparse
  checkout in Actions). No GitHub Packages token needed.
