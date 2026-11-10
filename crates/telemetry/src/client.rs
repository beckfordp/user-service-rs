// SPDX-License-Identifier: MIT

//! purerest's HTTP client side (RUST-6.3): `ClientTracing` (a CLIENT span
//! `<METHOD> <host>` under the active span, its context sent as
//! `traceparent`), `ClientMetrics` (`http.client.request.duration`), and
//! `Resilience` (retry outside the circuit breaker).

use std::time::{Duration, Instant};

use opentelemetry::propagation::{Injector, TextMapPropagator};
use opentelemetry::trace::{SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::metrics::{Histogram, LATENCY_BUCKETS, Metrics};
use crate::resilience::{CircuitBreaker, CircuitBreakerConfig, Retry, RetryConfig};
use crate::tracing::SdkTracer;

/// A client call that didn't produce a response.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The circuit breaker rejected the call without making it.
    #[error("Circuit breaker is open")]
    Open,
    /// The request failed on the way (connection, timeout, reset).
    #[error(transparent)]
    Transport(#[from] reqwest::Error),
}

struct HeaderInjector<'a>(&'a mut HeaderMap);

impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(key.as_bytes()),
            HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, value);
        }
    }
}

/// The error's type for `error.type` (the Java services put the exception
/// class there; these are reqwest's kinds).
fn error_type(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "reqwest::Error::Timeout"
    } else if error.is_connect() {
        "reqwest::Error::Connect"
    } else if error.is_body() || error.is_decode() {
        "reqwest::Error::Body"
    } else {
        "reqwest::Error::Request"
    }
}

/// An HTTP client with a request timeout, traced and metered per call.
#[derive(Debug, Clone)]
pub struct TracedClient {
    http: reqwest::Client,
    tracer: SdkTracer,
    duration: Histogram,
}

impl TracedClient {
    /// A client giving each request `timeout`.
    ///
    /// # Errors
    /// When the HTTP client can't be built.
    pub fn new(
        timeout: Duration,
        tracer: SdkTracer,
        metrics: &Metrics,
    ) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder().timeout(timeout).build()?,
            tracer,
            duration: metrics.histogram("http.client.request.duration", "s", &LATENCY_BUCKETS),
        })
    }

    /// `GET url`: a CLIENT span if a span is active, the duration always.
    ///
    /// # Errors
    /// Transport failures (connection, timeout, reset).
    pub async fn get(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        let parsed = reqwest::Url::parse(url).ok();
        let host = parsed
            .as_ref()
            .and_then(|u| u.host_str().map(str::to_owned))
            .unwrap_or_default();
        let path = parsed
            .as_ref()
            .map(|u| u.path().to_owned())
            .unwrap_or_default();
        let current = Context::current();
        let traced = current.span().span_context().is_valid();
        let cx = if traced {
            let span = self
                .tracer
                .span_builder(format!("GET {host}").trim().to_owned())
                .with_kind(SpanKind::Client)
                .with_attributes([
                    KeyValue::new("http.request.method", "GET"),
                    KeyValue::new("server.address", host.clone()),
                    KeyValue::new("url.path", path),
                ])
                .start_with_context(&self.tracer, &current);
            Some(current.with_span(span))
        } else {
            None
        };
        let mut headers = HeaderMap::new();
        if let Some(cx) = &cx {
            TraceContextPropagator::new().inject_context(cx, &mut HeaderInjector(&mut headers));
        }
        let start = Instant::now();
        let result = self.http.get(url).headers(headers).send().await;
        let elapsed = start.elapsed().as_secs_f64();
        match &result {
            Ok(response) => {
                let status = response.status().as_u16();
                let code = status.to_string();
                self.duration.record(
                    elapsed,
                    &[
                        ("http.request.method", "GET"),
                        ("server.address", &host),
                        ("http.response.status_code", &code),
                    ],
                );
                if let Some(cx) = &cx {
                    cx.span().set_attribute(KeyValue::new(
                        "http.response.status_code",
                        i64::from(status),
                    ));
                    if response.status().is_server_error() {
                        cx.span().set_status(Status::error(""));
                    }
                }
            }
            Err(error) => {
                self.duration.record(
                    elapsed,
                    &[
                        ("http.request.method", "GET"),
                        ("server.address", &host),
                        ("error.type", error_type(error)),
                    ],
                );
                if let Some(cx) = &cx {
                    cx.span().set_status(Status::error(""));
                }
            }
        }
        if let Some(cx) = cx {
            cx.span().end();
        }
        result
    }
}

/// purerest `Resilience.middleware`: retry (5xx, connection failures and
/// timeouts; never 4xx, a reset mid-response or an open breaker) outside the
/// circuit breaker (5xx and errors count as failures).
#[derive(Debug, Clone)]
pub struct ResilientClient {
    client: TracedClient,
    retry: Retry,
    breaker: CircuitBreaker,
}

impl ResilientClient {
    /// Wraps `client`, the metrics prefixed `purerest`.
    #[must_use]
    pub fn new(
        client: TracedClient,
        retry: RetryConfig,
        breaker: CircuitBreakerConfig,
        metrics: &Metrics,
    ) -> Self {
        Self {
            client,
            retry: Retry::new(retry, metrics, "purerest"),
            breaker: CircuitBreaker::new(breaker, metrics, "purerest"),
        }
    }

    async fn attempt(&self, url: &str) -> Result<reqwest::Response, ClientError> {
        if !self.breaker.acquire() {
            return Err(ClientError::Open);
        }
        let result = self.client.get(url).await;
        self.breaker
            .record(matches!(&result, Ok(response) if !response.status().is_server_error()));
        Ok(result?)
    }

    /// `GET url`, resiliently; an exhausted 5xx is returned as the response.
    ///
    /// # Errors
    /// [`ClientError::Open`] when rejected, [`ClientError::Transport`] when the
    /// last attempt failed on the way.
    pub async fn get(&self, url: &str) -> Result<reqwest::Response, ClientError> {
        self.retry
            .run(
                || self.attempt(url),
                |result| match result {
                    Ok(response) => response.status().is_server_error(),
                    Err(ClientError::Transport(error)) => error.is_connect() || error.is_timeout(),
                    Err(ClientError::Open) => false,
                },
                |result| !matches!(result, Ok(response) if !response.status().is_server_error()),
            )
            .await
    }
}
