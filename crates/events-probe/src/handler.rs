// SPDX-License-Identifier: MIT

//! The per-record decision notification-service's consumers share
//! (`EnvelopeHandler`), with the lookup ADR 0025 gives them (`UserClient`).

use std::future::Future;

use serde::Deserialize;
use serde_json::Value;

/// The shared envelope (docs/system-design.md "Envelope").
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // decoded to check the shape, as the Scala decoder does
struct Envelope {
    event_id: String,
    event_type: String,
    timestamp: String,
    source: String,
    correlation_id: String,
    payload: Value,
}

/// `UserRegistered`'s payload (gluon registry, `user-events-UserRegistered`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserRegistered {
    user_id: String,
    #[allow(dead_code)]
    email: String,
}

/// The recipient lookup: `Ok(Some(email))` found, `Ok(None)` unknown or
/// malformed (400/404, permanently unresolved), `Err` anything else (retried,
/// then dead-lettered).
pub trait Lookup {
    /// Looks `user_id` up.
    fn email(&self, user_id: &str) -> impl Future<Output = Result<Option<String>, String>> + Send;
}

/// How a record was dealt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handled {
    /// The lookup found the user.
    Resolved,
    /// The lookup said there is no such user: terminal, not retried.
    Unresolved,
    /// A null value or an event type this consumer has no route for.
    Skipped,
}

/// Why a record couldn't be dealt with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The envelope or its payload doesn't decode: no retry will help.
    Poison(String),
    /// The lookup failed (5xx, transport, breaker open).
    Lookup(String),
}

/// Handles one record's value.
///
/// # Errors
/// [`Failure`] when the record is poison or the lookup failed.
pub async fn handle(value: Option<&str>, lookup: &impl Lookup) -> Result<Handled, Failure> {
    let Some(raw) = value else {
        return Ok(Handled::Skipped);
    };
    let envelope: Envelope =
        serde_json::from_str(raw).map_err(|error| Failure::Poison(format!("envelope: {error}")))?;
    if envelope.event_type != "UserRegistered" {
        return Ok(Handled::Skipped);
    }
    let event: UserRegistered = serde_json::from_value(envelope.payload)
        .map_err(|error| Failure::Poison(format!("UserRegistered payload: {error}")))?;
    match lookup.email(&event.user_id).await {
        Ok(Some(_)) => Ok(Handled::Resolved),
        Ok(None) => Ok(Handled::Unresolved),
        Err(error) => Err(Failure::Lookup(error)),
    }
}

/// Whether the record's value decodes as an envelope at all (for the log line).
#[must_use]
pub fn event_type(value: &str) -> Option<String> {
    serde_json::from_str::<Envelope>(value)
        .ok()
        .map(|e| e.event_type)
}
