// SPDX-License-Identifier: MIT

//! purekafka's `DeadLetter`: processing retried with bounded backoff; when
//! the retries run out, the record's own key and value go to `<topic>-dlq`
//! (that publish guarded by purekafka's `Resilience`), are counted in
//! `purekafka.dlq.published`, and the stream carries on. A failed publish
//! fails the guard, so the supervisor restarts the stream.

use std::future::Future;

use telemetry::logging::Logger;
use telemetry::metrics::{Counter, Metrics};
use telemetry::resilience::{CircuitBreaker, CircuitBreakerConfig, Retry, RetryConfig};

/// Where dead letters go (a Kafka producer; a fake in tests).
pub trait Publish {
    /// Publishes `value` with `key` to `topic`.
    fn publish(
        &self,
        topic: &str,
        key: Option<&str>,
        value: &str,
    ) -> impl Future<Output = Result<(), String>> + Send;
}

/// The guard.
pub struct DeadLetter<P> {
    processing: Retry,
    publish_retry: Retry,
    publish_breaker: CircuitBreaker,
    publisher: P,
    published: Counter,
    logger: Logger,
}

impl<P: Publish> DeadLetter<P> {
    /// `dlq_retry` for processing; `publish_retry` and `publish_breaker` for
    /// the DLQ publish (notification-service's `dlq-publish-resilience`).
    #[must_use]
    pub fn new(
        dlq_retry: RetryConfig,
        publish_retry: RetryConfig,
        publish_breaker: CircuitBreakerConfig,
        publisher: P,
        metrics: &Metrics,
        logger: Logger,
    ) -> Self {
        Self {
            processing: Retry::new(dlq_retry, metrics, "purekafka"),
            publish_retry: Retry::new(publish_retry, metrics, "purekafka"),
            publish_breaker: CircuitBreaker::new(publish_breaker, metrics, "purekafka"),
            publisher,
            published: metrics.counter("purekafka.dlq.published"),
            logger,
        }
    }

    async fn publish_once(
        &self,
        topic: &str,
        key: Option<&str>,
        value: &str,
    ) -> Result<(), PublishError> {
        if !self.publish_breaker.acquire() {
            return Err(PublishError::Open);
        }
        let result = self.publisher.publish(topic, key, value).await;
        self.publish_breaker.record(result.is_ok());
        result.map_err(PublishError::Failed)
    }

    /// Runs `process` (again, up to the retries); routes `(key, value)` to
    /// `<topic>-dlq` if it never succeeds.
    ///
    /// # Errors
    /// Only when the DLQ publish itself fails.
    pub async fn guard<F, Fut>(
        &self,
        topic: &str,
        key: Option<&str>,
        value: &str,
        process: F,
    ) -> Result<(), String>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let Err(error) = self
            .processing
            .run(process, Result::is_err, Result::is_err)
            .await
        else {
            return Ok(());
        };
        let dlq_topic = format!("{topic}-dlq");
        self.publish_retry
            .run(
                || self.publish_once(&dlq_topic, key, value),
                |result| matches!(result, Err(PublishError::Failed(_))),
                Result::is_err,
            )
            .await
            .map_err(|failure| failure.to_string())?;
        self.published.add(1, &[]);
        self.logger.error(
            &[
                ("topic", topic),
                ("dlq_topic", &dlq_topic),
                ("key", key.unwrap_or("")),
                ("error", &error),
            ],
            "Routed unprocessable message to DLQ after exhausting retries",
        );
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
enum PublishError {
    #[error("Circuit breaker is open")]
    Open,
    #[error("{0}")]
    Failed(String),
}
