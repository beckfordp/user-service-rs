// SPDX-License-Identifier: MIT

//! The `events-probe` binary: reads its env contract, starts tracing and the
//! metrics endpoint, then consumes until SIGTERM or SIGINT.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::process::ExitCode;

use events_probe::config::{ProbeConfig, from_env};
use events_probe::handler::UserEvents;
use events_probe::lookup::HttpLookup;
use kafka_client::supervised::{ConsumerTelemetry, Resilience, run_supervised};
use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use telemetry::tracing::Tracing;
use tokio::net::TcpListener;
use tokio::sync::watch;

fn main() -> ExitCode {
    let env: HashMap<String, String> = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    let config = match from_env(&env) {
        Ok(config) => config,
        Err(problems) => {
            for problem in problems {
                eprintln!("events-probe: {problem}");
            }
            return ExitCode::FAILURE;
        }
    };
    let tracing = match Tracing::new(
        &config.service_name,
        config.otlp_endpoint.as_deref(),
        config.sampling_ratio,
    ) {
        Ok(tracing) => tracing,
        Err(error) => {
            eprintln!("events-probe: {error}");
            return ExitCode::FAILURE;
        }
    };
    let Ok(runtime) = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    else {
        eprintln!("events-probe: can't start the async runtime");
        return ExitCode::FAILURE;
    };
    let code = runtime.block_on(run(config, tracing.tracer()));
    drop(runtime);
    tracing.shutdown();
    code
}

async fn run(config: ProbeConfig, tracer: telemetry::tracing::SdkTracer) -> ExitCode {
    let logger = Logger::stdout(&config.service_name);
    let metrics = Metrics::new(&config.service_name);
    let listener = match TcpListener::bind(SocketAddr::from((
        Ipv4Addr::UNSPECIFIED,
        config.metrics_port,
    )))
    .await
    {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "events-probe: metrics port {}: {error}",
                config.metrics_port
            );
            return ExitCode::FAILURE;
        }
    };
    let lookup = match HttpLookup::new(
        &config.lookup_url,
        config.lookup_timeout,
        config.lookup_retry,
        config.breaker,
        tracer.clone(),
        &metrics,
    ) {
        Ok(lookup) => lookup,
        Err(error) => {
            eprintln!("events-probe: {error}");
            return ExitCode::FAILURE;
        }
    };
    let (stop, stopped) = watch::channel(false);
    let mut server_stop = stopped.clone();
    let server = tokio::spawn(
        axum::serve(listener, telemetry::metrics::router(metrics.clone()))
            .with_graceful_shutdown(async move {
                let _ = server_stop.wait_for(|stop| *stop).await;
            })
            .into_future(),
    );
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = stop.send(true);
    });
    logger.info(
        &[("topic", &config.topic), ("group", &config.group_id)],
        "events-probe starting",
    );
    let routes = UserEvents {
        topic: config.topic.clone(),
        lookup,
        logger: logger.clone(),
    };
    // notification-service's kafka settings (gluon-rs Resilience::default), the
    // probe's own dlq-retry and restart delays.
    let resilience = Resilience {
        dlq_retry: config.dlq_retry,
        restart_base_delay: config.restart_base_delay,
        restart_max_delay: config.restart_max_delay,
        ..Resilience::default()
    };
    let consumer_telemetry = ConsumerTelemetry {
        tracer,
        logger,
        metrics,
    };
    let consumed = run_supervised(
        config.consumer_settings(),
        &resilience,
        routes,
        consumer_telemetry,
        stopped.clone(),
    )
    .await;
    server.abort();
    if let Err(error) = consumed {
        eprintln!("events-probe: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn shutdown_signal() {
    let interrupt = tokio::signal::ctrl_c();
    let Ok(mut terminate) =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    else {
        let _ = interrupt.await;
        return;
    };
    tokio::select! {
        _ = interrupt => {}
        _ = terminate.recv() => {}
    }
}
