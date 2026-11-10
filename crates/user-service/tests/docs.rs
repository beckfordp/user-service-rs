// SPDX-License-Identifier: MIT

//! The docs end to end (RUST-4.1 spec FR1-FR3): the binary's `/docs` paths
//! and the OpenAPI document it serves. Sources: capture 20261109
//! openapi.yaml, http/docs*.txt. (The harness's docs group compares the whole
//! document with the capture; this checks the parts a reader relies on.)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde_json::{Value, json};
use support::{Postgres, Service};

fn started() -> (Postgres, Service) {
    let postgres = Postgres::start();
    let service = Service::start(&postgres.env());
    (postgres, service)
}

fn spec(service: &Service) -> Value {
    let (status, content_type, body) = service.get("/docs/docs.yaml").unwrap();
    assert_eq!(status, 200);
    assert_eq!(content_type.as_deref(), Some("application/yaml"));
    serde_yaml::from_str(&body).unwrap()
}

#[test]
fn the_spec_describes_every_operation_as_tapir_did() {
    let (_postgres, service) = started();
    let spec = spec(&service);
    assert_eq!(spec["openapi"], "3.1.0");
    assert_eq!(
        spec["info"],
        json!({"title": "User Service", "version": "1.0"})
    );
    let operations: Vec<(String, String, String)> = spec["paths"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(path, item)| {
            item.as_object().unwrap().iter().map(move |(method, op)| {
                (
                    path.clone(),
                    method.clone(),
                    op["operationId"].as_str().unwrap().to_owned(),
                )
            })
        })
        .collect();
    let expected: Vec<(String, String, String)> = [
        ("/auth/signup", "post", "postAuthSignup"),
        ("/auth/login", "post", "postAuthLogin"),
        ("/users/{id}", "get", "getUsersId"),
        ("/health", "get", "getHealth"),
        ("/health/ready", "get", "getHealthReady"),
    ]
    .iter()
    .map(|(p, m, o)| ((*p).to_owned(), (*m).to_owned(), (*o).to_owned()))
    .collect();
    assert_eq!(operations, expected);

    let signup = &spec["paths"]["/auth/signup"]["post"];
    let statuses: Vec<&String> = signup["responses"].as_object().unwrap().keys().collect();
    assert_eq!(statuses, ["201", "400", "409"]);
    assert_eq!(
        signup["responses"]["400"]["content"]["application/json"]["schema"]["anyOf"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        signup["responses"]["400"]["description"],
        "Email is invalid."
    );
    assert_eq!(
        spec["paths"]["/health/ready"]["get"]["responses"]["503"]["description"],
        "Postgres didn't answer a ping."
    );
    let schemas: Vec<&String> = spec["components"]["schemas"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(
        schemas,
        [
            "ErrorResponse",
            "LoginRequest",
            "LoginResponse",
            "SignupRequest",
            "SignupResponse",
            "UserResponse"
        ]
    );
    assert_eq!(
        spec["components"]["schemas"]["UserResponse"]["properties"]["createdAt"],
        json!({
            "description": "When the account was created (ISO-8601 instant).",
            "examples": ["2026-10-28T12:00:00Z"],
            "type": "string",
            "format": "date-time"
        })
    );
}

#[test]
fn swagger_ui_is_served_under_docs() {
    let (_postgres, service) = started();
    let (status, headers, _) = service.request("GET", "/docs", &[]);
    assert_eq!(status, 308);
    assert!(headers.contains(&("location".to_owned(), "./docs/".to_owned())));
    let (status, content_type, page) = service.get("/docs/").unwrap();
    assert_eq!((status, content_type.as_deref()), (200, Some("text/html")));
    assert!(page.contains("<title>Swagger UI</title>"));
    let (status, _, _) = service.request("HEAD", "/docs/docs.yaml", &[]);
    assert_eq!(status, 405);
}

#[test]
fn the_examples_are_valid_requests() {
    // The documented request examples work against the service itself.
    let (_postgres, service) = started();
    let spec = spec(&service);
    let example = &spec["paths"]["/auth/signup"]["post"]["requestBody"]["content"]["application/json"]
        ["example"];
    let (status, _, _) = service.post_json("/auth/signup", &example.to_string());
    assert_eq!(status, 201);
}
