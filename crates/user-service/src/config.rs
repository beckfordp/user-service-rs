// SPDX-License-Identifier: MIT

//! The config contract (RUST-2.3): the Scala service's environment variables,
//! defaults and startup validation.
//!
//! Sources: user-service `application.conf` (names and defaults),
//! `UserServiceConfig.validate` (ports, iterations, TTLs), purerest
//! `TracingConfig.fromEnv` (OTLP endpoint, sampler ratio). Pure: an
//! environment map in, a [`Config`] or every problem out.

use std::collections::HashMap;
use std::fmt;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::time::Duration;

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

/// Where the service's database is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresConfig {
    /// `POSTGRES_HOST`.
    pub host: String,
    /// `POSTGRES_PORT`.
    pub port: u16,
    /// `POSTGRES_DB`.
    pub database: String,
    /// `POSTGRES_USER`.
    pub user: String,
    /// `POSTGRES_PASSWORD`.
    pub password: String,
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

/// Where spans go, and how many.
#[derive(Debug, Clone, PartialEq)]
pub struct TracingConfig {
    /// `OTEL_EXPORTER_OTLP_ENDPOINT`; `None` when unset or blank.
    pub otlp_endpoint: Option<String>,
    /// `OTEL_TRACES_SAMPLER_ARG`, in [0, 1]; 1.0 when unset or blank.
    pub sampling_ratio: f64,
}

/// Every problem with the environment, so one fix-up round is enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// One line per invalid variable.
    pub problems: Vec<String>,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid configuration:")?;
        for problem in &self.problems {
            write!(f, "\n- {problem}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigError {}

/// Reads the config from an environment map.
///
/// # Errors
/// [`ConfigError`] listing every invalid variable.
pub fn from_env<S: BuildHasher>(env: &HashMap<String, String, S>) -> Result<Config, ConfigError> {
    let mut problems = Vec::new();
    let text = |name: &str, default: &str| env.get(name).map_or(default, String::as_str).to_owned();
    let mut port = |name: &str, default: u16| match env.get(name) {
        None => default,
        Some(raw) => raw.trim().parse::<u16>().unwrap_or_else(|_| {
            problems.push(format!(
                "{name} must be a port number (0-65535), got {raw:?}"
            ));
            default
        }),
    };
    let http_port = port("USER_SERVICE_PORT", 8080);
    let metrics_port = port("USER_SERVICE_METRICS_PORT", 9090);
    let postgres_port = port("POSTGRES_PORT", 5432);

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
    let otlp_endpoint = env
        .get("OTEL_EXPORTER_OTLP_ENDPOINT")
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let sampling_ratio = match env.get("OTEL_TRACES_SAMPLER_ARG").map(|v| v.trim()) {
        None | Some("") => 1.0,
        Some(raw) => match raw.parse::<f64>() {
            Ok(ratio) if (0.0..=1.0).contains(&ratio) => ratio,
            _ => {
                problems.push(format!(
                    "OTEL_TRACES_SAMPLER_ARG must be a number between 0 and 1, got {raw:?}"
                ));
                1.0
            }
        },
    };

    if !problems.is_empty() {
        return Err(ConfigError { problems });
    }
    Ok(Config {
        port: http_port,
        metrics_port,
        service_name: text("USER_SERVICE_NAME", "user-service"),
        postgres: PostgresConfig {
            host: text("POSTGRES_HOST", "localhost"),
            port: postgres_port,
            database: text("POSTGRES_DB", "user"),
            user: text("POSTGRES_USER", "user"),
            password: text("POSTGRES_PASSWORD", "user"),
        },
        auth: AuthConfig {
            access_token_ttl,
            refresh_token_ttl,
            pbkdf2_iterations,
        },
        tracing: TracingConfig {
            otlp_endpoint,
            sampling_ratio,
        },
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
