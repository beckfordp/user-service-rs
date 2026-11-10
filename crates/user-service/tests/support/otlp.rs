// SPDX-License-Identifier: MIT

//! A stand-in OTLP/HTTP collector: accepts `POST /v1/traces` (protobuf) and
//! keeps the spans, reduced to what the tests look at.

#![allow(clippy::struct_field_names, clippy::format_collect)]

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value;
use prost::Message;

/// One received span.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub trace_id: String,
    pub span_id: String,
    /// Empty for a root.
    pub parent_span_id: String,
    pub name: String,
    /// "SERVER", "INTERNAL", …
    pub kind: &'static str,
    /// Values as text.
    pub attributes: BTreeMap<String, String>,
    pub service_name: String,
    pub scope: String,
    /// 0 unset, 1 ok, 2 error.
    pub status: i32,
}

/// The collector; its URL goes in `OTEL_EXPORTER_OTLP_ENDPOINT`.
pub struct OtlpSink {
    pub endpoint: String,
    spans: Arc<Mutex<Vec<Span>>>,
    paths: Arc<Mutex<Vec<String>>>,
}

impl OtlpSink {
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let spans: Arc<Mutex<Vec<Span>>> = Arc::default();
        let paths: Arc<Mutex<Vec<String>>> = Arc::default();
        let (into, seen) = (spans.clone(), paths.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (into, seen) = (into.clone(), seen.clone());
                std::thread::spawn(move || serve(stream, &into, &seen));
            }
        });
        Self {
            endpoint,
            spans,
            paths,
        }
    }

    /// Every span received so far.
    pub fn spans(&self) -> Vec<Span> {
        self.spans.lock().unwrap().clone()
    }

    /// The request paths exports arrived on.
    pub fn paths(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }

    /// Waits up to 20 s for `count` spans matching `predicate`.
    pub fn wait_for(&self, count: usize, predicate: impl Fn(&Span) -> bool) -> Vec<Span> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let matching: Vec<Span> = self.spans().into_iter().filter(&predicate).collect();
            if matching.len() >= count {
                return matching;
            }
            assert!(
                Instant::now() < deadline,
                "{} of {count} spans after 20 s; received {:#?}",
                matching.len(),
                self.spans()
            );
            sleep(Duration::from_millis(100));
        }
    }
}

fn serve(stream: TcpStream, into: &Mutex<Vec<Span>>, seen: &Mutex<Vec<String>>) {
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_owned();
        let mut length = 0;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        seen.lock().unwrap().push(path);
        into.lock().unwrap().extend(decode(&body));
        writer
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-protobuf\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::StringValue(s)) => s.clone(),
        Some(Value::IntValue(i)) => i.to_string(),
        Some(Value::BoolValue(b)) => b.to_string(),
        Some(Value::DoubleValue(d)) => d.to_string(),
        other => format!("{other:?}"),
    }
}

fn decode(body: &[u8]) -> Vec<Span> {
    let request = ExportTraceServiceRequest::decode(body).unwrap();
    let mut spans = Vec::new();
    for resource_spans in request.resource_spans {
        let service_name = resource_spans
            .resource
            .iter()
            .flat_map(|r| &r.attributes)
            .find(|kv| kv.key == "service.name")
            .map(|kv| text(kv.value.as_ref().and_then(|v| v.value.as_ref())))
            .unwrap_or_default();
        for scope_spans in resource_spans.scope_spans {
            let scope = scope_spans.scope.map(|s| s.name).unwrap_or_default();
            for span in scope_spans.spans {
                spans.push(Span {
                    trace_id: hex(&span.trace_id),
                    span_id: hex(&span.span_id),
                    parent_span_id: hex(&span.parent_span_id),
                    name: span.name,
                    kind: match span.kind {
                        1 => "INTERNAL",
                        2 => "SERVER",
                        3 => "CLIENT",
                        4 => "PRODUCER",
                        5 => "CONSUMER",
                        _ => "UNSPECIFIED",
                    },
                    attributes: span
                        .attributes
                        .iter()
                        .map(|kv| {
                            (
                                kv.key.clone(),
                                text(kv.value.as_ref().and_then(|v| v.value.as_ref())),
                            )
                        })
                        .collect(),
                    service_name: service_name.clone(),
                    scope: scope.clone(),
                    status: span.status.map_or(0, |s| s.code),
                });
            }
        }
    }
    spans
}
