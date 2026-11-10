// SPDX-License-Identifier: MIT

//! `GET /users/{id}`, method and path errors, CORS (RUST-2.6). Source: capture
//! 20261109 http/users-*.txt, *-wrong-method.txt, unknown-path.txt, cors-*.txt.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde_json::{Value, json};
use support::{Postgres, Service};

fn started() -> (Postgres, Service) {
    let postgres = Postgres::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    let service = Service::start(&env);
    (postgres, service)
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

const TEXT: Option<&str> = Some("text/plain; charset=UTF-8");

#[test]
fn a_user_reads_back_with_its_creation_instant_as_java_prints_it() {
    let (postgres, service) = started();
    let (_, _, created) = service.post_json(
        "/auth/signup",
        &json!({"email": "Ada@Example.com", "password": "correct-horse-1"}).to_string(),
    );
    let user_id = serde_json::from_str::<Value>(&created).unwrap()["userId"]
        .as_str()
        .unwrap()
        .to_owned();
    let stored = postgres.query(&format!(
        "select to_char(created_at at time zone 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US') from user_account where id = '{user_id}'"
    ));
    let (status, content_type, body) = service
        .get(&format!("/users/{}", user_id.to_uppercase()))
        .unwrap();
    assert_eq!(
        (status, content_type.as_deref()),
        (200, Some("application/json"))
    );
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["userId"], json!(user_id));
    assert_eq!(body["email"], json!("ada@example.com"));
    let created_at = body["createdAt"].as_str().unwrap();
    let micros = stored.trim().rsplit('.').next().unwrap();
    let fraction = if micros == "000000" {
        String::new()
    } else if micros.ends_with("000") {
        format!(".{}", &micros[..3])
    } else {
        format!(".{micros}")
    };
    assert_eq!(
        created_at,
        format!("{}{fraction}Z", stored.trim().rsplit_once('.').unwrap().0)
    );
    // serde_json keeps the wire order (preserve_order, via schemars): the
    // capture's (http/users-200.txt).
    assert_eq!(
        body.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["userId", "email", "createdAt"]
    );
}

#[test]
fn unknown_and_malformed_ids() {
    let (_postgres, service) = started();
    let (status, content_type, body) = service
        .get("/users/00000000-0000-0000-0000-000000000000")
        .unwrap();
    assert_eq!(
        (status, content_type.as_deref(), body.as_str()),
        (
            404,
            Some("application/json"),
            r#"{"error":"User not found"}"#
        )
    );
    let (status, content_type, body) = service.get("/users/not-a-uuid").unwrap();
    assert_eq!(
        (status, content_type.as_deref(), body.as_str()),
        (400, TEXT, "Invalid value for: path parameter id")
    );
}

#[test]
fn wrong_methods_are_405_and_unknown_paths_404_in_plain_text() {
    let (_postgres, service) = started();
    for (method, path) in [
        ("GET", "/auth/signup"),
        ("DELETE", "/users/00000000-0000-0000-0000-000000000000"),
        ("HEAD", "/health"),
    ] {
        let (status, headers, body) = service.request(method, path, &[]);
        assert_eq!(status, 405, "{method} {path}");
        assert_eq!(header(&headers, "content-type"), TEXT, "{method} {path}");
        if method != "HEAD" {
            assert_eq!(body, "Method Not Allowed");
        }
    }
    let (status, headers, body) = service.request("GET", "/no-such-path", &[]);
    assert_eq!(
        (status, header(&headers, "content-type"), body.as_str()),
        (404, TEXT, "Not found")
    );
}

#[test]
fn cors_preflight_and_cross_origin_responses() {
    let (_postgres, service) = started();
    let (status, headers, body) = service.request(
        "OPTIONS",
        "/auth/signup",
        &[
            ("Origin", "http://localhost:5173"),
            ("Access-Control-Request-Method", "POST"),
            ("Access-Control-Request-Headers", "content-type"),
        ],
    );
    assert_eq!((status, body.as_str()), (200, ""));
    assert_eq!(header(&headers, "access-control-allow-origin"), Some("*"));
    assert_eq!(
        header(&headers, "access-control-allow-methods"),
        Some("PATCH, PUT, GET, POST, DELETE")
    );
    assert_eq!(
        header(&headers, "access-control-allow-headers"),
        Some("Content-Type")
    );
    assert_eq!(
        header(&headers, "vary"),
        Some("Access-Control-Request-Method, Access-Control-Request-Headers")
    );

    let (_, headers, _) = service.request("GET", "/health", &[("Origin", "http://localhost:5173")]);
    assert_eq!(header(&headers, "access-control-allow-origin"), Some("*"));
    assert_eq!(
        header(&headers, "access-control-expose-headers"),
        Some("X-Total-Count")
    );

    let (_, headers, _) = service.request("GET", "/health", &[]);
    assert!(
        headers
            .iter()
            .all(|(name, _)| !name.starts_with("access-control-")),
        "{headers:?}"
    );
}
