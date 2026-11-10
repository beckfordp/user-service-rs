// SPDX-License-Identifier: MIT

//! The consumer (purekafka's consumer side as notification-service uses it):
//! rdkafka, one CONSUMER span per record joining its `traceparent`
//! (`KafkaTracing.traced`), event age per record (`EventAge`), and lag from
//! librdkafka's own statistics (`ConsumerLag` reads the Java client's).

use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry::trace::{SpanKind, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use rdkafka::ClientContext;
use rdkafka::consumer::ConsumerContext;
use serde_json::Value;
use telemetry::metrics::{Gauge, Histogram, Metrics};
use telemetry::tracing::SdkTracer;

/// purekafka `EventAge.boundaries`: 10 ms to 10 min.
pub const EVENT_AGE_BUCKETS: [f64; 15] = [
    0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0, 600.0,
];

/// Seconds between the record's timestamp and now, clamped at zero.
#[must_use]
#[allow(clippy::cast_precision_loss)] // milliseconds, far below 2^52
pub fn event_age_seconds(now_ms: i64, record_ms: i64) -> f64 {
    (now_ms - record_ms).max(0) as f64 / 1000.0
}

/// `(topic, partition, lag)` for every assigned partition in librdkafka's
/// statistics JSON (`consumer_lag`, -1 when unknown, is skipped).
#[must_use]
pub fn lag_from_statistics(json: &str) -> Vec<(String, i32, i64)> {
    let Ok(statistics) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut lag = Vec::new();
    for (topic, details) in statistics["topics"].as_object().into_iter().flatten() {
        for partition in details["partitions"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(_, p)| p)
        {
            let (Some(id), Some(behind)) = (
                partition["partition"].as_i64(),
                partition["consumer_lag"].as_i64(),
            ) else {
                continue;
            };
            if id >= 0 && behind >= 0 {
                lag.push((topic.clone(), i32::try_from(id).unwrap_or(i32::MAX), behind));
            }
        }
    }
    lag
}

struct Headers<'a>(&'a [(String, String)]);

impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| k.as_str()).collect()
    }
}

/// A CONSUMER span `<topic> process`, joining the record's W3C context if it
/// carries one, else a new root.
#[must_use]
pub fn consumer_span(tracer: &SdkTracer, headers: &[(String, String)], topic: &str) -> Context {
    let parent = TraceContextPropagator::new().extract(&Headers(headers));
    let span = tracer
        .span_builder(format!("{topic} process"))
        .with_kind(SpanKind::Consumer)
        .with_attributes([
            KeyValue::new("messaging.system", "kafka"),
            KeyValue::new("messaging.destination.name", topic.to_owned()),
        ])
        .start_with_context(tracer, &parent);
    parent.with_span(span)
}

/// The consumer metrics purekafka exports.
#[derive(Debug, Clone)]
pub struct ConsumerMetrics {
    event_age: Histogram,
    lag: Gauge,
    group: String,
}

impl ConsumerMetrics {
    /// The instruments, for consumer group `group`.
    #[must_use]
    pub fn new(metrics: &Metrics, group: &str) -> Self {
        Self {
            event_age: metrics.histogram("purekafka.consumer.event.age", "s", &EVENT_AGE_BUCKETS),
            lag: metrics.gauge("purekafka.consumer.lag"),
            group: group.to_owned(),
        }
    }

    /// Records one record's age.
    pub fn record_age(&self, topic: &str, now_ms: i64, record_ms: i64) {
        self.event_age.record(
            event_age_seconds(now_ms, record_ms),
            &[
                ("messaging.destination.name", topic),
                ("messaging.consumer.group.name", &self.group),
            ],
        );
    }

    /// Sets the lag gauges from librdkafka's statistics.
    #[allow(clippy::cast_precision_loss)] // record counts
    pub fn record_lag(&self, statistics: &str) {
        for (topic, partition, lag) in lag_from_statistics(statistics) {
            let partition = partition.to_string();
            self.lag.set(
                lag as f64,
                &[
                    ("messaging.destination.name", &topic),
                    ("messaging.kafka.destination.partition", &partition),
                    ("messaging.consumer.group.name", &self.group),
                ],
            );
        }
    }
}

/// librdkafka's client context: statistics become the lag gauge.
pub struct ProbeContext {
    /// Where the lag goes.
    pub metrics: ConsumerMetrics,
}

impl ClientContext for ProbeContext {
    fn stats_raw(&self, statistics: &[u8]) {
        self.metrics
            .record_lag(&String::from_utf8_lossy(statistics));
    }
}

impl ConsumerContext for ProbeContext {}
