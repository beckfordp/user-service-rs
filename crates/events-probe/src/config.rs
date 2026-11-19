// SPDX-License-Identifier: MIT

//! The probe's env contract (gluon-contract `gluon_contract/probe.py`; the
//! defaults are notification-service's `application.conf`, which the probe
//! stands in for).

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::time::Duration;

use kafka_client::settings::ConsumerSettings;
use telemetry::config::{parse_duration, tracing_from_env};
use telemetry::resilience::{CircuitBreakerConfig, RetryConfig};

/// Everything the probe reads from its environment.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeConfig {
    /// `PROBE_KAFKA_BOOTSTRAP`.
    pub bootstrap: String,
    /// `PROBE_TOPIC`.
    pub topic: String,
    /// `PROBE_GROUP_ID`.
    pub group_id: String,
    /// `PROBE_LOOKUP_URL`: the user-service stand-in's base URL.
    pub lookup_url: String,
    /// `PROBE_METRICS_PORT` (default 9091).
    pub metrics_port: u16,
    /// `PROBE_SERVICE_NAME` (default `events-probe`): `service.name`, the logger.
    pub service_name: String,
    /// `PROBE_LOOKUP_TIMEOUT` (default 5s, notification-service's client timeout).
    pub lookup_timeout: Duration,
    /// `PROBE_LOOKUP_MAX_RETRIES` / `_BASE_DELAY` (3, 100ms: user-client retry).
    pub lookup_retry: RetryConfig,
    /// `PROBE_BREAKER_FAILURE_THRESHOLD` / `_RESET_TIMEOUT` (5, 30s).
    pub breaker: CircuitBreakerConfig,
    /// `PROBE_DLQ_MAX_RETRIES` / `_BASE_DELAY` (3, 200ms: dlq-retry).
    pub dlq_retry: RetryConfig,
    /// `PROBE_RESTART_BASE_DELAY` (1s: stream-supervisor).
    pub restart_base_delay: Duration,
    /// `PROBE_RESTART_MAX_DELAY` (30s).
    pub restart_max_delay: Duration,
    /// `PROBE_LAG_INTERVAL` (5s): librdkafka statistics, hence the lag gauge.
    pub lag_interval: Duration,
    /// `OTEL_EXPORTER_OTLP_ENDPOINT`.
    pub otlp_endpoint: Option<String>,
    /// `OTEL_TRACES_SAMPLER_ARG` (default 1).
    pub sampling_ratio: f64,
}

impl ProbeConfig {
    /// Its consumer: the probe's topic and group, on its broker, with its lag
    /// interval (gluon-rs `kafka-client`).
    #[must_use]
    pub fn consumer_settings(&self) -> ConsumerSettings {
        ConsumerSettings {
            bootstrap: self.bootstrap.clone(),
            group_id: self.group_id.clone(),
            topic: self.topic.clone(),
            lag_interval: self.lag_interval,
        }
    }
}

/// Reads the config; every problem at once.
///
/// # Errors
/// One message per missing or invalid variable.
pub fn from_env<S: BuildHasher>(
    env: &HashMap<String, String, S>,
) -> Result<ProbeConfig, Vec<String>> {
    let mut problems = Vec::new();
    let mut required = |name: &str| match env.get(name).map(|v| v.trim()) {
        Some(value) if !value.is_empty() => value.to_owned(),
        _ => {
            problems.push(format!("{name} is required"));
            String::new()
        }
    };
    let bootstrap = required("PROBE_KAFKA_BOOTSTRAP");
    let lookup_url = required("PROBE_LOOKUP_URL");
    let text =
        |name: &str, default: &str| env.get(name).cloned().unwrap_or_else(|| default.to_owned());
    let mut duration = |name: &str, default: &str| match parse_duration(&text(name, default)) {
        Ok(value) => value,
        Err(error) => {
            problems.push(format!("{name}: {error}"));
            Duration::ZERO
        }
    };
    let lookup_timeout = duration("PROBE_LOOKUP_TIMEOUT", "5s");
    let lookup_delay = duration("PROBE_LOOKUP_BASE_DELAY", "100ms");
    let reset_timeout = duration("PROBE_BREAKER_RESET_TIMEOUT", "30s");
    let dlq_delay = duration("PROBE_DLQ_BASE_DELAY", "200ms");
    let restart_base_delay = duration("PROBE_RESTART_BASE_DELAY", "1s");
    let restart_max_delay = duration("PROBE_RESTART_MAX_DELAY", "30s");
    let lag_interval = duration("PROBE_LAG_INTERVAL", "5s");
    let mut number = |name: &str, default: &str| {
        if let Ok(value) = text(name, default).trim().parse::<u32>() {
            value
        } else {
            problems.push(format!(
                "{name} must be a whole number, got {:?}",
                text(name, default)
            ));
            0
        }
    };
    let lookup_retries = number("PROBE_LOOKUP_MAX_RETRIES", "3");
    let failure_threshold = number("PROBE_BREAKER_FAILURE_THRESHOLD", "5");
    let dlq_retries = number("PROBE_DLQ_MAX_RETRIES", "3");
    let metrics_port = match text("PROBE_METRICS_PORT", "9091").trim().parse::<u16>() {
        Ok(port) if port > 0 => port,
        _ => {
            problems.push(format!(
                "PROBE_METRICS_PORT must be a port number, got {:?}",
                text("PROBE_METRICS_PORT", "9091")
            ));
            0
        }
    };
    let tracing = tracing_from_env(env, &mut problems);
    if !problems.is_empty() {
        return Err(problems);
    }
    Ok(ProbeConfig {
        bootstrap,
        topic: text("PROBE_TOPIC", "user-events"),
        group_id: text("PROBE_GROUP_ID", "events-probe"),
        lookup_url: lookup_url.trim_end_matches('/').to_owned(),
        metrics_port,
        service_name: text("PROBE_SERVICE_NAME", "events-probe"),
        lookup_timeout,
        lookup_retry: RetryConfig {
            max_retries: lookup_retries,
            base_delay: lookup_delay,
        },
        breaker: CircuitBreakerConfig {
            failure_threshold,
            reset_timeout,
        },
        dlq_retry: RetryConfig {
            max_retries: dlq_retries,
            base_delay: dlq_delay,
        },
        restart_base_delay,
        restart_max_delay,
        lag_interval,
        otlp_endpoint: tracing.otlp_endpoint,
        sampling_ratio: tracing.sampling_ratio,
    })
}
