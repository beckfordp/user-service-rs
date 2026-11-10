// SPDX-License-Identifier: MIT

//! The HTTP server: routes, binding and graceful shutdown.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use telemetry::logging::Logger;
use telemetry::metrics::{Metrics, ServerMetrics, server_metrics};
use telemetry::tracing::{SdkTracer, Unmatched, server_tracing};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::{AuthConfig, Config};
use crate::hashing::PasswordHasher;
use crate::responses::TextError;
use crate::store::Store;
use crate::{auth, cors, docs, users};

/// How long readiness waits for Postgres to answer.
const READY_PING: Duration = Duration::from_secs(2);

/// What the handlers share.
#[derive(Clone)]
pub struct AppState {
    /// The instrumented database store.
    pub store: Store,
    /// The registry the scrape endpoint renders.
    pub metrics: Metrics,
    /// Hashes and verifies passwords.
    pub hasher: PasswordHasher,
    /// Token lifetimes.
    pub auth: AuthConfig,
    /// Starts spans (server spans, `outbox.write`).
    pub tracer: SdkTracer,
    /// Writes the JSON log lines.
    pub logger: Logger,
    /// Where the rendered AsyncAPI HTML is served from.
    pub asyncapi_html_dir: std::path::PathBuf,
}

/// The service's routes, as user-service `AppRoutes` layers them: traced,
/// metered outside tracing, CORS outermost (a preflight is answered with no
/// span and no measurement).
pub fn router(state: AppState) -> Router {
    let tracer = state.tracer.clone();
    let server = ServerMetrics::new(&state.metrics);
    Router::new()
        .route("/health", get(health).head(method_not_allowed))
        .route("/health/ready", get(ready).head(method_not_allowed))
        .route("/auth/signup", post(auth::signup))
        .route("/auth/login", post(auth::login))
        .route("/users/{id}", get(users::get_user).head(method_not_allowed))
        .merge(docs::router(state.asyncapi_html_dir.clone()))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .layer(axum::middleware::from_fn_with_state(tracer, server_tracing))
        .layer(axum::middleware::from_fn_with_state(server, server_metrics))
        .layer(axum::middleware::from_fn(cors::cors))
        .with_state(state)
}

/// 405, as tapir answers a known path with another method (capture
/// `http/*-wrong-method.txt`). HEAD included: tapir doesn't answer HEAD for a
/// GET endpoint (capture `http/docs-yaml-headers.txt`).
async fn method_not_allowed() -> TextError {
    TextError {
        status: StatusCode::METHOD_NOT_ALLOWED,
        message: "Method Not Allowed".to_owned(),
    }
}

/// 404 for any unknown path (capture `http/unknown-path.txt`); marked
/// [`Unmatched`], so its span has no status code (capture stdout.log).
async fn not_found() -> (Unmatched, TextError) {
    (
        Unmatched,
        TextError {
            status: StatusCode::NOT_FOUND,
            message: "Not found".to_owned(),
        },
    )
}

/// `GET /health`: 200 with an empty body while the process is up; checks no
/// dependencies (capture `http/health.txt`: no Content-Type).
async fn health() -> StatusCode {
    StatusCode::OK
}

/// `GET /health/ready`: 200 when Postgres answers a ping, 503 when it doesn't
/// (capture `http/health-ready.txt`, openapi.yaml); empty body either way.
async fn ready(State(state): State<AppState>) -> StatusCode {
    if state.store.ping(READY_PING).await {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

/// Serves the API on `0.0.0.0:<port>` and the Prometheus scrape on
/// `0.0.0.0:<metrics port>` until `shutdown` completes.
///
/// # Errors
/// When a port can't be bound or serving fails.
pub async fn run(
    config: &Config,
    state: AppState,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> io::Result<()> {
    let metrics_listener = TcpListener::bind(SocketAddr::from((
        Ipv4Addr::UNSPECIFIED,
        config.metrics_port,
    )))
    .await?;
    let listener =
        TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, config.port))).await?;
    let (stop, stopped) = watch::channel(false);
    let until_stopped = move || {
        let mut stopped = stopped.clone();
        async move {
            let _ = stopped.wait_for(|stop| *stop).await;
        }
    };
    let metrics = axum::serve(
        metrics_listener,
        telemetry::metrics::router(state.metrics.clone()),
    )
    .with_graceful_shutdown(until_stopped());
    let api = axum::serve(listener, router(state)).with_graceful_shutdown(until_stopped());
    tokio::spawn(async move {
        shutdown.await;
        let _ = stop.send(true);
    });
    let (api, metrics) = tokio::join!(api.into_future(), metrics.into_future());
    api.and(metrics)
}
