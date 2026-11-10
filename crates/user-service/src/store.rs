// SPDX-License-Identifier: MIT

//! The service's tables (`user_account`, `auth_tokens`, `outbox_events`), as
//! the Scala service writes them (its `UserStore`), instrumented as it is:
//! every call is timed as a `db.client.operation.duration` (`ping`, `insert`,
//! `select`) around acquiring a connection from the measured pool and the
//! work; registrations and logins are counted (`UserJourneyMetrics`). Writes
//! that belong together share one transaction.

use std::future::Future;
use std::time::{Duration, Instant};

use opentelemetry::KeyValue;
use opentelemetry::context::FutureExt;
use opentelemetry::trace::TraceContextExt;
use sqlx::pool::PoolConnection;
use sqlx::{Connection, PgConnection, PgPool, Postgres};
use telemetry::metrics::{Counter, Held, Histogram, LATENCY_BUCKETS, Metrics, PoolMetrics};
use telemetry::tracing::{SdkTracer, internal_span, traceparent};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::domain::Email;
use crate::events::OutboxEvent;
use crate::tokens::IssuedToken;

/// What `create_account` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateOutcome {
    /// The account and its first token are stored.
    Created {
        /// `user_account.created_at`, as the database set it.
        created_at: OffsetDateTime,
    },
    /// The address was taken; nothing was written.
    Duplicate,
}

/// An account as login needs it.
#[derive(Debug, Clone)]
pub struct StoredAccount {
    /// `user_account.id`.
    pub id: Uuid,
    /// `user_account.password_hash`.
    pub password_hash: String,
}

/// An account as `GET /users/{id}` shows it.
#[derive(Debug, Clone)]
pub struct PublicAccount {
    /// `user_account.id`.
    pub id: Uuid,
    /// `user_account.email`.
    pub email: String,
    /// `user_account.created_at`.
    pub created_at: OffsetDateTime,
}

/// Why a store call failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The database refused or failed.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    /// No answer in time.
    #[error("no answer within {0:?}")]
    Timeout(Duration),
}

impl StoreError {
    /// The `error.type` attribute: the error's type, as the Scala service
    /// records the exception class (which a Rust service can't reproduce).
    fn kind(&self) -> &'static str {
        match self {
            Self::Timeout(_) => "Timeout",
            Self::Database(error) => match error {
                sqlx::Error::Database(_) => "sqlx::Error::Database",
                sqlx::Error::Io(_) => "sqlx::Error::Io",
                sqlx::Error::PoolTimedOut => "sqlx::Error::PoolTimedOut",
                sqlx::Error::PoolClosed => "sqlx::Error::PoolClosed",
                sqlx::Error::Protocol(_) => "sqlx::Error::Protocol",
                _ => "sqlx::Error",
            },
        }
    }
}

/// The instrumented store.
#[derive(Debug, Clone)]
pub struct Store {
    pool: PgPool,
    pool_metrics: PoolMetrics,
    operations: Histogram,
    registrations: Counter,
    logins: Counter,
    tracer: SdkTracer,
}

type Conn = Held<PoolConnection<Postgres>>;

impl Store {
    /// Wraps `pool` (named `user` in the metrics, as the Scala pool is).
    #[must_use]
    pub fn new(pool: PgPool, metrics: &Metrics, tracer: SdkTracer) -> Self {
        Self {
            pool,
            pool_metrics: PoolMetrics::new(metrics, "user"),
            operations: metrics.histogram("db.client.operation.duration", "s", &LATENCY_BUCKETS),
            registrations: metrics.counter("users.registrations"),
            logins: metrics.counter("users.logins"),
            tracer,
        }
    }

    async fn connection(&self) -> Result<Conn, StoreError> {
        Ok(self.pool_metrics.acquire(self.pool.acquire()).await?)
    }

    async fn timed<T>(
        &self,
        operation: &str,
        work: impl Future<Output = Result<T, StoreError>>,
    ) -> Result<T, StoreError> {
        let start = Instant::now();
        let result = work.await;
        let elapsed = start.elapsed().as_secs_f64();
        let mut attributes = vec![("db.system", "postgresql"), ("db.operation", operation)];
        if let Err(error) = &result {
            attributes.push(("error.type", error.kind()));
        }
        self.operations.record(elapsed, &attributes);
        result
    }

    /// Whether Postgres answers `SELECT 1` within `within`.
    pub async fn ping(&self, within: Duration) -> bool {
        self.timed("ping", async {
            let work = async {
                let mut conn = self.connection().await?;
                sqlx::query("SELECT 1").execute(&mut **conn).await?;
                Ok(())
            };
            tokio::time::timeout(within, work)
                .await
                .unwrap_or(Err(StoreError::Timeout(within)))
        })
        .await
        .is_ok()
    }

