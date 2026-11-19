// SPDX-License-Identifier: MIT

//! The HTTP server: routes, binding and graceful shutdown.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use axum::Router;
use axum::routing::{get, post};
use service_basics::cors::{CorsPolicy, Unrouted};
use service_basics::edge::edge;
use service_basics::health::{self, Readiness};
use service_basics::text_error::{self, TextError, method_not_allowed};
use telemetry::logging::Logger;
use telemetry::metrics::{Metrics, ServerMetrics, server_metrics};
use telemetry::tracing::{SdkTracer, Unmatched, server_tracing};
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::config::{AuthConfig, Config};
use crate::hashing::PasswordHasher;
use crate::store::UserStore;
use crate::{auth, docs, users};

/// How long readiness waits for Postgres to answer.
const READY_PING: Duration = Duration::from_secs(2);

/// What the handlers share, over a store `S` ([`crate::store::PgUserStore`]
/// in the service, an in-memory one in tests).
#[derive(Clone)]
pub struct AppState<S> {
    /// The service's tables.
    pub store: S,
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
/// metered outside tracing, behind the Scala services' edge (gluon-rs
/// `service-basics`): CORS outermost (a preflight is answered with no span and
/// no measurement; `Cors.scala` exposes `X-Total-Count`), tapir's Allow-less 405.
pub fn router<S: UserStore>(state: AppState<S>) -> Router {
    let tracer = state.tracer.clone();
    let server = ServerMetrics::new(&state.metrics);
    let routes = health::routes()
        .route("/auth/signup", post(auth::signup))
        .route("/auth/login", post(auth::login))
        .route("/users/{id}", get(users::get_user).head(method_not_allowed))
        .merge(docs::router(state.asyncapi_html_dir.clone()))
        .method_not_allowed_fallback(method_not_allowed)
        .fallback(not_found)
        .layer(axum::middleware::from_fn_with_state(tracer, server_tracing))
        .layer(axum::middleware::from_fn_with_state(server, server_metrics))
        .with_state(state);
    edge(routes, CorsPolicy::exposing("X-Total-Count"))
}

/// 404 for any unknown path (capture `http/unknown-path.txt`); marked
/// [`Unmatched`], so its span has no status code (capture stdout.log), and
/// [`Unrouted`], so it gets no CORS headers.
async fn not_found() -> (Unmatched, Unrouted, TextError) {
    (Unmatched, Unrouted, text_error::not_found().await)
}

/// Ready when Postgres answers a ping within 2 s (capture
/// `http/health-ready.txt`, openapi.yaml). The store's ping, so it is measured
/// as `db.operation=ping` and through the pool metrics.
impl<S: UserStore> Readiness for AppState<S> {
    async fn ready(&self) -> bool {
        self.store.ping(READY_PING).await
    }
}

/// Serves the API on `0.0.0.0:<port>` and the Prometheus scrape on
/// `0.0.0.0:<metrics port>` until `shutdown` completes.
///
/// # Errors
/// When a port can't be bound or serving fails.
pub async fn run<S: UserStore>(
    config: &Config,
    state: AppState<S>,
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
