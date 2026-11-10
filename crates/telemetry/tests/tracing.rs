// SPDX-License-Identifier: MIT

//! Server tracing as purerest's `Tracing`, `TracingConfig` and `ServerTracing`
//! behave (RUST-3.2 spec FR1-FR3), against an in-memory exporter. Sources:
//! purerest tracing sources; capture 20261109 stdout.log.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use opentelemetry::trace::{Span as _, SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue, Value};
use opentelemetry_sdk::trace::{InMemorySpanExporter, SpanData};
use telemetry::tracing::{
    ConsoleExporter, Tracing, Unmatched, server_tracing, traceparent, traces_url,
};
use tower::ServiceExt;

const INBOUND_TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const INBOUND_SPAN: &str = "00f067aa0ba902b7";

fn inbound(sampled: bool) -> String {
    format!(
        "00-{INBOUND_TRACE}-{INBOUND_SPAN}-{}",
        if sampled { "01" } else { "00" }
    )
}

struct Fixture {
    tracing: Tracing,
    exporter: InMemorySpanExporter,
    app: Router,
}

#[allow(clippy::unused_async)] // an axum handler
async fn child(tracer: opentelemetry_sdk::trace::SdkTracer) -> StatusCode {
    let span = tracer
        .span_builder("outbox.write")
        .with_kind(SpanKind::Internal)
        .start_with_context(&tracer, &Context::current());
    drop(span);
    StatusCode::CREATED
}

fn fixture(ratio: f64) -> Fixture {
    let exporter = InMemorySpanExporter::default();
    let tracing = Tracing::with_exporter("user-service", ratio, exporter.clone());
    let tracer = tracing.tracer();
    let child_tracer = tracer.clone();
    let app = Router::new()
        .route("/ok", get(|| async { StatusCode::OK }))
        .route("/teapot", get(|| async { StatusCode::IM_A_TEAPOT }))
        .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
        .route(
            "/unavailable",
            get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        )
        .route("/child", get(move || child(child_tracer.clone())))
        .route("/users/{id}", get(|| async { StatusCode::OK }))
        .fallback(|| async { (StatusCode::NOT_FOUND, Unmatched).into_response() })
        .layer(axum::middleware::from_fn_with_state(tracer, server_tracing));
    Fixture {
        tracing,
        exporter,
        app,
    }
}

impl Fixture {
    async fn call(&self, method: &str, uri: &str, traceparent: Option<&str>) -> StatusCode {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(value) = traceparent {
            request = request.header("traceparent", value);
        }
        let response = self
            .app
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        response.status()
    }

    fn spans(&self) -> Vec<SpanData> {
        self.tracing.force_flush();
        self.exporter.get_finished_spans().unwrap()
    }

    fn only_span(&self) -> SpanData {
        let spans = self.spans();
        assert_eq!(spans.len(), 1, "{spans:#?}");
        spans.into_iter().next().unwrap()
    }
}

