// SPDX-License-Identifier: MIT

//! The config contract (RUST-2.3): the Scala service's environment variables,
//! defaults and startup validation.
//!
//! Sources: user-service `application.conf` (names and defaults),
//! `UserServiceConfig.validate` (ports, iterations, TTLs), purerest
//! `TracingConfig.fromEnv` (OTLP endpoint, sampler ratio; gluon-rs
//! `telemetry::config::tracing_from_env`). Pure: an
//! environment map in, a [`Config`] or every problem out; the pieces every
//! service reads the same way are gluon-rs `service-basics`.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::time::Duration;

pub use service_basics::config::{ConfigError, PostgresConfig};
use service_basics::config::{port, postgres};
pub use telemetry::config::TracingConfig;
use telemetry::config::tracing_from_env;

/// Everything the service reads from its environment.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// `USER_SERVICE_PORT`: the HTTP API.
    pub port: u16,
    /// `USER_SERVICE_METRICS_PORT`: the Prometheus endpoint.
    pub metrics_port: u16,
    /// `USER_SERVICE_NAME`: `service.name` on spans, the logger name.
    pub service_name: String,
    /// `POSTGRES_*`.
    pub postgres: PostgresConfig,
    /// `AUTH_*`.
    pub auth: AuthConfig,
    /// `OTEL_*`.
    pub tracing: TracingConfig,
    /// `ASYNCAPI_HTML_DIR`: where the rendered AsyncAPI HTML is
    /// (`scripts/asyncapi-html.sh`), relative to the working directory as in
    /// the Scala config; default `target/asyncapi-docs`.
    pub asyncapi_html_dir: PathBuf,
}

/// Token lifetimes and password hashing cost.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthConfig {
    /// `AUTH_ACCESS_TOKEN_TTL`.
    pub access_token_ttl: Duration,
    /// `AUTH_REFRESH_TOKEN_TTL`.
    pub refresh_token_ttl: Duration,
    /// `AUTH_PBKDF2_ITERATIONS`.
    pub pbkdf2_iterations: u32,
}

/// Reads the config from an environment map.
///
/// # Errors
/// [`ConfigError`] listing every invalid variable.
pub fn from_env<S: BuildHasher>(env: &HashMap<String, String, S>) -> Result<Config, ConfigError> {
    let mut problems = Vec::new();
    let text = |name: &str, default: &str| env.get(name).map_or(default, String::as_str).to_owned();
    let http_port = port(env, "USER_SERVICE_PORT", 8080, &mut problems);
    let metrics_port = port(env, "USER_SERVICE_METRICS_PORT", 9090, &mut problems);
    let postgres = postgres(env, "user", &mut problems);

    let access_token_ttl = ttl(env, "AUTH_ACCESS_TOKEN_TTL", "1h", &mut problems);
    let refresh_token_ttl = ttl(env, "AUTH_REFRESH_TOKEN_TTL", "30d", &mut problems);
    let pbkdf2_iterations = match env.get("AUTH_PBKDF2_ITERATIONS") {
        None => 210_000,
        Some(raw) => match raw.trim().parse::<u32>() {
            Ok(iterations) if iterations >= 1 => iterations,
            _ => {
                problems.push(format!(
                    "AUTH_PBKDF2_ITERATIONS must be an integer of at least 1, got {raw:?}"
                ));
                210_000
            }
        },
    };
    let tracing = tracing_from_env(env, &mut problems);

    if !problems.is_empty() {
        return Err(ConfigError { problems });
    }
    Ok(Config {
        port: http_port,
        metrics_port,
        service_name: text("USER_SERVICE_NAME", "user-service"),
        postgres,
        auth: AuthConfig {
            access_token_ttl,
            refresh_token_ttl,
            pbkdf2_iterations,
        },
        tracing,
        asyncapi_html_dir: PathBuf::from(text("ASYNCAPI_HTML_DIR", "target/asyncapi-docs")),
    })
}

/// A token TTL: a HOCON duration of at least one second (the Scala service
/// checks `toSeconds > 0`).
fn ttl<S: BuildHasher>(
    env: &HashMap<String, String, S>,
    name: &str,
    default: &str,
    problems: &mut Vec<String>,
) -> Duration {
    let raw = env.get(name).map_or(default, String::as_str);
    match parse_duration(raw) {
        Ok(duration) if duration.as_secs() > 0 => duration,
        Ok(_) => {
            problems.push(format!("{name} must be at least one second, got {raw:?}"));
            Duration::ZERO
        }
        Err(reason) => {
            problems.push(format!("{name}: {reason}"));
            Duration::ZERO
        }
    }
}

pub use telemetry::config::parse_duration;
