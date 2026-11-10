// SPDX-License-Identifier: MIT

//! Tracing end to end (RUST-3.2 spec FR1-FR3): the binary exporting to a
//! stand-in OTLP collector. Sources: capture 20261109 stdout.log (spans) and
//! db-outbox-events.jsonl (`trace_parent`); purerest Tracing; user-service
//! `UserStore.insertOutbox`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;

use serde_json::{Value, json};
use support::otlp::OtlpSink;
use support::{Postgres, Service};

const TRACE: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
const PARENT: &str = "00f067aa0ba902b7";

fn started(extra: &[(&'static str, String)]) -> (Postgres, OtlpSink, Service) {
    let postgres = Postgres::start();
    let sink = OtlpSink::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    env.push(("OTEL_EXPORTER_OTLP_ENDPOINT", sink.endpoint.clone()));
    env.extend(extra.iter().cloned());
    let service = Service::start(&env);
    (postgres, sink, service)
}

fn signup(service: &Service, headers: &[(&str, &str)]) -> String {
    let body = json!({"email": "kat@example.com", "password": "correct-horse-1"}).to_string();
    let mut all = vec![("Content-Type", "application/json")];
    all.extend_from_slice(headers);
    let (status, _, text) = service.request_with_body("POST", "/auth/signup", &all, &body);
    assert_eq!(status, 201, "{text}");
    serde_json::from_str::<Value>(&text).unwrap()["userId"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn attrs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

#[test]
fn a_signup_is_a_server_span_with_an_outbox_write_child_named_in_the_row() {
    let (postgres, sink, service) = started(&[]);
    let inbound = format!("00-{TRACE}-{PARENT}-01");
    let user_id = signup(&service, &[("traceparent", &inbound)]);
    let spans = sink.wait_for(2, |s| s.trace_id == TRACE);
    let server = spans
        .iter()
        .find(|s| s.name == "POST /auth/signup")
        .unwrap();
    let outbox = spans.iter().find(|s| s.name == "outbox.write").unwrap();

    assert_eq!(
        (server.kind, server.parent_span_id.as_str()),
        ("SERVER", PARENT)
    );
    assert_eq!(
        server.attributes,
        attrs(&[
            ("http.request.method", "POST"),
            ("url.path", "/auth/signup"),
            ("http.response.status_code", "201"),
        ])
    );
    assert_eq!(
        (outbox.kind, outbox.parent_span_id.as_str()),
        ("INTERNAL", server.span_id.as_str())
    );
    assert_eq!(
        outbox.attributes,
        attrs(&[
            ("outbox.event_type", "UserRegistered"),
            ("outbox.aggregate_id", &user_id),
        ])
    );
    for span in [server, outbox] {
        assert_eq!(
            (span.service_name.as_str(), span.scope.as_str()),
            ("user-service", "user-service")
        );
    }
    assert_eq!(
        postgres
            .query("SELECT trace_parent FROM outbox_events")
            .trim(),
        format!("00-{TRACE}-{}-01", outbox.span_id)
    );
    assert!(
        sink.paths().iter().all(|p| p == "/v1/traces"),
        "{:?}",
        sink.paths()
    );
}

#[test]
fn without_a_traceparent_the_request_span_is_a_root() {
    let (_postgres, sink, service) = started(&[]);
    service.get("/health").unwrap();
    let spans = sink.wait_for(1, |s| s.name == "GET /health");
    assert_eq!(spans[0].parent_span_id, "");
    assert_eq!(spans[0].status, 0);
}

#[test]
fn the_service_name_names_the_resource_and_the_scope() {
    let (_postgres, sink, service) =
        started(&[("USER_SERVICE_NAME", "renamed-user-service".to_owned())]);
    service.get("/health").unwrap();
    let spans = sink.wait_for(1, |s| s.name == "GET /health");
    assert_eq!(spans[0].service_name, "renamed-user-service");
    assert_eq!(spans[0].scope, "renamed-user-service");
}

#[test]
fn an_unmatched_path_and_a_preflight() {
    let (_postgres, sink, service) = started(&[]);
    let inbound = format!("00-{TRACE}-{PARENT}-01");
    service.request(
        "OPTIONS",
        "/auth/signup",
        &[
            ("Origin", "http://example.com"),
            ("Access-Control-Request-Method", "POST"),
            ("traceparent", &inbound),
        ],
    );
    let (status, _, _) = service.request("GET", "/no-such-path", &[("traceparent", &inbound)]);
    assert_eq!(status, 404);
    let spans = sink.wait_for(1, |s| s.trace_id == TRACE);
    std::thread::sleep(std::time::Duration::from_millis(500));
    let spans_now: Vec<_> = sink
        .spans()
        .into_iter()
        .filter(|s| s.trace_id == TRACE)
        .collect();
    assert_eq!(
        spans_now.len(),
        1,
        "the preflight is not traced: {spans_now:#?}"
    );
    assert_eq!(spans[0].name, "GET /no-such-path");
    assert_eq!(
        spans[0].attributes,
        attrs(&[
            ("http.request.method", "GET"),
            ("url.path", "/no-such-path")
        ])
    );
}

#[test]
fn a_sampling_ratio_of_zero_exports_no_root_but_still_writes_the_trace_parent() {
    let (postgres, sink, mut service) = started(&[("OTEL_TRACES_SAMPLER_ARG", "0".to_owned())]);
    signup(&service, &[]);
    let trace_parent = postgres.query("SELECT trace_parent FROM outbox_events");
    assert!(trace_parent.trim().ends_with("-00"), "{trace_parent}");
    service.terminate();
    service.wait_exit();
    assert!(sink.spans().is_empty(), "{:#?}", sink.spans());
}

#[test]
fn spans_are_flushed_on_shutdown() {
    let (_postgres, sink, mut service) = started(&[]);
    service.get("/health").unwrap();
    service.terminate();
    service.wait_exit();
    assert!(
        sink.spans().iter().any(|s| s.name == "GET /health"),
        "batched spans exported before exit: {:#?}",
        sink.spans()
    );
}

#[test]
fn without_an_endpoint_spans_go_to_stderr() {
    let postgres = Postgres::start();
    let mut service = Service::start(&postgres.env());
    service.get("/health").unwrap();
    service.terminate();
    service.wait_exit();
    let stderr = service.stderr();
    assert!(
        stderr
            .lines()
            .any(|line| line.starts_with("'GET /health' : ")
                && line.contains(" SERVER [tracer: user-service:] ")),
        "{stderr}"
    );
}
