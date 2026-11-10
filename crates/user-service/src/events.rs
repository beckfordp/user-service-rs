// SPDX-License-Identifier: MIT

//! The `UserRegistered` event as the outbox stores it (RUST-3.1): the shared
//! envelope (docs/system-design.md "Envelope", ADR 0009) around the payload
//! registered in gluon `registry/user-service/registry.json`.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde::Serialize;
use serde_json::Value;
use telemetry::docs::asyncapi::{self, Channel};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::users::java_instant;

/// The `UserRegistered` event type.
pub const USER_REGISTERED: &str = "UserRegistered";

/// The envelope's `source`: the publishing service's name.
pub const SOURCE: &str = "user-service";

/// New user signed up: POST /auth/signup succeeded.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(title = "UserRegisteredEvent")]
pub struct UserRegistered {
    /// User UUID. Also the partition key.
    #[schemars(example = &"5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e")]
    pub user_id: String,
    /// User's email address.
    #[schemars(example = &"jane@example.com")]
    pub email: String,
}

/// The envelope every event on every domain topic shares.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<P> {
    event_id: String,
    event_type: &'static str,
    timestamp: String,
    source: &'static str,
    correlation_id: String,
    payload: P,
}

/// An `outbox_events` row, ready to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct OutboxEvent {
    /// The row id (not the envelope's `eventId`).
    pub id: Uuid,
    /// The envelope's `eventType`.
    pub event_type: &'static str,
    /// The Kafka key: the userId.
    pub aggregate_id: String,
    /// The envelope, stored as JSONB and published as is.
    pub payload: Value,
    /// The envelope's `correlationId`, also a Kafka header.
    pub correlation_id: String,
}

/// The `UserRegistered` row for a new account, stamped `now` at the
/// resolution of Java's clock (microseconds), as the Scala service's are.
#[must_use]
pub fn user_registered(user_id: Uuid, email: &str, now: OffsetDateTime) -> OutboxEvent {
    let user = user_id.to_string();
    let envelope = Envelope {
        event_id: Uuid::new_v4().to_string(),
        event_type: USER_REGISTERED,
        timestamp: java_instant(to_micros(now)),
        source: SOURCE,
        correlation_id: user.clone(),
        payload: UserRegistered {
            user_id: user.clone(),
            email: email.to_owned(),
        },
    };
    OutboxEvent {
        id: Uuid::new_v4(),
        event_type: USER_REGISTERED,
        aggregate_id: user.clone(),
        // An envelope of strings and a UUID always serializes.
        payload: serde_json::to_value(envelope).unwrap_or(Value::Null),
        correlation_id: user,
    }
}

/// The AsyncAPI document (purekafka `AsyncApiDoc`, user-service
/// `AsyncApiChannels`): `UserRegistered` published on `user-events` through
/// the outbox.
#[must_use]
pub fn asyncapi() -> Value {
    asyncapi::document(
        "User Service",
        "1.0",
        &[Channel::publish_cdc::<UserRegistered>(
            "user-events",
            USER_REGISTERED,
        )],
    )
}

/// The payload's JSON Schema as the registry holds it: draft-07, closed.
#[must_use]
pub fn user_registered_schema() -> Value {
    SchemaSettings::draft07()
        .into_generator()
        .root_schema_for::<UserRegistered>()
        .to_value()
}

fn to_micros(now: OffsetDateTime) -> OffsetDateTime {
    let micros = now.nanosecond() / 1_000 * 1_000;
    now.replace_nanosecond(micros).unwrap_or(now)
}