    /// Inserts the account, its first access token and its `UserRegistered`
    /// outbox row in one transaction (ADR 0008); a duplicate address writes
    /// nothing. The row is written inside an `outbox.write` span whose context
    /// it records in `trace_parent`. Counts the registration by outcome.
    ///
    /// # Errors
    /// Database failures.
    pub async fn create_account(
        &self,
        id: Uuid,
        email: &Email,
        password_hash: &str,
        access_token: &IssuedToken,
        event: &OutboxEvent,
    ) -> Result<CreateOutcome, StoreError> {
        let outcome = self
            .timed("insert", async {
                let mut conn = self.connection().await?;
                let mut tx = (**conn).begin().await?;
                let created_at: Option<OffsetDateTime> = sqlx::query_scalar(
                    r#"INSERT INTO "user_account" (id, email, password_hash) VALUES ($1, $2, $3)
                       ON CONFLICT (email) DO NOTHING RETURNING created_at"#,
                )
                .bind(id)
                .bind(email.as_str())
                .bind(password_hash)
                .fetch_optional(&mut *tx)
                .await?;
                let Some(created_at) = created_at else {
                    tx.rollback().await?;
                    return Ok(CreateOutcome::Duplicate);
                };
                insert_token(&mut tx, id, access_token).await?;
                self.write_outbox(&mut tx, event).await?;
                tx.commit().await?;
                Ok(CreateOutcome::Created { created_at })
            })
            .await?;
        let label = match outcome {
            CreateOutcome::Created { .. } => "created",
            CreateOutcome::Duplicate => "duplicate",
        };
        self.registrations.add(1, &[("outcome", label)]);
        Ok(outcome)
    }

    /// The account with this (normalised) address, if any.
    ///
    /// # Errors
    /// Database failures.
    pub async fn find_by_email(&self, email: &Email) -> Result<Option<StoredAccount>, StoreError> {
        self.timed("select", async {
            let mut conn = self.connection().await?;
            let row: Option<(Uuid, String)> =
                sqlx::query_as(r#"SELECT id, password_hash FROM "user_account" WHERE email = $1"#)
                    .bind(email.as_str())
                    .fetch_optional(&mut **conn)
                    .await?;
            Ok(row.map(|(id, password_hash)| StoredAccount { id, password_hash }))
        })
        .await
    }

    /// The account with this id, without its password hash.
    ///
    /// # Errors
    /// Database failures.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<PublicAccount>, StoreError> {
        self.timed("select", async {
            let mut conn = self.connection().await?;
            let row: Option<(Uuid, String, OffsetDateTime)> =
                sqlx::query_as(r#"SELECT id, email, created_at FROM "user_account" WHERE id = $1"#)
                    .bind(id)
                    .fetch_optional(&mut **conn)
                    .await?;
            Ok(row.map(|(id, email, created_at)| PublicAccount {
                id,
                email,
                created_at,
            }))
        })
        .await
    }

    /// Stores a login's access and refresh tokens in one transaction, and
    /// counts the login (only successful logins get this far).
    ///
    /// # Errors
    /// Database failures.
    pub async fn record_login_tokens(
        &self,
        user_id: Uuid,
        access: &IssuedToken,
        refresh: &IssuedToken,
    ) -> Result<(), StoreError> {
        self.timed("insert", async {
            let mut conn = self.connection().await?;
            let mut tx = (**conn).begin().await?;
            insert_token(&mut tx, user_id, access).await?;
            insert_token(&mut tx, user_id, refresh).await?;
            tx.commit().await?;
            Ok(())
        })
        .await?;
        self.logins.add(1, &[]);
        Ok(())
    }

    /// The insert inside an `outbox.write` span (user-service
    /// `UserStore.insertOutbox`): INTERNAL, a child of the request span, naming
    /// the event type and aggregate.
    async fn write_outbox(
        &self,
        tx: &mut PgConnection,
        event: &OutboxEvent,
    ) -> Result<(), sqlx::Error> {
        let cx = internal_span(
            &self.tracer,
            "outbox.write",
            vec![
                KeyValue::new("outbox.event_type", event.event_type),
                KeyValue::new("outbox.aggregate_id", event.aggregate_id.clone()),
            ],
        );
        let trace_parent = traceparent(cx.span().span_context());
        let inserted = insert_event(tx, event, &trace_parent)
            .with_context(cx.clone())
            .await;
        cx.span().end();
        inserted
    }
}

async fn insert_token(
    tx: &mut PgConnection,
    user_id: Uuid,
    token: &IssuedToken,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO "auth_tokens" (id, user_id, token_hash, kind, expires_at)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&token.hash)
    .bind(token.kind.as_str())
    .bind(token.expires_at)
    .execute(tx)
    .await?;
    Ok(())
}

async fn insert_event(
    tx: &mut PgConnection,
    event: &OutboxEvent,
    trace_parent: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO outbox_events (id, aggregate_id, payload, trace_parent, correlation_id)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(event.id)
    .bind(&event.aggregate_id)
    .bind(&event.payload)
    .bind(trace_parent)
    .bind(&event.correlation_id)
    .execute(tx)
    .await?;
    Ok(())
}
