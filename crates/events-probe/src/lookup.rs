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

use crate::handler::{Lookup, LookupError};

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
    ) -> Result<Self, reqwest::Error> {
        let instrumented = TracedClient::new(timeout, tracer, metrics)?;
        Ok(Self {
            client: ResilientClient::new(instrumented, retry, breaker, metrics),
            base_url: base_url.to_owned(),
        })
    }
}

impl Lookup for HttpLookup {
    async fn email(&self, user_id: &str) -> Result<Option<String>, LookupError> {
        let url = format!("{}/users/{user_id}", self.base_url);
        let response = self.client.get(&url).await?;
        match response.status().as_u16() {
            200 => response
                .json::<UserBody>()
                .await
                .map(|body| Some(body.email))
                .map_err(LookupError::Body),
            400 | 404 => Ok(None),
            status => Err(LookupError::Status {
                user_id: user_id.to_owned(),
                status,
            }),
        }
    }
}
