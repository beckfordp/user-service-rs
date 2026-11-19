// SPDX-License-Identifier: MIT

//! The probe's pieces that need no broker (RUST-6.2 spec FR1, FR3): config,
//! its `UserRegistered` route (the lag hook moved with the loop to gluon-rs
//! `kafka-client`, gluon TD-S5). Envelope handling, the DLQ,
//! age, lag and the span are gluon-rs `kafka-event-consumer`'s, tested there.
//! Sources: gluon-contract `probe.py` (env contract); notification-service
//! `UserClient`; purekafka `ConsumerLag`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unused_async_trait_impl
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use events_probe::config::from_env;
use events_probe::handler::{Lookup, LookupError, ProbeError, UserEvents};
use kafka_client::settings::ConsumerSettings;
use kafka_event_consumer::envelope::{Envelope, Routes};
use serde_json::{Value, json};
use telemetry::logging::Logger;

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
    assert_eq!(config.topic.as_str(), "user-events");
    assert_eq!(config.group_id.as_str(), "events-probe");
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
fn the_consumer_is_the_probes_topic_and_group() {
    let config = from_env(&env(&REQUIRED)).unwrap();
    assert_eq!(
        config.consumer_settings(),
        ConsumerSettings {
            bootstrap: config.bootstrap.clone(),
            group_id: config.group_id.clone(),
            topic: config.topic.clone(),
            lag_interval: Duration::from_secs(5),
        }
    );
}

#[test]
fn config_reports_every_problem() {
    let error = from_env(&env(&[
        ("PROBE_LOOKUP_TIMEOUT", "soon"),
        ("PROBE_DLQ_MAX_RETRIES", "-1"),
        ("PROBE_METRICS_PORT", "x"),
        ("PROBE_TOPIC", "user events"),
        ("PROBE_GROUP_ID", ""),
    ]))
    .unwrap_err();
    let text = error.join("\n");
    for needle in [
        "PROBE_KAFKA_BOOTSTRAP",
        "PROBE_LOOKUP_URL",
        "PROBE_LOOKUP_TIMEOUT",
        "PROBE_DLQ_MAX_RETRIES",
        "PROBE_METRICS_PORT",
        "PROBE_TOPIC: topic \"user events\" may hold only",
        "PROBE_GROUP_ID: group id is empty",
    ] {
        assert!(text.contains(needle), "{needle} not in {text}");
    }
}

/// A lookup that answers from a table (`Err(status)`: an unexpected response)
/// and records who it was asked about.
struct Table {
    answers: HashMap<String, Result<Option<String>, u16>>,
    asked: Mutex<Vec<String>>,
}

impl Lookup for Table {
    async fn email(&self, user_id: &str) -> Result<Option<String>, LookupError> {
        self.asked.lock().unwrap().push(user_id.to_owned());
        self.answers
            .get(user_id)
            .cloned()
            .unwrap_or(Ok(None))
            .map_err(|status| LookupError::Status {
                user_id: user_id.to_owned(),
                status,
            })
    }
}

/// The probe's routes over a lookup table, and the lines they log.
fn routes(
    entries: &[(&str, Result<Option<String>, u16>)],
) -> (UserEvents<Table>, Arc<Mutex<Vec<u8>>>) {
    let log: Arc<Mutex<Vec<u8>>> = Arc::default();
    let lookup = Table {
        answers: entries
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect(),
        asked: Mutex::default(),
    };
    let routes = UserEvents {
        topic: "user-events".to_owned(),
        lookup,
        logger: Logger::to_writer("events-probe", log.clone()),
    };
    (routes, log)
}

fn logged(log: &Arc<Mutex<Vec<u8>>>) -> Vec<(String, String)> {
    String::from_utf8(log.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| {
            let line: Value = serde_json::from_str(line).unwrap();
            (
                line["level"].as_str().unwrap().to_owned(),
                line["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn envelope(event_type: &str, payload: &Value) -> Envelope {
    serde_json::from_value(json!({
        "eventId": "e1", "eventType": event_type, "timestamp": "2026-11-09T18:43:19.487722Z",
        "source": "user-service", "correlationId": "u1", "payload": payload
    }))
    .unwrap()
}

#[tokio::test]
async fn a_user_registered_event_is_looked_up_and_resolved() {
    let (routes, log) = routes(&[("u1", Ok(Some("u1@example.com".to_owned())))]);
    let event = routes
        .decode(&envelope(
            "UserRegistered",
            &json!({"userId": "u1", "email": "u1@example.com"}),
        ))
        .unwrap()
        .unwrap();
    assert!(routes.handle("e1", &event).await.is_ok());
    assert_eq!(*routes.lookup.asked.lock().unwrap(), ["u1"]);
    assert_eq!(
        logged(&log),
        [("INFO".to_owned(), "Recipient resolved".to_owned())]
    );
}

#[tokio::test]
async fn an_unknown_user_is_unresolved_and_done() {
    let (routes, log) = routes(&[]);
    let event = routes
        .decode(&envelope(
            "UserRegistered",
            &json!({"userId": "u2", "email": "e"}),
        ))
        .unwrap()
        .unwrap();
    assert!(routes.handle("e1", &event).await.is_ok());
    assert_eq!(
        logged(&log),
        [("WARN".to_owned(), "Recipient unresolved".to_owned())]
    );
}

#[tokio::test]
async fn a_failing_lookup_fails_the_event_for_the_guard_to_retry() {
    let (routes, log) = routes(&[("u4", Err(503))]);
    let event = routes
        .decode(&envelope(
            "UserRegistered",
            &json!({"userId": "u4", "email": "e"}),
        ))
        .unwrap()
        .unwrap();
    let result = routes.handle("e1", &event).await;
    let Err(ProbeError::Lookup(error @ LookupError::Status { status: 503, .. })) = result else {
        panic!("want the lookup's 503, got {result:?}");
    };
    assert_eq!(
        error.to_string(),
        "Unexpected response from user-service for u4: 503"
    );
    assert_eq!(
        logged(&log),
        [("ERROR".to_owned(), "Recipient lookup failed".to_owned())]
    );
}

#[test]
fn other_types_have_no_route_and_a_bad_payload_is_poison() {
    let (routes, _log) = routes(&[]);
    assert!(
        routes
            .decode(&envelope("UserDeleted", &json!({"userId": "u3"})))
            .is_none()
    );
    assert!(matches!(
        routes.decode(&envelope("UserRegistered", &json!({"email": "no user id"}))),
        Some(Err(ProbeError::Payload(_)))
    ));
}
