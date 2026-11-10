// SPDX-License-Identifier: MIT

//! purerest's client resilience (RUST-6.3 spec FR1-FR3): retry, the
//! count-based circuit breaker, the traced and metered client, and the three
//! composed, against a local server. Sources: purerest `Retry`,
//! `CircuitBreaker`, `Resilience`, `ClientTracing`, `ClientMetrics`; purekafka
//! `Retry` (the generic form).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opentelemetry::Context;
use opentelemetry::context::FutureExt;
use opentelemetry::trace::{SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry_sdk::trace::InMemorySpanExporter;
use telemetry::client::{ClientError, ResilientClient, TracedClient};
use telemetry::metrics::Metrics;
use telemetry::resilience::{CircuitBreaker, CircuitBreakerConfig, Retry, RetryConfig};
use telemetry::tracing::Tracing;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn scrape_value(metrics: &Metrics, series: &str) -> Option<String> {
    metrics
        .render()
        .lines()
        .find(|line| line.starts_with(series))
        .map(|line| line.rsplit(' ').next().unwrap().to_owned())
}

#[tokio::test]
async fn retry_retries_what_it_is_told_with_doubling_delays_and_counts_attempts() {
    let metrics = Metrics::new("s");
    let retry = Retry::new(
        RetryConfig {
            max_retries: 2,
            base_delay: Duration::from_millis(40),
        },
        &metrics,
        "purerest",
    );
    let calls = Arc::new(Mutex::new(0));
    let started = Instant::now();
    let result: Result<u32, String> = retry
        .run(
            || {
                let calls = calls.clone();
                async move {
                    let mut n = calls.lock().unwrap();
                    *n += 1;
                    if *n < 3 {
                        Err(format!("attempt {n}"))
                    } else {
                        Ok(*n)
                    }
                }
            },
            Result::is_err,
            Result::is_err,
        )
        .await;
    assert_eq!(result, Ok(3));
    assert!(
        started.elapsed() >= Duration::from_millis(120),
        "40 + 80 ms of backoff"
    );
    assert_eq!(
        scrape_value(
            &metrics,
            "purerest_retry_attempts_total{otel_scope_name=\"s\",outcome=\"retried\"}"
        )
        .as_deref(),
        Some("2.0")
    );
    assert_eq!(
        scrape_value(
            &metrics,
            "purerest_retry_attempts_total{otel_scope_name=\"s\",outcome=\"succeeded\"}"
        )
        .as_deref(),
        Some("1.0")
    );
}

#[tokio::test]
async fn retry_gives_up_after_max_retries_and_returns_the_last_result() {
    let metrics = Metrics::new("s");
    let retry = Retry::new(
        RetryConfig {
            max_retries: 1,
            base_delay: Duration::ZERO,
        },
        &metrics,
        "purekafka",
    );
    let calls = Arc::new(Mutex::new(0));
    let result: Result<(), u32> = retry
        .run(
            || {
                let calls = calls.clone();
                async move {
                    *calls.lock().unwrap() += 1;
                    Err(*calls.lock().unwrap())
                }
            },
            Result::is_err,
            Result::is_err,
        )
        .await;
    assert_eq!(result, Err(2));
    assert_eq!(
        scrape_value(
            &metrics,
            "purekafka_retry_attempts_total{otel_scope_name=\"s\",outcome=\"exhausted\"}"
        )
        .as_deref(),
        Some("1.0")
    );
}

#[tokio::test]
async fn a_result_not_worth_retrying_is_returned_at_once() {
    let metrics = Metrics::new("s");
    let retry = Retry::new(
        RetryConfig {
            max_retries: 3,
            base_delay: Duration::ZERO,
        },
        &metrics,
        "purerest",
    );
    let calls = Arc::new(Mutex::new(0));
    let result: Result<(), &str> = retry
        .run(
            || {
                let calls = calls.clone();
                async move {
                    *calls.lock().unwrap() += 1;
                    Err("400")
                }
            },
            |_| false,
            Result::is_err,
        )
        .await;
    assert_eq!((result, *calls.lock().unwrap()), (Err("400"), 1));
}

fn breaker(threshold: u32, reset: Duration) -> (Metrics, CircuitBreaker) {
    let metrics = Metrics::new("s");
    let breaker = CircuitBreaker::new(
        CircuitBreakerConfig {
            failure_threshold: threshold,
            reset_timeout: reset,
        },
        &metrics,
        "purerest",
    );
    (metrics, breaker)
}

fn state(metrics: &Metrics) -> String {
    ["CLOSED", "OPEN", "HALF_OPEN"]
        .iter()
        .find(|s| {
            scrape_value(
                metrics,
                &format!("purerest_circuit_breaker_state{{otel_scope_name=\"s\",state=\"{s}\"}}"),
            )
            .as_deref()
                == Some("1.0")
        })
        .map(|s| (*s).to_owned())
        .unwrap_or_default()
}

#[test]
fn the_breaker_opens_when_the_whole_window_fails_and_rejects_until_the_reset_timeout() {
    let (metrics, breaker) = breaker(2, Duration::from_millis(100));
    assert!(breaker.acquire());
    breaker.record(false);
    assert_eq!(state(&metrics), "CLOSED");
    assert!(breaker.acquire());
    breaker.record(false);
    assert!(!breaker.acquire(), "open");
    assert_eq!(state(&metrics), "OPEN");
    assert_eq!(
        scrape_value(&metrics, "purerest_circuit_breaker_calls_rejected_total").as_deref(),
        Some("1.0")
    );
    assert_eq!(
        scrape_value(
            &metrics,
            "purerest_circuit_breaker_state_transitions_total{from_state=\"CLOSED\",otel_scope_name=\"s\",to_state=\"OPEN\"}"
        )
        .as_deref(),
        Some("1.0")
    );
    std::thread::sleep(Duration::from_millis(120));
    assert!(breaker.acquire(), "one half-open trial");
    assert_eq!(state(&metrics), "HALF_OPEN");
    assert!(!breaker.acquire(), "only one trial at a time");
    breaker.record(true);
    assert_eq!(state(&metrics), "CLOSED");
    assert!(breaker.acquire());
}

#[test]
fn a_failed_half_open_trial_opens_it_again() {
    let (metrics, breaker) = breaker(1, Duration::from_millis(50));
    assert!(breaker.acquire());
    breaker.record(false);
    std::thread::sleep(Duration::from_millis(70));
    assert!(breaker.acquire());
    breaker.record(false);
    assert!(!breaker.acquire());
    assert_eq!(state(&metrics), "OPEN");
}

#[test]
fn a_success_in_the_window_keeps_it_closed() {
    let (metrics, breaker) = breaker(2, Duration::from_secs(60));
    for success in [false, true, false] {
        assert!(breaker.acquire());
        breaker.record(success);
    }
    assert!(
        breaker.acquire(),
        "the last two calls weren't both failures"
    );
    assert_eq!(state(&metrics), "CLOSED");
}

#[derive(Clone, Copy, Debug)]
enum Answer {
    Status(u16),
    Slow(u64),
    Reset,
}

/// A local server answering from a script, then 200 `{"email":"e"}`; records
/// each request's head.
async fn server(script: Vec<Answer>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let script = Arc::new(Mutex::new(VecDeque::from(script)));
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let (script, log) = (script.clone(), log.clone());
            tokio::spawn(async move {
                let mut buffer = vec![0; 4096];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                log.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buffer[..read]).into_owned());
                let answer = script.lock().unwrap().pop_front();
                let status = match answer {
                    Some(Answer::Reset) => return,
                    Some(Answer::Slow(ms)) => {
                        tokio::time::sleep(Duration::from_millis(ms)).await;
                        200
                    }
                    Some(Answer::Status(code)) => code,
                    None => 200,
                };
                let body = if status == 200 {
                    "{\"email\":\"e\"}"
                } else {
                    ""
                };
                let response = format!(
                    "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
            });
        }
    });
    (url, seen)
}

