// SPDX-License-Identifier: MIT

//! The probe's one route (`UserRegistered`), with the lookup ADR 0025 gives
//! notification-service's consumers (`UserClient`). The rest of the
//! per-record decision is gluon-rs `kafka-event-consumer`'s `handle_record`.

use std::future::Future;

use kafka_event_consumer::envelope::{Envelope, Routes};
use serde::Deserialize;
use telemetry::client::ClientError;
use telemetry::logging::Logger;

/// `UserRegistered`'s payload (gluon registry, `user-events-UserRegistered`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRegistered {
    /// Whom to look up.
    pub user_id: String,
    /// Decoded to check the shape, as the Scala decoder does.
    pub email: String,
}

/// Why a lookup failed: retried, then dead-lettered. Each reads as the
/// message it replaced (gluon TD-64.1).
#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    /// No response (breaker open, transport).
    #[error(transparent)]
    Client(#[from] ClientError),
    /// A 200 whose body isn't a user.
    #[error(transparent)]
    Body(reqwest::Error),
    /// Neither 200 nor 400/404.
    #[error("Unexpected response from user-service for {user_id}: {status}")]
    Status {
        /// Whom it was asked about.
        user_id: String,
        /// What it answered.
        status: u16,
    },
}

/// Why a `UserRegistered` wasn't handled.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// The payload isn't a `UserRegistered` (poison).
    #[error(transparent)]
    Payload(#[from] serde_json::Error),
    /// The lookup failed.
    #[error(transparent)]
    Lookup(#[from] LookupError),
}

/// The recipient lookup: `Ok(Some(email))` found, `Ok(None)` unknown or
/// malformed (400/404, permanently unresolved), `Err` anything else (retried,
/// then dead-lettered).
pub trait Lookup {
    /// Looks `user_id` up.
    fn email(
        &self,
        user_id: &str,
    ) -> impl Future<Output = Result<Option<String>, LookupError>> + Send;
}

/// The probe's routes on `topic`: `UserRegistered` → the lookup.
pub struct UserEvents<L> {
    /// The topic, for the log lines.
    pub topic: String,
    /// The recipient lookup.
    pub lookup: L,
    /// JSON log lines.
    pub logger: Logger,
}

impl<L: Lookup + Sync> Routes for UserEvents<L> {
    type Event = UserRegistered;
    type Error = ProbeError;

    fn decode(&self, envelope: &Envelope) -> Option<Result<UserRegistered, ProbeError>> {
        (envelope.event_type == "UserRegistered")
            .then(|| serde_json::from_value(envelope.payload.clone()).map_err(ProbeError::from))
    }

    /// Resolved and unresolved are both done; a failed lookup is retried.
    async fn handle(&self, _event_id: &str, event: &UserRegistered) -> Result<(), ProbeError> {
        let topic = self.topic.as_str();
        match self.lookup.email(&event.user_id).await {
            Ok(Some(_)) => {
                self.logger.info(&[("topic", topic)], "Recipient resolved");
                Ok(())
            }
            Ok(None) => {
                self.logger
                    .warn(&[("topic", topic)], "Recipient unresolved");
                Ok(())
            }
            Err(error) => {
                self.logger.error(
                    &[("topic", topic), ("error", &error.to_string())],
                    "Recipient lookup failed",
                );
                Err(error.into())
            }
        }
    }
}
