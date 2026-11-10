// SPDX-License-Identifier: MIT

//! purekafka's `DeadLetter` and `StreamSupervisor` as the probe has them
//! (RUST-6.4 spec FR1-FR3), without a broker: a fake publisher and a fake
//! stream. Sources: purekafka `DeadLetter`, `StreamSupervisor`, `Resilience`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unused_async_trait_impl
)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use events_probe::dead_letter::{DeadLetter, Publish};
use events_probe::supervisor::{next_delay, supervise};
use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use telemetry::resilience::{CircuitBreakerConfig, RetryConfig};
use tokio::sync::watch;

/// A dead letter: topic, key, value.
type Letter = (String, Option<String>, String);

#[derive(Clone, Default)]
struct Published {
    records: Arc<Mutex<Vec<Letter>>>,
    fail: Arc<Mutex<bool>>,
}

impl Publish for Published {
    async fn publish(&self, topic: &str, key: Option<&str>, value: &str) -> Result<(), String> {
        if *self.fail.lock().unwrap() {
            return Err("broker gone".to_owned());
        }
        self.records.lock().unwrap().push((
            topic.to_owned(),
            key.map(ToOwned::to_owned),
            value.to_owned(),
        ));
        Ok(())
    }
}

fn guard(
    max_retries: u32,
) -> (
    Metrics,
    Published,
    Arc<Mutex<Vec<u8>>>,
    DeadLetter<Published>,
) {
    let metrics = Metrics::new("events-probe");
    let publisher = Published::default();
    let log: Arc<Mutex<Vec<u8>>> = Arc::default();
    let guard = DeadLetter::new(
        RetryConfig {
            max_retries,
            base_delay: Duration::from_millis(1),
        },
        RetryConfig {
            max_retries: 1,
            base_delay: Duration::from_millis(1),
        },
        CircuitBreakerConfig {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
        },
        publisher.clone(),
        &metrics,
        Logger::to_writer("events-probe", log.clone()),
    );
    (metrics, publisher, log, guard)
}

fn attempts(
    counter: &Arc<Mutex<u32>>,
    fail_first: u32,
) -> impl FnMut() -> std::future::Ready<Result<(), String>> {
    let counter = counter.clone();
    move || {
        let mut n = counter.lock().unwrap();
        *n += 1;
        std::future::ready(if *n <= fail_first {
            Err(format!("attempt {n}"))
        } else {
            Ok(())
        })
    }
}

#[tokio::test]
async fn processing_is_retried_and_a_late_success_publishes_nothing() {
    let (_, publisher, _, guard) = guard(2);
    let calls = Arc::new(Mutex::new(0));
    guard
        .guard("user-events", Some("k"), "v", attempts(&calls, 2))
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), 3);
    assert!(publisher.records.lock().unwrap().is_empty());
}

#[tokio::test]
async fn exhaustion_publishes_key_and_value_to_the_dlq_topic_counts_and_logs() {
    let (metrics, publisher, log, guard) = guard(1);
    let calls = Arc::new(Mutex::new(0));
    guard
        .guard("user-events", Some("k1"), "{raw", attempts(&calls, 99))
        .await
        .unwrap();
    assert_eq!(*calls.lock().unwrap(), 2);
    assert_eq!(
        *publisher.records.lock().unwrap(),
        [(
            "user-events-dlq".to_owned(),
            Some("k1".to_owned()),
            "{raw".to_owned()
        )]
    );
    assert!(
        metrics
            .render()
            .contains("purekafka_dlq_published_total{otel_scope_name=\"events-probe\"} 1.0")
    );
    assert!(metrics.render().contains(
        "purekafka_retry_attempts_total{otel_scope_name=\"events-probe\",outcome=\"exhausted\"}"
    ));
    let text = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains(
            "\"message\":\"Routed unprocessable message to DLQ after exhausting retries\""
        ),
        "{text}"
    );
    assert!(text.contains("\"dlq_topic\":\"user-events-dlq\""), "{text}");
}

#[tokio::test]
async fn a_failed_dlq_publish_fails_the_guard() {
    let (_, publisher, _, guard) = guard(0);
    *publisher.fail.lock().unwrap() = true;
    let calls = Arc::new(Mutex::new(0));
    let result = guard
        .guard("user-events", None, "v", attempts(&calls, 99))
        .await;
    assert!(result.is_err());
}

#[test]
fn the_restart_delay_doubles_up_to_the_cap() {
    let cap = Duration::from_secs(30);
    assert_eq!(
        next_delay(Duration::from_secs(1), cap),
        Duration::from_secs(2)
    );
    assert_eq!(next_delay(Duration::from_secs(20), cap), cap);
}

#[tokio::test]
async fn a_failed_stream_is_rebuilt_with_backoff_and_each_restart_counted() {
    let metrics = Metrics::new("events-probe");
    let log: Arc<Mutex<Vec<u8>>> = Arc::default();
    let (stop, stopped) = watch::channel(false);
    let builds = Arc::new(Mutex::new(0));
    let started = Instant::now();
    let counter = builds.clone();
    supervise(
        Duration::from_millis(20),
        Duration::from_millis(40),
        &metrics,
        &Logger::to_writer("events-probe", log.clone()),
        stopped,
        || {
            let counter = counter.clone();
            let stop = stop.clone();
            async move {
                let n = {
                    let mut n = counter.lock().unwrap();
                    *n += 1;
                    *n
                };
                if n < 4 {
                    Err(format!("build {n} failed"))
                } else {
                    let _ = stop.send(true);
                    Ok(())
                }
            }
        },
    )
    .await;
    assert_eq!(*builds.lock().unwrap(), 4);
    assert!(
        started.elapsed() >= Duration::from_millis(20 + 40 + 40),
        "20, 40, then capped at 40"
    );
    assert!(
        metrics
            .render()
            .contains("purekafka_stream_restarts_total{otel_scope_name=\"events-probe\"} 3.0")
    );
    let text = String::from_utf8(log.lock().unwrap().clone()).unwrap();
    assert!(
        text.contains("Consumer stream failed (attempt 1), restarting after 20ms"),
        "{text}"
    );
}
