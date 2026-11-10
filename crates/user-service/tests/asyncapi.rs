// SPDX-License-Identifier: MIT

//! AsyncAPI end to end (RUST-4.2 spec FR1-FR2): the `asyncapi-spec` binary
//! (what `scripts/asyncapi-html.sh` renders) and the service's routes.
//! Sources: capture 20261109 asyncapi.json (the telemetry fixture, byte for
//! byte), http/asyncapi*.txt.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::process::Command;

use support::{Postgres, Service};

const CAPTURED: &str = include_str!("../../telemetry/tests/fixtures/asyncapi-user-service.json");

#[test]
fn the_spec_binary_prints_the_scala_document() {
    let output = Command::new(env!("CARGO_BIN_EXE_asyncapi-spec"))
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        CAPTURED.trim_end()
    );
}

#[test]
fn the_service_serves_the_document_and_the_html_directory() {
    let postgres = Postgres::start();
    let html = tempfile::tempdir().unwrap();
    std::fs::write(html.path().join("index.html"), "<html>asyncapi</html>").unwrap();
    let mut env = postgres.env();
    env.push((
        "ASYNCAPI_HTML_DIR",
        html.path().to_str().unwrap().to_owned(),
    ));
    let service = Service::start(&env);

    let (status, content_type, body) = service.get("/docs/asyncapi.json").unwrap();
    assert_eq!(
        (status, content_type.as_deref()),
        (200, Some("application/json"))
    );
    assert_eq!(body, CAPTURED.trim_end());

    let (status, headers, _) = service.request("GET", "/docs/asyncapi", &[]);
    assert_eq!(status, 301);
    assert!(headers.contains(&("location".to_owned(), "/docs/asyncapi/".to_owned())));

    let (status, content_type, page) = service.get("/docs/asyncapi/").unwrap();
    assert_eq!((status, content_type.as_deref()), (200, Some("text/html")));
    assert_eq!(page, "<html>asyncapi</html>");
    let (status, _, body) = service.get("/docs/asyncapi/js/missing.js").unwrap();
    assert_eq!((status, body.as_str()), (404, ""));
}
