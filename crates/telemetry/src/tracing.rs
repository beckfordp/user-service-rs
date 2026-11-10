// SPDX-License-Identifier: MIT

//! Tracing as purerest's `Tracing`, `TracingConfig` and `ServerTracing` do it
//! (gluon ADR 0021; RUST-3.2): an OpenTelemetry SDK exporting over OTLP/HTTP
//! or to the console, a SERVER span per request that continues an inbound W3C
//! `traceparent`, and the W3C text of a span context for the outbox.

use std::borrow::Cow;
use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex};

use axum::extract::{Request, State};
use axum::http::HeaderMap;
use axum::middleware::Next;
use axum::response::{IntoResponse, IntoResponseParts, Response, ResponseParts};
use opentelemetry::context::FutureExt;
use opentelemetry::propagation::{Extractor, TextMapPropagator};
use opentelemetry::trace::{SpanContext, SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue};
use opentelemetry_otlp::{Protocol, WithExportConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::resource::TelemetryResourceDetector;
pub use opentelemetry_sdk::trace::SdkTracer;
use opentelemetry_sdk::trace::{
    Sampler, SdkTracerProvider, SpanData, SpanExporter, TracerProviderBuilder,
};

/// A tracer provider and the service name its tracer is scoped to.
#[derive(Debug, Clone)]
pub struct Tracing {
    provider: SdkTracerProvider,
    service_name: String,
}

/// The OTLP exporter couldn't be built (e.g. an endpoint that isn't a URI).
#[derive(Debug, thiserror::Error)]
#[error("OTLP trace exporter: {0}")]
pub struct TracingError(String);

impl Tracing {
    /// Spans go over OTLP/HTTP (protobuf, batched) to `<otlp_endpoint>/v1/traces`
    /// when an endpoint is given, else to stderr one line each. Roots are
    /// sampled at `sampling_ratio`; children follow their parent. Spans carry
    /// `service.name = service_name`.
    ///
    /// # Errors
    /// When the OTLP exporter can't be built from the endpoint.
    pub fn new(
        service_name: &str,
        otlp_endpoint: Option<&str>,
        sampling_ratio: f64,
    ) -> Result<Self, TracingError> {
        let builder = provider_builder(service_name, sampling_ratio);
        let builder = match otlp_endpoint {
            Some(endpoint) => {
                let exporter = opentelemetry_otlp::SpanExporter::builder()
                    .with_http()
                    .with_protocol(Protocol::HttpBinary)
                    .with_endpoint(traces_url(endpoint))
                    .build()
                    .map_err(|error| TracingError(error.to_string()))?;
                builder.with_batch_exporter(exporter)
            }
            None => builder.with_simple_exporter(ConsoleExporter::stderr()),
        };
        Ok(Self {
            provider: builder.build(),
            service_name: service_name.to_owned(),
        })
    }

    /// As [`Tracing::new`] with `exporter`, batched (tests pass an in-memory one).
    #[must_use]
    pub fn with_exporter(
        service_name: &str,
        sampling_ratio: f64,
        exporter: impl SpanExporter + 'static,
    ) -> Self {
        Self {
            provider: provider_builder(service_name, sampling_ratio)
                .with_batch_exporter(exporter)
                .build(),
            service_name: service_name.to_owned(),
        }
    }

    /// The tracer, its instrumentation scope named after the service.
    #[must_use]
    pub fn tracer(&self) -> SdkTracer {
        use opentelemetry::trace::TracerProvider as _;
        self.provider.tracer(self.service_name.clone())
    }

    /// Exports every finished span now.
    pub fn force_flush(&self) {
        let _ = self.provider.force_flush();
    }

    /// Flushes and stops exporting; later calls do nothing.
    pub fn shutdown(&self) {
        let _ = self.provider.shutdown();
    }
}

fn provider_builder(service_name: &str, sampling_ratio: f64) -> TracerProviderBuilder {
    let resource = Resource::builder_empty()
        .with_detector(Box::new(TelemetryResourceDetector))
        .with_service_name(service_name.to_owned())
        .build();
    SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            sampling_ratio,
        ))))
}

/// `endpoint` as the OTLP/HTTP traces URL: like `OTEL_EXPORTER_OTLP_ENDPOINT`
/// it is a base URL, so `/v1/traces` is appended unless already there.
#[must_use]
pub fn traces_url(endpoint: &str) -> String {
    let base = endpoint.trim_end_matches('/');
    if base.ends_with("/v1/traces") {
        base.to_owned()
    } else {
        format!("{base}/v1/traces")
    }
}

/// The W3C `traceparent` text of `context` (version 00).
#[must_use]
pub fn traceparent(context: &SpanContext) -> String {
    format!(
        "00-{}-{}-{:02x}",
        context.trace_id(),
        context.span_id(),
        context.trace_flags().to_u8()
    )
}

