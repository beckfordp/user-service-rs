// SPDX-License-Identifier: MIT

//! The probe's pieces that need no broker (RUST-6.2 spec FR1, FR3-FR5): config,
//! the per-record decision, event age, lag from librdkafka's statistics and the
//! consumer span. Sources: gluon-contract `probe.py` (env contract);
//! notification-service `EnvelopeHandler`, `UserClient`; purekafka
//! `KafkaTracing.traced`, `EventAge`, `ConsumerLag`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unused_async_trait_impl
)]

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use events_probe::config::from_env;
use events_probe::handler::{Failure, Handled, Lookup, handle};
use events_probe::kafka::{consumer_span, event_age_seconds, lag_from_statistics};
use opentelemetry::trace::{SpanKind, TraceContextExt};
use opentelemetry_sdk::trace::InMemorySpanExporter;
use serde_json::json;
use telemetry::tracing::Tracing;

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

const REQUIRED: [(&str, &str); 4] = [
    ("PROBE_KAFKA_BOOTSTRAP", "127.0.0.1:9092"),
    ("PROBE_TOPIC", "user-events"),
    ("PROBE_GROUP_ID", "events-probe"),
    ("PROBE_LOOKUP_URL", "http://127.0.0.1:8084"),
];

#[test]
fn config_defaults_are_notification_services() {
    let config = from_env(&env(&REQUIRED)).unwrap();
    assert_eq!(config.bootstrap, "127.0.0.1:9092");
    assert_eq!(config.lookup_timeout, Duration::from_secs(5));
    assert_eq!(
        (
            config.lookup_retry.max_retries,
            config.lookup_retry.base_delay
        ),
        (3, Duration::from_millis(100))
    );
    assert_eq!(
        (
            config.breaker.failure_threshold,
            config.breaker.reset_timeout
        ),
        (5, Duration::from_secs(30))
    );
    assert_eq!(
        (config.dlq_retry.max_retries, config.dlq_retry.base_delay),
        (3, Duration::from_millis(200))
    );
    assert_eq!(
        (config.restart_base_delay, config.restart_max_delay),
        (Duration::from_secs(1), Duration::from_secs(30))
    );
    assert_eq!(config.lag_interval, Duration::from_secs(5));
    assert_eq!(config.metrics_port, 9091);
    assert_eq!(config.service_name, "events-probe");
}

#[test]
fn config_reports_every_problem() {
    let error = from_env(&env(&[
        ("PROBE_LOOKUP_TIMEOUT", "soon"),
        ("PROBE_DLQ_MAX_RETRIES", "-1"),
        ("PROBE_METRICS_PORT", "x"),
    ]))
    .unwrap_err();
    let text = error.join("\n");
    for needle in [
        "PROBE_KAFKA_BOOTSTRAP",
        "PROBE_LOOKUP_URL",
        "PROBE_LOOKUP_TIMEOUT",
        "PROBE_DLQ_MAX_RETRIES",
        "PROBE_METRICS_PORT",
    ] {
        assert!(text.contains(needle), "{needle} not in {text}");
    }
}

/// A lookup that answers from a table and records who it was asked about.
struct Table {
    answers: HashMap<String, Result<Option<String>, String>>,
    asked: Mutex<Vec<String>>,
}

impl Lookup for Table {
    async fn email(&self, user_id: &str) -> Result<Option<String>, String> {
        self.asked.lock().unwrap().push(user_id.to_owned());
        self.answers.get(user_id).cloned().unwrap_or(Ok(None))
    }
}

fn table(entries: &[(&str, Result<Option<String>, String>)]) -> Table {
    Table {
        answers: entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        asked: Mutex::default(),
    }
}

fn envelope(event_type: &str, payload: &serde_json::Value) -> String {
    json!({
        "eventId": "e1", "eventType": event_type, "timestamp": "2026-11-09T18:43:19.487722Z",
        "source": "user-service", "correlationId": "u1", "payload": payload
    })
    .to_string()
}

#[tokio::test]
async fn a_user_registered_event_is_looked_up() {
    let lookup = table(&[("u1", Ok(Some("u1@example.com".to_owned())))]);
    let value = envelope(
        "UserRegistered",
        &json!({"userId": "u1", "email": "u1@example.com"}),
    );
    assert_eq!(handle(Some(&value), &lookup).await, Ok(Handled::Resolved));
    assert_eq!(*lookup.asked.lock().unwrap(), ["u1"]);
}

