// SPDX-License-Identifier: MIT

//! The consume loop: each record timed for age, handled inside its consumer
//! span under the DLQ guard, then its offset committed (at least once, as
//! fs2-kafka's `commit` after `handle` in notification-service). The loop
//! fails when every broker is gone or a commit fails; `main` supervises it.

use std::time::{SystemTime, UNIX_EPOCH};

use opentelemetry::context::FutureExt;
use opentelemetry::trace::TraceContextExt;
use std::time::Duration;

use rdkafka::ClientConfig;
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::message::{BorrowedMessage, Headers, Message, OwnedHeaders};
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::types::RDKafkaErrorCode;
use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use telemetry::tracing::SdkTracer;
use tokio::sync::watch;

use crate::config::ProbeConfig;
use crate::dead_letter::{DeadLetter, Publish};
use crate::handler::{Failure, Handled, Lookup, event_type, handle};
use crate::kafka::{ConsumerMetrics, ProbeContext, consumer_span};

/// Dead letters go to Kafka through this producer.
#[derive(Clone)]
pub struct KafkaPublisher {
    producer: FutureProducer,
}

impl KafkaPublisher {
    /// A producer on `bootstrap`.
    ///
    /// # Errors
    /// When librdkafka rejects the configuration.
    pub fn new(bootstrap: &str) -> Result<Self, KafkaError> {
        let producer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap)
            .set("message.timeout.ms", "10000")
            .create()?;
        Ok(Self { producer })
    }
}

impl Publish for KafkaPublisher {
    async fn publish(&self, topic: &str, key: Option<&str>, value: &str) -> Result<(), String> {
        let mut record = FutureRecord::to(topic)
            .payload(value)
            .headers(OwnedHeaders::new());
        if let Some(key) = key {
            record = record.key(key);
        }
        self.producer
            .send(record, Duration::from_secs(10))
            .await
            .map(|_| ())
            .map_err(|(error, _)| error.to_string())
    }
}

/// What the loop needs.
pub struct Probe<L, P> {
    /// The settings.
    pub config: ProbeConfig,
    /// Spans.
    pub tracer: SdkTracer,
    /// JSON log lines.
    pub logger: Logger,
    /// Where metrics go.
    pub metrics: Metrics,
    /// The recipient lookup.
    pub lookup: L,
    /// The DLQ guard.
    pub dead_letter: DeadLetter<P>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// A consumer subscribed to the probe's topic.
///
/// # Errors
/// When librdkafka rejects the configuration or the subscription.
pub fn consumer(
    config: &ProbeConfig,
    metrics: &Metrics,
) -> Result<StreamConsumer<ProbeContext>, KafkaError> {
    let context = ProbeContext {
        metrics: ConsumerMetrics::new(metrics, &config.group_id),
    };
    let consumer: StreamConsumer<ProbeContext> = ClientConfig::new()
        .set("bootstrap.servers", &config.bootstrap)
        .set("group.id", &config.group_id)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .set("allow.auto.create.topics", "true")
        .set(
            "statistics.interval.ms",
            config.lag_interval.as_millis().to_string(),
        )
        .create_with_context(context)?;
    consumer.subscribe(&[&config.topic])?;
    Ok(consumer)
}

fn headers(message: &BorrowedMessage<'_>) -> Vec<(String, String)> {
    message
        .headers()
        .map(|headers| {
            headers
                .iter()
                .filter_map(|h| {
                    Some((h.key.to_owned(), String::from_utf8(h.value?.to_vec()).ok()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

impl<L: Lookup + Sync, P: Publish + Sync> Probe<L, P> {
    /// Handles one record inside its consumer span, under the DLQ guard
    /// (`EnvelopeHandler`: a null value is skipped; anything that fails, poison
    /// included, is retried and then dead-lettered raw).
    ///
    /// # Errors
    /// Only when dead-lettering failed too: the stream must restart.
    pub async fn process(
        &self,
        topic: &str,
        key: Option<&str>,
        value: Option<&str>,
        headers: &[(String, String)],
    ) -> Result<(), String> {
        let cx = consumer_span(&self.tracer, headers, topic);
        let result = async {
            let Some(raw) = value else {
                self.log(topic, None, &Ok(Handled::Skipped));
                return Ok(());
            };
            self.dead_letter
                .guard(topic, key, raw, || async {
                    let outcome = handle(Some(raw), &self.lookup).await;
                    self.log(topic, Some(raw), &outcome);
                    outcome.map(|_| ()).map_err(|failure| match failure {
                        Failure::Poison(error) | Failure::Lookup(error) => error,
                    })
                })
                .await
        }
        .with_context(cx.clone())
        .await;
        cx.span().end();
        result
    }

    fn log(&self, topic: &str, value: Option<&str>, outcome: &Result<Handled, Failure>) {
        match (outcome, value) {
            (Ok(Handled::Skipped), None) => self.logger.error(
                &[("topic", topic)],
                &format!("Received {topic} record with a null value - skipping"),
            ),
            (Ok(Handled::Skipped), Some(raw)) => {
                let kind = event_type(raw).unwrap_or_default();
                self.logger.info(
                    &[("topic", topic), ("event_type", &kind)],
                    &format!("Ignoring unrelated {topic} event type"),
                );
            }
            (Ok(Handled::Resolved), _) => {
                self.logger.info(&[("topic", topic)], "Recipient resolved");
            }
            (Ok(Handled::Unresolved), _) => self
                .logger
                .warn(&[("topic", topic)], "Recipient unresolved"),
            (Err(Failure::Poison(error)), raw) => {
                let bytes = raw.map_or(0, str::len).to_string();
                self.logger.error(
                    &[("topic", topic), ("bytes", &bytes), ("error", error)],
                    &format!("Failed to decode {topic} envelope"),
                );
            }
            (Err(Failure::Lookup(error)), _) => {
                self.logger.error(
                    &[("topic", topic), ("error", error)],
                    "Recipient lookup failed",
                );
            }
        }
    }

    /// Consumes until `stop` says so: each record's age recorded, the record
    /// handled, its offset committed.
    ///
    /// # Errors
    /// The stream's failure: every broker gone, a failed commit, or a record
    /// that couldn't even be dead-lettered.
    pub async fn consume(&self, mut stop: watch::Receiver<bool>) -> Result<(), String> {
        let consumer = consumer(&self.config, &self.metrics).map_err(|error| error.to_string())?;
        let ages = ConsumerMetrics::new(&self.metrics, &self.config.group_id);
        loop {
            let message = tokio::select! {
                _ = stop.wait_for(|stop| *stop) => return Ok(()),
                message = consumer.recv() => message,
            };
            let message = match message {
                Ok(message) => message,
                Err(error)
                    if error.rdkafka_error_code() == Some(RDKafkaErrorCode::AllBrokersDown) =>
                {
                    return Err(error.to_string());
                }
                Err(error) => {
                    self.logger
                        .warn(&[("error", &error.to_string())], "Consumer error");
                    continue;
                }
            };
            let topic = message.topic().to_owned();
            if let Some(timestamp) = message.timestamp().to_millis() {
                ages.record_age(&topic, now_ms(), timestamp);
            }
            let key = message
                .key()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
            let value = message
                .payload()
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
            self.process(&topic, key.as_deref(), value.as_deref(), &headers(&message))
                .await?;
            consumer
                .commit_message(&message, CommitMode::Sync)
                .map_err(|error| format!("commit failed: {error}"))?;
        }
    }
}