fn attributes(span: &SpanData) -> Vec<(String, Value)> {
    let mut pairs: Vec<(String, Value)> = span
        .attributes
        .iter()
        .map(|KeyValue { key, value, .. }| (key.to_string(), value.clone()))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

fn http_attributes(method: &str, path: &str, status: Option<i64>) -> Vec<(String, Value)> {
    let mut pairs = vec![
        (
            "http.request.method".to_owned(),
            Value::from(method.to_owned()),
        ),
        ("url.path".to_owned(), Value::from(path.to_owned())),
    ];
    if let Some(code) = status {
        pairs.push(("http.response.status_code".to_owned(), Value::I64(code)));
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

#[test]
fn the_traces_url_appends_the_signal_path_once() {
    assert_eq!(
        traces_url("http://tempo:4318"),
        "http://tempo:4318/v1/traces"
    );
    assert_eq!(
        traces_url("http://tempo:4318/"),
        "http://tempo:4318/v1/traces"
    );
    assert_eq!(
        traces_url("http://tempo:4318/v1/traces"),
        "http://tempo:4318/v1/traces"
    );
    assert_eq!(
        traces_url("http://tempo:4318/v1/traces/"),
        "http://tempo:4318/v1/traces"
    );
}

#[tokio::test]
async fn a_request_gets_a_server_span_named_by_method_and_raw_path() {
    let fixture = fixture(1.0);
    assert_eq!(fixture.call("GET", "/ok", None).await, StatusCode::OK);
    let span = fixture.only_span();
    assert_eq!(span.name, "GET /ok");
    assert_eq!(span.span_kind, SpanKind::Server);
    assert_eq!(attributes(&span), http_attributes("GET", "/ok", Some(200)));
    assert_eq!(span.status, Status::Unset);
    assert_eq!(span.instrumentation_scope.name(), "user-service");
    assert!(span.span_context.is_sampled());
}

#[tokio::test]
async fn the_span_name_keeps_the_path_as_sent() {
    let fixture = fixture(1.0);
    let path = "/users/691B03E8-F1D7-45D9-A561-5183FFAE9D0B";
    fixture.call("GET", path, None).await;
    let span = fixture.only_span();
    assert_eq!(span.name, format!("GET {path}"));
    assert_eq!(attributes(&span), http_attributes("GET", path, Some(200)));
}

#[tokio::test]
async fn an_inbound_traceparent_is_continued() {
    let fixture = fixture(1.0);
    fixture.call("GET", "/ok", Some(&inbound(true))).await;
    let span = fixture.only_span();
    assert_eq!(span.span_context.trace_id().to_string(), INBOUND_TRACE);
    assert_eq!(span.parent_span_id.to_string(), INBOUND_SPAN);
    assert!(span.parent_span_is_remote);
}

#[tokio::test]
async fn without_a_traceparent_the_span_is_a_root() {
    let fixture = fixture(1.0);
    fixture.call("GET", "/ok", None).await;
    let span = fixture.only_span();
    assert_eq!(span.parent_span_id, opentelemetry::trace::SpanId::INVALID);
    assert_ne!(span.span_context.trace_id().to_string(), INBOUND_TRACE);
}

#[tokio::test]
async fn a_malformed_traceparent_starts_a_new_root() {
    let fixture = fixture(1.0);
    fixture.call("GET", "/ok", Some("00-nonsense-01")).await;
    let span = fixture.only_span();
    assert_eq!(span.parent_span_id, opentelemetry::trace::SpanId::INVALID);
}

#[tokio::test]
async fn only_a_server_error_marks_the_span_as_error() {
    let fixture = fixture(1.0);
    fixture.call("GET", "/teapot", None).await;
    fixture.call("GET", "/boom", None).await;
    fixture.call("GET", "/unavailable", None).await;
    let mut spans = fixture.spans();
    spans.sort_by(|a, b| a.name.cmp(&b.name));
    let statuses: Vec<(&str, &Status)> = spans.iter().map(|s| (&*s.name, &s.status)).collect();
    assert_eq!(
        statuses,
        [
            ("GET /boom", &Status::error("")),
            ("GET /teapot", &Status::Unset),
            ("GET /unavailable", &Status::error("")),
        ]
    );
}

#[tokio::test]
async fn an_unmatched_path_has_no_status_code_attribute() {
    let fixture = fixture(1.0);
    assert_eq!(
        fixture.call("GET", "/no-such-path", None).await,
        StatusCode::NOT_FOUND
    );
    let span = fixture.only_span();
    assert_eq!(span.name, "GET /no-such-path");
    assert_eq!(
        attributes(&span),
        http_attributes("GET", "/no-such-path", None)
    );
}

#[tokio::test]
async fn a_span_started_in_the_handler_is_a_child_of_the_request_span() {
    let fixture = fixture(1.0);
    fixture.call("GET", "/child", Some(&inbound(true))).await;
    let spans = fixture.spans();
    let server = spans.iter().find(|s| s.name == "GET /child").unwrap();
    let child = spans.iter().find(|s| s.name == "outbox.write").unwrap();
    assert_eq!(child.span_kind, SpanKind::Internal);
    assert_eq!(child.parent_span_id, server.span_context.span_id());
    assert_eq!(
        child.span_context.trace_id(),
        server.span_context.trace_id()
    );
}

#[tokio::test]
async fn roots_are_sampled_at_the_ratio_and_children_follow_their_parent() {
    let never = fixture(0.0);
    never.call("GET", "/ok", None).await;
    assert!(never.spans().is_empty(), "ratio 0 samples no root");
    never.call("GET", "/ok", Some(&inbound(true))).await;
    assert_eq!(never.spans().len(), 1, "a sampled parent is followed");

    let always = fixture(1.0);
    always.call("GET", "/ok", Some(&inbound(false))).await;
    assert!(always.spans().is_empty(), "an unsampled parent is followed");
}

#[test]
fn traceparent_is_the_w3c_text_of_a_span_context() {
    let exporter = InMemorySpanExporter::default();
    let tracing = Tracing::with_exporter("user-service", 1.0, exporter);
    let tracer = tracing.tracer();
    let span = tracer.start("outbox.write");
    let cx = Context::current_with_span(span);
    let context = cx.span().span_context().clone();
    assert_eq!(
        traceparent(&context),
        format!("00-{}-{}-01", context.trace_id(), context.span_id())
    );

    let never = Tracing::with_exporter("user-service", 0.0, InMemorySpanExporter::default());
    let span = never.tracer().start("outbox.write");
    let context = span.span_context().clone();
    assert!(context.is_valid(), "an unsampled span still has a context");
    assert!(traceparent(&context).ends_with("-00"));
}

#[test]
fn the_console_exporter_prints_a_line_per_span_as_java_does() {
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::default();
    let tracing = Tracing::with_exporter(
        "user-service",
        1.0,
        ConsoleExporter::to_writer(buffer.clone()),
    );
    let tracer = tracing.tracer();
    let span = tracer
        .span_builder("GET /health")
        .with_kind(SpanKind::Server)
        .with_attributes([
            KeyValue::new("http.request.method", "GET"),
            KeyValue::new("url.path", "/health"),
            KeyValue::new("http.response.status_code", 200_i64),
        ])
        .start(&tracer);
    let context = span.span_context().clone();
    drop(span);
    tracing.force_flush();
    let text = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert_eq!(
        text,
        format!(
            "'GET /health' : {} {} SERVER [tracer: user-service:] AttributesMap{{data={{http.request.method=GET, url.path=/health, http.response.status_code=200}}, capacity=128, totalAddedValues=3}}\n",
            context.trace_id(),
            context.span_id()
        )
    );
}

#[test]
fn shutdown_flushes_batched_spans_and_is_idempotent() {
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::default();
    let tracing = Tracing::with_exporter(
        "user-service",
        1.0,
        ConsoleExporter::to_writer(buffer.clone()),
    );
    drop(tracing.tracer().start("last"));
    tracing.shutdown();
    tracing.shutdown();
    let text = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(text.starts_with("'last' : "), "{text}");
}