#[tokio::test]
async fn the_client_traces_each_call_under_the_active_span_and_meters_it() {
    let exporter = InMemorySpanExporter::default();
    let tracing = Tracing::with_exporter("events-probe", 1.0, exporter.clone());
    let metrics = Metrics::new("events-probe");
    let client = TracedClient::new(Duration::from_secs(5), tracing.tracer(), &metrics).unwrap();
    let (url, seen) = server(vec![Answer::Status(503)]).await;
    let parent = tracing.tracer().start("user-events process");
    let cx = Context::current_with_span(parent);
    let response = client
        .get(&format!("{url}/users/u1"))
        .with_context(cx.clone())
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 503);
    cx.span().end();
    // No active span: no client span (purerest ClientTracing is a passthrough).
    client.get(&format!("{url}/users/u2")).await.unwrap();
    tracing.force_flush();
    let spans = exporter.get_finished_spans().unwrap();
    let client_spans: Vec<_> = spans
        .iter()
        .filter(|s| s.span_kind == SpanKind::Client)
        .collect();
    assert_eq!(client_spans.len(), 1);
    let span = client_spans[0];
    assert_eq!(span.name, "GET 127.0.0.1");
    assert_eq!(span.parent_span_id, cx.span().span_context().span_id());
    assert_eq!(span.status, Status::error(""));
    let mut attributes: Vec<(String, String)> = span
        .attributes
        .iter()
        .map(|kv| (kv.key.to_string(), kv.value.to_string()))
        .collect();
    attributes.sort();
    assert_eq!(
        attributes,
        [
            ("http.request.method".to_owned(), "GET".to_owned()),
            ("http.response.status_code".to_owned(), "503".to_owned()),
            ("server.address".to_owned(), "127.0.0.1".to_owned()),
            ("url.path".to_owned(), "/users/u1".to_owned()),
        ]
    );
    let first = seen.lock().unwrap()[0].to_ascii_lowercase();
    let traceparent = format!(
        "traceparent: 00-{}-{}-01",
        span.span_context.trace_id(),
        span.span_context.span_id()
    );
    assert!(first.contains(&traceparent), "{first}");
    let text = metrics.render();
    assert!(
        text.contains("# TYPE http_client_request_duration_seconds histogram"),
        "{text}"
    );
    assert!(
        text.contains("http_client_request_duration_seconds_count{http_request_method=\"GET\",http_response_status_code=\"503\",otel_scope_name=\"events-probe\",server_address=\"127.0.0.1\"} 1"),
        "{text}"
    );
}

