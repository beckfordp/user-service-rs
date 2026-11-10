// SPDX-License-Identifier: MIT

//! The config contract (RUST-2.3 spec FR1): the Scala service's environment
//! variables, defaults and startup validation.

use std::collections::HashMap;
use std::time::Duration;

use user_service::config::{
    AuthConfig, Config, PostgresConfig, TracingConfig, from_env, parse_duration,
};

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn defaults_match_the_scala_service() {
    let config = from_env(&HashMap::new()).unwrap();
    assert_eq!(
        config,
        Config {
            port: 8080,
            metrics_port: 9090,
            service_name: "user-service".to_owned(),
            postgres: PostgresConfig {
                host: "localhost".to_owned(),
                port: 5432,
                database: "user".to_owned(),
                user: "user".to_owned(),
                password: "user".to_owned(),
            },
            auth: AuthConfig {
                access_token_ttl: Duration::from_hours(1),
                refresh_token_ttl: Duration::from_hours(30 * 24),
                pbkdf2_iterations: 210_000,
            },
            tracing: TracingConfig {
                otlp_endpoint: None,
                sampling_ratio: 1.0,
            },
            asyncapi_html_dir: "target/asyncapi-docs".into(),
        }
    );
}

#[test]
fn every_variable_overrides_its_default() {
    let config = from_env(&env(&[
        ("USER_SERVICE_PORT", "18080"),
        ("USER_SERVICE_METRICS_PORT", "19090"),
        ("USER_SERVICE_NAME", "user-service-rs"),
        ("POSTGRES_HOST", "127.0.0.1"),
        ("POSTGRES_PORT", "15432"),
        ("POSTGRES_DB", "db"),
        ("POSTGRES_USER", "u"),
        ("POSTGRES_PASSWORD", "p"),
        ("AUTH_ACCESS_TOKEN_TTL", "15 minutes"),
        ("AUTH_REFRESH_TOKEN_TTL", "7d"),
        ("AUTH_PBKDF2_ITERATIONS", "1000"),
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:4318"),
        ("OTEL_TRACES_SAMPLER_ARG", "0.25"),
        ("ASYNCAPI_HTML_DIR", "/opt/docs/asyncapi"),
    ]))
    .unwrap();
    assert_eq!(
        config.asyncapi_html_dir,
        std::path::PathBuf::from("/opt/docs/asyncapi")
    );
    assert_eq!(config.port, 18080);
    assert_eq!(config.metrics_port, 19090);
    assert_eq!(config.service_name, "user-service-rs");
    assert_eq!(
        config.postgres,
        PostgresConfig {
            host: "127.0.0.1".to_owned(),
            port: 15432,
            database: "db".to_owned(),
            user: "u".to_owned(),
            password: "p".to_owned(),
        }
    );
    assert_eq!(config.auth.access_token_ttl, Duration::from_mins(15));
    assert_eq!(config.auth.refresh_token_ttl, Duration::from_hours(7 * 24));
    assert_eq!(config.auth.pbkdf2_iterations, 1000);
    assert_eq!(
        config.tracing,
        TracingConfig {
            otlp_endpoint: Some("http://127.0.0.1:4318".to_owned()),
            sampling_ratio: 0.25,
        }
    );
}

#[test]
fn blank_otel_variables_mean_unset() {
    let config = from_env(&env(&[
        ("OTEL_EXPORTER_OTLP_ENDPOINT", "  "),
        ("OTEL_TRACES_SAMPLER_ARG", ""),
    ]))
    .unwrap();
    assert_eq!(config.tracing.otlp_endpoint, None);
    assert!((config.tracing.sampling_ratio - 1.0).abs() < f64::EPSILON);
}

#[test]
fn hocon_durations_parse_like_typesafe_config() {
    let cases = [
        ("1h", 3600),
        ("30d", 30 * 86_400),
        ("90s", 90),
        ("2 hours", 7200),
        ("1 day", 86_400),
        ("5m", 300),
        ("10 minutes", 600),
        ("1 second", 1),
    ];
    for (text, seconds) in cases {
        assert_eq!(
            parse_duration(text),
            Ok(Duration::from_secs(seconds)),
            "{text}"
        );
    }
    assert_eq!(parse_duration("1500"), Ok(Duration::from_millis(1500)));
    assert_eq!(parse_duration("250ms"), Ok(Duration::from_millis(250)));
    assert_eq!(parse_duration("3us"), Ok(Duration::from_micros(3)));
    assert_eq!(parse_duration("7ns"), Ok(Duration::from_nanos(7)));
    assert!(parse_duration("soon").is_err());
    assert!(parse_duration("1 fortnight").is_err());
    assert!(parse_duration("").is_err());
}

#[test]
fn every_invalid_value_is_reported_at_once() {
    let error = from_env(&env(&[
        ("USER_SERVICE_PORT", "notaport"),
        ("USER_SERVICE_METRICS_PORT", "70000"),
        ("POSTGRES_PORT", "-1"),
        ("AUTH_ACCESS_TOKEN_TTL", "500ms"),
        ("AUTH_REFRESH_TOKEN_TTL", "whenever"),
        ("AUTH_PBKDF2_ITERATIONS", "0"),
        ("OTEL_TRACES_SAMPLER_ARG", "abc"),
    ]))
    .unwrap_err();
    assert_eq!(error.problems.len(), 7, "{error}");
    let message = error.to_string();
    for variable in [
        "USER_SERVICE_PORT",
        "USER_SERVICE_METRICS_PORT",
        "POSTGRES_PORT",
        "AUTH_ACCESS_TOKEN_TTL",
        "AUTH_REFRESH_TOKEN_TTL",
        "AUTH_PBKDF2_ITERATIONS",
        "OTEL_TRACES_SAMPLER_ARG",
    ] {
        assert!(
            message.contains(variable),
            "{variable} missing from: {message}"
        );
    }
}

#[test]
fn a_sampling_ratio_outside_zero_to_one_is_invalid() {
    for bad in ["1.5", "-0.1", "NaN"] {
        let error = from_env(&env(&[("OTEL_TRACES_SAMPLER_ARG", bad)])).unwrap_err();
        assert!(
            error.to_string().contains("OTEL_TRACES_SAMPLER_ARG"),
            "{bad}"
        );
    }
}
