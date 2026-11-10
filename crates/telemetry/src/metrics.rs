// SPDX-License-Identifier: MIT

//! Metrics as purerest exposes them (`Metrics.oteljava`, `ServerMetrics`,
//! `PoolMetrics`, `LatencyBuckets`; RUST-3.3): OTel-style instruments rendered
//! in the Prometheus text format the OTel Java Prometheus exporter writes, so
//! the generated dashboards (gluon ADR 0027) query the same series.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use axum::Router;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_TYPE as CONTENT_TYPE_HEADER;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

use crate::jvm::parse_uuid;
use crate::tracing::Unmatched;

/// purerest `LatencyBuckets.seconds`: 5 ms to 10 s, dense where the SLOs sit.
pub const LATENCY_BUCKETS: [f64; 24] = [
    0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.75, 1.0, 1.25,
    1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.5, 10.0,
];

/// The scrape response's Content-Type (Prometheus text format 0.0.4).
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Attribute pairs as the OTel semantic conventions name them (`http.route`).
pub type Attributes<'a> = &'a [(&'a str, &'a str)];

/// A series' labels: `(name, value)` sorted by name, names in Prometheus form.
type Labels = Vec<(String, String)>;

#[derive(Debug, Clone)]
enum Series {
    Counter(f64),
    Gauge(f64),
    Histogram {
        counts: Vec<u64>,
        count: u64,
        sum: f64,
    },
}

#[derive(Debug)]
struct Family {
    kind: &'static str,
    buckets: Vec<f64>,
    series: BTreeMap<(Vec<String>, Vec<String>), Series>,
}

#[derive(Debug, Default)]
struct Registry {
    families: BTreeMap<String, Family>,
}

/// A meter and its registry: instruments record into it; [`Metrics::render`]
/// writes the scrape. Cloning shares the registry.
#[derive(Debug, Clone)]
pub struct Metrics {
    scope: Arc<str>,
    registry: Arc<Mutex<Registry>>,
}

