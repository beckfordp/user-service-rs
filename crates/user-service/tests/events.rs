// SPDX-License-Identifier: MIT

//! The `UserRegistered` envelope and its payload schema (RUST-3.1 spec FR2-FR3).
//! Sources: docs/system-design.md "Envelope" and `UserRegistered`; gluon
//! registry/user-service/registry.json; capture 20261109 db-outbox-events.jsonl.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use serde_json::{Value, json};
use time::macros::datetime;
use user_service::events;
use uuid::Uuid;

fn user() -> Uuid {
    Uuid::parse_str("691b03e8-f1d7-45d9-a561-5183ffae9d0b").unwrap()
}

#[test]
fn the_row_is_keyed_and_correlated_by_the_user() {
    let event = events::user_registered(
        user(),
        "kat@example.com",
        datetime!(2026-11-09 18:43:19.487722 UTC),
    );
    assert_eq!(event.aggregate_id, user().to_string());
    assert_eq!(event.correlation_id, user().to_string());
}

#[test]
fn the_envelope_has_the_shared_fields_and_the_payload_inside() {
    let event = events::user_registered(
        user(),
        "kat@example.com",
        datetime!(2026-11-09 18:43:19.487722 UTC),
    );
    let envelope = event.payload.as_object().unwrap();
    let mut keys: Vec<&str> = envelope.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "correlationId",
            "eventId",
            "eventType",
            "payload",
            "source",
            "timestamp"
        ]
    );
    assert_eq!(envelope["eventType"], "UserRegistered");
    assert_eq!(envelope["source"], "user-service");
    assert_eq!(envelope["correlationId"], user().to_string());
    assert_eq!(envelope["timestamp"], "2026-11-09T18:43:19.487722Z");
    assert_eq!(
        envelope["payload"],
        json!({"userId": user().to_string(), "email": "kat@example.com"})
    );
    let event_id = Uuid::parse_str(envelope["eventId"].as_str().unwrap()).unwrap();
    assert_ne!(event_id, event.id, "eventId is not the row id");
}

#[test]
fn each_event_gets_fresh_ids() {
    let now = datetime!(2026-11-09 18:43:19 UTC);
    let first = events::user_registered(user(), "kat@example.com", now);
    let second = events::user_registered(user(), "kat@example.com", now);
    assert_ne!(first.id, second.id);
    assert_ne!(first.payload["eventId"], second.payload["eventId"]);
}

#[test]
fn the_timestamp_has_the_java_clock_resolution() {
    // Instant.now() on Linux ticks in microseconds; the capture's timestamps
    // have six digits (db-outbox-events.jsonl).
    let event = events::user_registered(
        user(),
        "kat@example.com",
        datetime!(2026-11-09 18:43:19.487722999 UTC),
    );
    assert_eq!(event.payload["timestamp"], "2026-11-09T18:43:19.487722Z");
}

fn registry_file() -> PathBuf {
    std::env::var_os("GLUON_REGISTRY")
        .map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../registry"),
            PathBuf::from,
        )
        .join("user-service/registry.json")
}

fn latest_registered(artifact: &str) -> Value {
    let path = registry_file();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "gluon registry not found at {} ({error}); set GLUON_REGISTRY to gluon's registry/",
            path.display()
        )
    });
    let registry: Value = serde_json::from_str(&text).unwrap();
    let artifact = registry["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["artifactId"] == artifact)
        .unwrap_or_else(|| panic!("{artifact} is not in {}", path.display()));
    artifact["versions"].as_array().unwrap().last().unwrap()["schema"].clone()
}

#[test]
fn the_payload_schema_is_the_latest_registered_version() {
    // ADR 0028 decision 4: CI checks the generated schema against gluon's
    // registry; contracts are frozen, so it must equal the latest version.
    assert_eq!(
        events::user_registered_schema(),
        latest_registered("user-events-UserRegistered")
    );
}