fn resilient(timeout_ms: u64, retries: u32, threshold: u32) -> (Metrics, ResilientClient) {
    let metrics = Metrics::new("events-probe");
    let tracing = Tracing::with_exporter("events-probe", 1.0, InMemorySpanExporter::default());
    let client = ResilientClient::new(
        TracedClient::new(
            Duration::from_millis(timeout_ms),
            tracing.tracer(),
            &metrics,
        )
        .unwrap(),
        RetryConfig {
            max_retries: retries,
            base_delay: Duration::from_millis(5),
        },
        CircuitBreakerConfig {
            failure_threshold: threshold,
            reset_timeout: Duration::from_secs(60),
        },
        &metrics,
    );
    (metrics, client)
}

#[tokio::test]
async fn five_hundreds_and_timeouts_are_retried_until_an_answer() {
    let (_, client) = resilient(200, 3, 10);
    let (url, seen) = server(vec![Answer::Status(503), Answer::Slow(1000)]).await;
    let response = client.get(&format!("{url}/users/u1")).await.unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(seen.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn exhausted_five_hundreds_return_the_last_response() {
    let (metrics, client) = resilient(1000, 1, 10);
    let (url, seen) = server(vec![Answer::Status(500), Answer::Status(502)]).await;
    let response = client.get(&format!("{url}/users/u1")).await.unwrap();
    assert_eq!(response.status().as_u16(), 502);
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(metrics.render().contains(
        "purerest_retry_attempts_total{otel_scope_name=\"events-probe\",outcome=\"exhausted\"} 1.0"
    ));
}

#[tokio::test]
async fn four_hundreds_and_resets_are_not_retried() {
    let (_, client) = resilient(1000, 3, 10);
    let (url, seen) = server(vec![Answer::Status(404), Answer::Reset]).await;
    assert_eq!(
        client
            .get(&format!("{url}/users/u1"))
            .await
            .unwrap()
            .status()
            .as_u16(),
        404
    );
    assert!(matches!(
        client.get(&format!("{url}/users/u1")).await,
        Err(ClientError::Transport(_))
    ));
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn an_open_breaker_fails_fast_without_calling_and_is_not_retried() {
    let (metrics, client) = resilient(1000, 0, 2);
    let (url, seen) = server(vec![Answer::Status(500), Answer::Status(500)]).await;
    for _ in 0..2 {
        assert_eq!(
            client
                .get(&format!("{url}/users/u1"))
                .await
                .unwrap()
                .status()
                .as_u16(),
            500
        );
    }
    assert!(matches!(
        client.get(&format!("{url}/users/u1")).await,
        Err(ClientError::Open)
    ));
    assert_eq!(seen.lock().unwrap().len(), 2);
    assert!(metrics.render().contains(
        "purerest_circuit_breaker_calls_rejected_total{otel_scope_name=\"events-probe\"} 1.0"
    ));
}