fn prometheus_label(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn prometheus_name(name: &str, unit: &str, counter: bool) -> String {
    let mut text: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ':' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if unit == "s" && !text.ends_with("_seconds") {
        text.push_str("_seconds");
    }
    if counter && !text.ends_with("_total") {
        text.push_str("_total");
    }
    text
}

impl Metrics {
    /// A meter whose series carry `otel_scope_name = scope` (the service name).
    #[must_use]
    pub fn new(scope: &str) -> Self {
        Self {
            scope: scope.into(),
            registry: Arc::default(),
        }
    }

    fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn register(&self, name: String, kind: &'static str, buckets: &[f64]) -> Instrument {
        self.registry()
            .families
            .entry(name.clone())
            .or_insert_with(|| Family {
                kind,
                buckets: buckets.to_vec(),
                series: BTreeMap::new(),
            });
        Instrument {
            metrics: self.clone(),
            name,
        }
    }

    /// A histogram (`unit` `s` adds `_seconds`) with explicit bucket boundaries.
    #[must_use]
    pub fn histogram(&self, name: &str, unit: &str, buckets: &[f64]) -> Histogram {
        Histogram(self.register(prometheus_name(name, unit, false), "histogram", buckets))
    }

    /// A monotonic counter (exposed with `_total`).
    #[must_use]
    pub fn counter(&self, name: &str) -> Counter {
        Counter(self.register(prometheus_name(name, "", true), "counter", &[]))
    }

    /// A gauge: each series holds the last value set.
    #[must_use]
    pub fn gauge(&self, name: &str) -> Gauge {
        Gauge(self.register(prometheus_name(name, "", false), "gauge", &[]))
    }

    /// An up-down counter (exposed as a gauge).
    #[must_use]
    pub fn up_down_counter(&self, name: &str) -> UpDownCounter {
        UpDownCounter(self.register(prometheus_name(name, "", false), "gauge", &[]))
    }

    /// The scrape: families by name, then series by label names and values,
    /// `# TYPE` lines only; families that never recorded are left out.
    #[must_use]
    pub fn render(&self) -> String {
        let registry = self.registry();
        let mut text = String::new();
        for (name, family) in &registry.families {
            if family.series.is_empty() {
                continue;
            }
            let _ = writeln!(text, "# TYPE {name} {}", family.kind);
            for ((names, values), series) in &family.series {
                let labels: Labels = names.iter().cloned().zip(values.iter().cloned()).collect();
                match series {
                    Series::Counter(value) | Series::Gauge(value) => {
                        let _ = writeln!(
                            text,
                            "{name}{} {}",
                            braces(&labels, None),
                            java_double(*value)
                        );
                    }
                    Series::Histogram { counts, count, sum } => {
                        let mut cumulative = 0;
                        for (bound, bucket) in family.buckets.iter().zip(counts) {
                            cumulative += bucket;
                            let le = java_double(*bound);
                            let _ = writeln!(
                                text,
                                "{name}_bucket{} {cumulative}",
                                braces(&labels, Some(&le))
                            );
                        }
                        let _ = writeln!(
                            text,
                            "{name}_bucket{} {count}",
                            braces(&labels, Some("+Inf"))
                        );
                        let _ = writeln!(text, "{name}_count{} {count}", braces(&labels, None));
                        let _ = writeln!(
                            text,
                            "{name}_sum{} {}",
                            braces(&labels, None),
                            java_double(*sum)
                        );
                    }
                }
            }
        }
        text
    }

    fn update(
        &self,
        name: &str,
        attributes: Attributes<'_>,
        apply: impl FnOnce(&mut Series, &[f64]),
        empty: impl FnOnce(usize) -> Series,
    ) {
        let mut labels: Labels = attributes
            .iter()
            .map(|(key, value)| (prometheus_label(key), (*value).to_owned()))
            .collect();
        labels.push(("otel_scope_name".to_owned(), self.scope.to_string()));
        labels.sort();
        let key: (Vec<String>, Vec<String>) = labels.into_iter().unzip();
        let mut registry = self.registry();
        if let Some(family) = registry.families.get_mut(name) {
            let buckets = family.buckets.clone();
            let series = family
                .series
                .entry(key)
                .or_insert_with(|| empty(buckets.len()));
            apply(series, &buckets);
        }
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn braces(labels: &Labels, le: Option<&str>) -> String {
    let mut parts: Vec<String> = labels
        .iter()
        .map(|(name, value)| format!("{name}=\"{}\"", escape(value)))
        .collect();
    if let Some(bound) = le {
        parts.push(format!("le=\"{bound}\""));
    }
    format!("{{{}}}", parts.join(","))
}

/// `Double.toString`: shortest round-trip digits, at least one decimal,
/// scientific (`1.0E7`, `1.0E-4`) outside [10⁻³, 10⁷).
fn java_double(value: f64) -> String {
    if value == 0.0 {
        return "0.0".to_owned();
    }
    let magnitude = value.abs();
    if (1e-3..1e7).contains(&magnitude) {
        let text = format!("{value}");
        return if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        };
    }
    let text = format!("{value:e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_owned()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exponent}")
}

#[derive(Debug, Clone)]
struct Instrument {
    metrics: Metrics,
    name: String,
}

/// A histogram instrument.
#[derive(Debug, Clone)]
pub struct Histogram(Instrument);

impl Histogram {
    /// Records one measurement.
    pub fn record(&self, value: f64, attributes: Attributes<'_>) {
        self.0.metrics.update(
            &self.0.name,
            attributes,
            |series, buckets| {
                if let Series::Histogram { counts, count, sum } = series {
                    // Upper bounds are inclusive, as in OTel explicit buckets.
                    let index = buckets.iter().position(|bound| value <= *bound);
                    if let Some(slot) = index.and_then(|i| counts.get_mut(i)) {
                        *slot += 1;
                    }
                    *count += 1;
                    *sum += value;
                }
            },
            |n| Series::Histogram {
                counts: vec![0; n],
                count: 0,
                sum: 0.0,
            },
        );
    }
}

/// A monotonic counter instrument.
#[derive(Debug, Clone)]
pub struct Counter(Instrument);

impl Counter {
    /// Adds `n`.
    #[allow(clippy::cast_precision_loss)] // counts stay far below 2^52
    pub fn add(&self, n: u64, attributes: Attributes<'_>) {
        self.0.metrics.update(
            &self.0.name,
            attributes,
            |series, _| {
                if let Series::Counter(value) = series {
                    *value += n as f64;
                }
            },
            |_| Series::Counter(0.0),
        );
    }
}

/// An up-down counter instrument (a gauge in the scrape).
#[derive(Debug, Clone)]
pub struct UpDownCounter(Instrument);

impl UpDownCounter {
    /// Adds `n` (negative to decrement).
    #[allow(clippy::cast_precision_loss)] // gauges stay far below 2^52
    pub fn add(&self, n: i64, attributes: Attributes<'_>) {
        self.0.metrics.update(
            &self.0.name,
            attributes,
            |series, _| {
                if let Series::Gauge(value) = series {
                    *value += n as f64;
                }
            },
            |_| Series::Gauge(0.0),
        );
    }
}

/// A gauge instrument (OTel's synchronous gauge): the last value set wins.
#[derive(Debug, Clone)]
pub struct Gauge(Instrument);

impl Gauge {
    /// Sets the series' value.
    pub fn set(&self, value: f64, attributes: Attributes<'_>) {
        self.0.metrics.update(
            &self.0.name,
            attributes,
            |series, _| {
                if let Series::Gauge(current) = series {
                    *current = value;
                }
            },
            |_| Series::Gauge(0.0),
        );
    }
}

/// purerest `ServerMetrics.routeTemplate`: the raw path, every segment that
/// `java.util.UUID.fromString` accepts replaced by `{id}`.
#[must_use]
pub fn route_template(path: &str) -> String {
    path.split('/')
        .map(|segment| {
            if parse_uuid(segment).is_some() {
                "{id}"
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The `http.server.request.duration` instrument the middleware records into.
#[derive(Debug, Clone)]
pub struct ServerMetrics {
    duration: Histogram,
}

impl ServerMetrics {
    /// Creates (or reuses) the histogram in `metrics`.
    #[must_use]
    pub fn new(metrics: &Metrics) -> Self {
        Self {
            duration: metrics.histogram("http.server.request.duration", "s", &LATENCY_BUCKETS),
        }
    }
}

/// Axum middleware (purerest `ServerMetrics.middleware`): times each request
/// and records it with `http.request.method`, `http.route` and — when a route
/// answered (not [`Unmatched`]) — `http.response.status_code`.
pub async fn server_metrics(
    State(server): State<ServerMetrics>,
    request: Request,
    next: Next,
) -> Response {
    let method = request.method().as_str().to_owned();
    let route = route_template(request.uri().path());
    let start = Instant::now();
    let response = next.run(request).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();
    let mut attributes = vec![
        ("http.request.method", method.as_str()),
        ("http.route", route.as_str()),
    ];
    if response.extensions().get::<Unmatched>().is_none() {
        attributes.push(("http.response.status_code", status.as_str()));
    }
    server.duration.record(elapsed, &attributes);
    response
}

/// The scrape endpoint (`/metrics`, and `/` as the Java exporter also serves).
pub fn router(metrics: Metrics) -> Router {
    let scrape = move || {
        let metrics = metrics.clone();
        async move { ([(CONTENT_TYPE_HEADER, CONTENT_TYPE)], metrics.render()).into_response() }
    };
    Router::new()
        .route("/metrics", get(scrape.clone()))
        .route("/", get(scrape))
}

/// purerest `PoolMetrics`: times connection acquisition and counts callers
/// waiting and connections held, all tagged with the pool's name.
#[derive(Debug, Clone)]
pub struct PoolMetrics {
    pool_name: Arc<str>,
    wait_time: Histogram,
    used: UpDownCounter,
    pending: UpDownCounter,
}

impl PoolMetrics {
    /// The three instruments, tagged `db.client.connection.pool.name = pool_name`.
    #[must_use]
    pub fn new(metrics: &Metrics, pool_name: &str) -> Self {
        Self {
            pool_name: pool_name.into(),
            wait_time: metrics.histogram("db.client.connection.wait_time", "s", &LATENCY_BUCKETS),
            used: metrics.up_down_counter("db.client.connection.count"),
            pending: metrics.up_down_counter("db.client.connection.pending_requests"),
        }
    }

    /// Runs `acquire`: pending while it runs; on success its wait time is
    /// recorded and the connection counts as used until the [`Held`] drops.
    ///
    /// # Errors
    /// Whatever `acquire` fails with (nothing is recorded but pending).
    pub async fn acquire<T, E>(
        &self,
        acquire: impl Future<Output = Result<T, E>>,
    ) -> Result<Held<T>, E> {
        let pool = [("db.client.connection.pool.name", &*self.pool_name)];
        self.pending.add(1, &pool);
        let start = Instant::now();
        let acquired = acquire.await;
        self.pending.add(-1, &pool);
        let connection = acquired?;
        self.wait_time.record(start.elapsed().as_secs_f64(), &pool);
        self.used.add(1, &self.used_attributes());
        Ok(Held {
            value: Some(connection),
            metrics: self.clone(),
        })
    }

    fn used_attributes(&self) -> [(&str, &str); 2] {
        [
            ("db.client.connection.pool.name", &self.pool_name),
            ("db.client.connection.state", "used"),
        ]
    }
}

/// A connection counted as used until dropped.
#[derive(Debug)]
pub struct Held<T> {
    value: Option<T>,
    metrics: PoolMetrics,
}

impl<T> Deref for Held<T> {
    type Target = T;

    fn deref(&self) -> &T {
        // Only `drop` takes the value out.
        self.value.as_ref().unwrap_or_else(|| unreachable!())
    }
}

impl<T> DerefMut for Held<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.value.as_mut().unwrap_or_else(|| unreachable!())
    }
}

impl<T> Drop for Held<T> {
    fn drop(&mut self) {
        // Release the connection first, then stop counting it.
        drop(self.value.take());
        self.metrics.used.add(-1, &self.metrics.used_attributes());
    }
}
