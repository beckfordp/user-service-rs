// SPDX-License-Identifier: MIT

//! The `user-service` binary: reads the config contract, starts tracing, then
//! serves until SIGTERM or SIGINT and flushes spans on the way out.

use std::process::ExitCode;

use service_basics::{environment, runtime, shutdown};
use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use telemetry::tracing::{SdkTracer, Tracing};
use user_service::config::{self, Config};
use user_service::hashing::PasswordHasher;
use user_service::store::Store;
use user_service::{db, server};

fn main() -> ExitCode {
    let config = match config::from_env(&environment::variables()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("user-service: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Built outside the async runtime: the OTLP exporter's blocking HTTP
    // client must not be created (or dropped) on a runtime thread.
    let tracing = match Tracing::new(
        &config.service_name,
        config.tracing.otlp_endpoint.as_deref(),
        config.tracing.sampling_ratio,
    ) {
        Ok(tracing) => tracing,
        Err(error) => {
            eprintln!("user-service: {error}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match runtime::multi_thread() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("user-service: can't start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let code = runtime.block_on(serve(&config, tracing.tracer()));
    drop(runtime);
    tracing.shutdown();
    code
}

async fn serve(config: &Config, tracer: SdkTracer) -> ExitCode {
    let pool = match db::connect(&config.postgres).await {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "user-service: database {}:{}/{} unreachable: {error}",
                config.postgres.host, config.postgres.port, config.postgres.database
            );
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = db::migrate(&pool).await {
        eprintln!("user-service: database migration failed: {error}");
        return ExitCode::FAILURE;
    }
    let logger = Logger::stdout(&config.service_name);
    let (port, metrics_port) = (config.port.to_string(), config.metrics_port.to_string());
    logger.info(
        &[("port", &port), ("metrics_port", &metrics_port)],
        "user-service starting",
    );
    let metrics = Metrics::new(&config.service_name);
    let state = server::AppState {
        store: Store::new(pool, &metrics, tracer.clone()),
        metrics,
        hasher: PasswordHasher::new(config.auth.pbkdf2_iterations),
        auth: config.auth.clone(),
        tracer,
        logger,
        asyncapi_html_dir: config.asyncapi_html_dir.clone(),
    };
    match server::run(config, state, shutdown::signal("user-service")).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("user-service: {error}");
            ExitCode::FAILURE
        }
    }
}