#[tokio::test]
async fn an_unknown_user_is_unresolved() {
    let lookup = table(&[]);
    let value = envelope("UserRegistered", &json!({"userId": "u2", "email": "e"}));
    assert_eq!(handle(Some(&value), &lookup).await, Ok(Handled::Unresolved));
}

#[tokio::test]
async fn unknown_types_and_null_values_are_skipped_without_a_lookup() {
    let lookup = table(&[]);
    let value = envelope("UserDeleted", &json!({"userId": "u3"}));
    assert_eq!(handle(Some(&value), &lookup).await, Ok(Handled::Skipped));
    assert_eq!(handle(None, &lookup).await, Ok(Handled::Skipped));
    assert!(lookup.asked.lock().unwrap().is_empty());
}

#[tokio::test]
async fn undecodable_envelopes_and_payloads_are_poison() {
    let lookup = table(&[]);
    assert!(matches!(
        handle(Some("{nope"), &lookup).await,
        Err(Failure::Poison(_))
    ));
    let missing = json!({"eventType": "UserRegistered"}).to_string();
    assert!(matches!(
        handle(Some(&missing), &lookup).await,
        Err(Failure::Poison(_))
    ));
    let bad_payload = envelope("UserRegistered", &json!({"email": "no user id"}));
    assert!(matches!(
        handle(Some(&bad_payload), &lookup).await,
        Err(Failure::Poison(_))
    ));
}

#[tokio::test]
async fn a_failing_lookup_is_a_lookup_failure() {
    let lookup = table(&[("u4", Err("503".to_owned()))]);
    let value = envelope("UserRegistered", &json!({"userId": "u4", "email": "e"}));
    assert_eq!(
        handle(Some(&value), &lookup).await,
        Err(Failure::Lookup("503".to_owned()))
    );
}

#[test]
fn event_age_is_now_minus_the_record_time_never_negative() {
    assert!((event_age_seconds(10_500, 10_000) - 0.5).abs() < f64::EPSILON);
    assert!(event_age_seconds(10_000, 12_000).abs() < f64::EPSILON);
}

#[test]
fn lag_comes_from_librdkafkas_statistics_per_assigned_partition() {
    let statistics = json!({
        "topics": {
            "user-events": {"partitions": {
                "0": {"partition": 0, "consumer_lag": 3},
                "1": {"partition": 1, "consumer_lag": -1},
                "-1": {"partition": -1, "consumer_lag": -1}
            }},
            "other": {"partitions": {"2": {"partition": 2, "consumer_lag": 0}}}
        }
    })
    .to_string();
    let mut lag = lag_from_statistics(&statistics);
    lag.sort();
    assert_eq!(
        lag,
        [("other".to_owned(), 2, 0), ("user-events".to_owned(), 0, 3)]
    );
    assert_eq!(lag_from_statistics("not json"), []);
}

#[test]
fn each_record_gets_a_consumer_span_joining_its_traceparent() {
    let exporter = InMemorySpanExporter::default();
    let tracing = Tracing::with_exporter("events-probe", 1.0, exporter.clone());
    let headers = vec![(
        "traceparent".to_owned(),
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01".to_owned(),
    )];
    let cx = consumer_span(&tracing.tracer(), &headers, "user-events");
    cx.span().end();
    tracing.force_flush();
    let spans = exporter.get_finished_spans().unwrap();
    let span = &spans[0];
    assert_eq!(span.name, "user-events process");
    assert_eq!(span.span_kind, SpanKind::Consumer);
    assert_eq!(
        span.span_context.trace_id().to_string(),
        "4bf92f3577b34da6a3ce929d0e0e4736"
    );
    assert_eq!(span.parent_span_id.to_string(), "00f067aa0ba902b7");
    let attributes: Vec<(String, String)> = span
        .attributes
        .iter()
        .map(|kv| (kv.key.to_string(), kv.value.to_string()))
        .collect();
    assert_eq!(
        attributes,
        [
            ("messaging.system".to_owned(), "kafka".to_owned()),
            (
                "messaging.destination.name".to_owned(),
                "user-events".to_owned()
            )
        ]
    );
}
