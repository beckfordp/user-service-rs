// SPDX-License-Identifier: MIT

//! The recipient lookup (ADR 0025; notification-service `UserClient`):
//! `GET {base}/users/{id}` through purerest's resilience, traced and metered
//! (`telemetry::client`).

use std::time::Duration;

use serde::Deserialize;
use telemetry::client::{ResilientClient, TracedClient};
use telemetry::metrics::Metrics;
use telemetry::resilience::{CircuitBreakerConfig, RetryConfig};
use telemetry::tracing::SdkTracer;

use crate::handler::Lookup;

#[derive(Debug, Deserialize)]
struct UserBody {
    email: String,
}

/// The lookup: request timeout, client span and metrics, circuit breaker,
/// retry.
#[derive(Debug, Clone)]
pub struct HttpLookup {
    client: ResilientClient,
    base_url: String,
}

impl HttpLookup {
    /// Calls `base_url` with these settings.
    ///
    /// # Errors
    /// When the HTTP client can't be built.
    pub fn new(
        base_url: &str,
        timeout: Duration,
        retry: RetryConfig,
        breaker: CircuitBreakerConfig,
        tracer: SdkTracer,
        metrics: &Metrics,
    ) -> Result<Self, String> {
        let instrumented =
            TracedClient::new(timeout, tracer, metrics).map_err(|error| error.to_string())?;
        Ok(Self {
            client: ResilientClient::new(instrumented, retry, breaker, metrics),
            base_url: base_url.to_owned(),
        })
    }
}

impl Lookup for HttpLookup {
    async fn email(&self, user_id: &str) -> Result<Option<String>, String> {
        let url = format!("{}/users/{user_id}", self.base_url);
        let response = self
            .client
            .get(&url)
            .await
            .map_err(|error| error.to_string())?;
        match response.status().as_u16() {
            200 => response
                .json::<UserBody>()
                .await
                .map(|body| Some(body.email))
                .map_err(|error| error.to_string()),
            400 | 404 => Ok(None),
            other => Err(format!(
                "Unexpected response from user-service for {user_id}: {other}"
            )),
        }
    }
}
