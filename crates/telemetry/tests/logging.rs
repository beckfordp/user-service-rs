// SPDX-License-Identifier: MIT

//! Log lines as logstash-logback-encoder writes them for purerest's
//! trace-correlated logger (RUST-3.4 spec FR1). Sources: capture 20261109
//! stdout.log; purerest `Logging`, `logback-docker.xml`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use opentelemetry::Context;
use opentelemetry::trace::{TraceContextExt, Tracer};
use opentelemetry_sdk::trace::InMemorySpanExporter;
use serde_json::Value;
use telemetry::logging::{Logger, logstash_timestamp};
use telemetry::tracing::Tracing;
use time::macros::datetime;

fn logger() -> (Logger, Arc<Mutex<Vec<u8>>>) {
    let buffer: Arc<Mutex<Vec<u8>>> = Arc::default();
    (Logger::to_writer("user-service", buffer.clone()), buffer)
}

fn lines(buffer: &Arc<Mutex<Vec<u8>>>) -> Vec<String> {
    String::from_utf8(buffer.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(ToOwned::to_owned)
        .collect()
}

fn keys(line: &str) -> Vec<String> {
    // Key order as written (serde_json's Map may reorder, so read it raw).
    let value: Value = serde_json::from_str(line).unwrap();
    let mut found: Vec<(usize, String)> = value
        .as_object()
        .unwrap()
        .keys()
        .map(|key| (line.find(&format!("\"{key}\":")).unwrap(), key.clone()))
        .collect();
    found.sort();
    found.into_iter().map(|(_, key)| key).collect()
}

#[test]
fn a_line_has_the_logstash_fields_first_then_the_context() {
    let (logger, buffer) = logger();
    logger.info(
        &[("method", "POST"), ("path", "/auth/signup")],
        "Received request",
    );
    let lines = lines(&buffer);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("{\"@timestamp\":\""), "{}", lines[0]);
    assert_eq!(
        keys(&lines[0]),
        [
            "@timestamp",
            "@version",
            "message",
            "logger_name",
            "thread_name",
            "level",
            "level_value",
            "method",
            "path"
        ]
    );
    let line: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(line["@version"], "1");
    assert_eq!(line["message"], "Received request");
    assert_eq!(line["logger_name"], "user-service");
    assert_eq!(line["level"], "INFO");
    assert_eq!(line["level_value"], 20000);
    assert_eq!(line["method"], "POST");
    assert!(line["thread_name"].is_string());
}

#[test]
fn levels_have_logbacks_values() {
    let (logger, buffer) = logger();
    logger.info(&[], "i");
    logger.warn(&[], "w");
    logger.error(&[], "e");
    let levels: Vec<(String, i64)> = lines(&buffer)
        .iter()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .map(|v| {
            (
                v["level"].as_str().unwrap().to_owned(),
                v["level_value"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        levels,
        [
            ("INFO".to_owned(), 20000),
            ("WARN".to_owned(), 30000),
            ("ERROR".to_owned(), 40000)
        ]
    );
}

#[test]
fn inside_a_span_the_line_carries_its_trace_and_span_ids() {
    let (logger, buffer) = logger();
    let tracing = Tracing::with_exporter("user-service", 1.0, InMemorySpanExporter::default());
    let span = tracing.tracer().start("POST /auth/signup");
    let cx = Context::current_with_span(span);
    let expected = cx.span().span_context().clone();
    {
        let _guard = cx.clone().attach();
        logger.info(&[("method", "POST")], "Received request");
    }
    logger.info(&[], "outside");
    let lines = lines(&buffer);
    assert_eq!(
        keys(&lines[0])[7..],
        [
            "method".to_owned(),
            "trace_id".to_owned(),
            "span_id".to_owned()
        ]
    );
    let line: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(line["trace_id"], expected.trace_id().to_string());
    assert_eq!(line["span_id"], expected.span_id().to_string());
    let outside: Value = serde_json::from_str(&lines[1]).unwrap();
    assert!(outside.get("trace_id").is_none());
}

#[test]
fn text_is_escaped_into_one_line() {
    let (logger, buffer) = logger();
    logger.warn(&[("error", "a \"quoted\"\nline\\")], "two\nlines");
    let lines = lines(&buffer);
    assert_eq!(lines.len(), 1);
    let line: Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(line["message"], "two\nlines");
    assert_eq!(line["error"], "a \"quoted\"\nline\\");
}

#[test]
fn timestamps_are_utc_with_the_fraction_trimmed_at_microseconds() {
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 18:43:20.751790 UTC)),
        "2026-11-09T18:43:20.75179Z"
    );
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 18:43:16.528934 UTC)),
        "2026-11-09T18:43:16.528934Z"
    );
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 18:43:16.5 UTC)),
        "2026-11-09T18:43:16.5Z"
    );
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 18:43:16 UTC)),
        "2026-11-09T18:43:16Z"
    );
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 18:43:16.000000999 UTC)),
        "2026-11-09T18:43:16Z"
    );
    assert_eq!(
        logstash_timestamp(datetime!(2026-11-09 19:43:16.1 +01:00)),
        "2026-11-09T18:43:16.1Z"
    );
}

#[test]
fn lines_are_stamped_now() {
    let (logger, buffer) = logger();
    logger.info(&[], "now");
    let line: Value = serde_json::from_str(&lines(&buffer)[0]).unwrap();
    let stamp = line["@timestamp"].as_str().unwrap();
    let parsed =
        time::OffsetDateTime::parse(stamp, &time::format_description::well_known::Rfc3339).unwrap();
    assert!((time::OffsetDateTime::now_utc() - parsed).abs() < time::Duration::seconds(5));
}