/// Marks a response as the fallback's (no route matched): its span gets no
/// `http.response.status_code`, as in purerest, where that 404 comes from
/// outside the traced routes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unmatched;

impl IntoResponseParts for Unmatched {
    type Error = std::convert::Infallible;

    fn into_response_parts(self, mut parts: ResponseParts) -> Result<ResponseParts, Self::Error> {
        parts.extensions_mut().insert(self);
        Ok(parts)
    }
}

impl IntoResponse for Unmatched {
    fn into_response(self) -> Response {
        (self, ()).into_response()
    }
}

struct Headers<'a>(&'a HeaderMap);

impl Extractor for Headers<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(axum::http::HeaderName::as_str).collect()
    }
}

/// Axum middleware: a SERVER span per request named `<METHOD> <path as sent>`,
/// continuing an inbound W3C trace context or else a new root, with
/// `http.request.method`, `url.path` and `http.response.status_code`; a 5xx
/// sets status ERROR. The span is the current context while the handler runs.
pub async fn server_tracing(
    State(tracer): State<SdkTracer>,
    request: Request,
    next: Next,
) -> Response {
    let parent = TraceContextPropagator::new().extract(&Headers(request.headers()));
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    let span = tracer
        .span_builder(format!("{method} {path}"))
        .with_kind(SpanKind::Server)
        .with_attributes([
            KeyValue::new("http.request.method", method),
            KeyValue::new("url.path", path),
        ])
        .start_with_context(&tracer, &parent);
    let cx = parent.with_span(span);
    let response = next.run(request).with_context(cx.clone()).await;
    let span = cx.span();
    if response.extensions().get::<Unmatched>().is_none() {
        span.set_attribute(KeyValue::new(
            "http.response.status_code",
            i64::from(response.status().as_u16()),
        ));
        if response.status().is_server_error() {
            span.set_status(Status::error(""));
        }
    }
    span.end();
    response
}

/// Starts an INTERNAL span, a child of the current context, with `attributes`.
#[must_use]
pub fn internal_span(
    tracer: &SdkTracer,
    name: impl Into<Cow<'static, str>>,
    attributes: Vec<KeyValue>,
) -> Context {
    let span = tracer
        .span_builder(name)
        .with_kind(SpanKind::Internal)
        .with_attributes(attributes)
        .start_with_context(tracer, &Context::current());
    Context::current_with_span(span)
}

/// Writes each span as one line, in the Java `LoggingSpanExporter`'s format:
/// `'<name>' : <trace> <span> <KIND> [tracer: <scope>:<version>] AttributesMap{…}`.
/// Used when no OTLP endpoint is configured, as purerest falls back to the
/// console.
#[derive(Clone)]
pub struct ConsoleExporter {
    writer: Arc<Mutex<dyn Write + Send>>,
}

impl fmt::Debug for ConsoleExporter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ConsoleExporter")
    }
}

impl ConsoleExporter {
    /// Lines go to stderr.
    #[must_use]
    pub fn stderr() -> Self {
        Self {
            writer: Arc::new(Mutex::new(std::io::stderr())),
        }
    }

    /// Lines go to `writer` (tests pass a buffer).
    #[must_use]
    pub fn to_writer<W: Write + Send + 'static>(writer: Arc<Mutex<W>>) -> Self {
        Self { writer }
    }
}

fn kind_name(kind: &SpanKind) -> &'static str {
    match kind {
        SpanKind::Client => "CLIENT",
        SpanKind::Server => "SERVER",
        SpanKind::Producer => "PRODUCER",
        SpanKind::Consumer => "CONSUMER",
        SpanKind::Internal => "INTERNAL",
    }
}

fn console_line(span: &SpanData) -> String {
    let data: Vec<String> = span
        .attributes
        .iter()
        .map(|kv| format!("{}={}", kv.key, kv.value))
        .collect();
    format!(
        "'{}' : {} {} {} [tracer: {}:{}] AttributesMap{{data={{{}}}, capacity=128, totalAddedValues={}}}\n",
        span.name,
        span.span_context.trace_id(),
        span.span_context.span_id(),
        kind_name(&span.span_kind),
        span.instrumentation_scope.name(),
        span.instrumentation_scope.version().unwrap_or_default(),
        data.join(", "),
        span.attributes.len()
    )
}

impl SpanExporter for ConsoleExporter {
    fn export(&self, batch: Vec<SpanData>) -> impl Future<Output = OTelSdkResult> + Send {
        let text: String = batch.iter().map(console_line).collect();
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(text.as_bytes());
            let _ = writer.flush();
        }
        std::future::ready(Ok(()))
    }
}
