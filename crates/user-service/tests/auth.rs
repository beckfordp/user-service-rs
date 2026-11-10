// SPDX-License-Identifier: MIT

//! Signup and login end to end (RUST-2.5 spec FR4-FR5): the binary against a
//! throwaway Postgres. Sources: capture 20261109 http/signup-*.txt, login-*.txt.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde_json::{Value, json};
use support::{Postgres, Service};

const JSON: Option<&str> = Some("application/json");
const TEXT: Option<&str> = Some("text/plain; charset=UTF-8");

fn body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| panic!("not JSON: {text}"))
}

fn credentials(email: &str, password: &str) -> String {
    json!({"email": email, "password": password}).to_string()
}

fn started() -> (Postgres, Service) {
    let postgres = Postgres::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    let service = Service::start(&env);
    (postgres, service)
}

#[test]
fn signup_creates_an_account_once() {
    let (_postgres, service) = started();
    let (status, content_type, created) = service.post_json(
        "/auth/signup",
        &credentials(" Ada@Example.com ", "correct-horse-1"),
    );
    assert_eq!((status, content_type.as_deref()), (201, JSON));
    let created = body(&created);
    assert_eq!(created.as_object().unwrap().len(), 2);
    assert_eq!(created["userId"].as_str().unwrap().len(), 36);
    assert_eq!(created["token"].as_str().unwrap().len(), 43);

    let (status, content_type, duplicate) = service.post_json(
        "/auth/signup",
        &credentials("ADA@example.com", "another-password"),
    );
    assert_eq!((status, content_type.as_deref()), (409, JSON));
    assert_eq!(
        body(&duplicate),
        json!({"error": "An account with this email already exists"})
    );
}

#[test]
fn signup_refuses_bad_credentials_with_the_scala_messages() {
    let (_postgres, service) = started();
    let (status, _, invalid) = service.post_json(
        "/auth/signup",
        &credentials("not-an-email", "correct-horse-1"),
    );
    assert_eq!(
        (status, body(&invalid)),
        (400, json!({"error": "Enter a valid email address"}))
    );
    let (status, _, short) =
        service.post_json("/auth/signup", &credentials("a@example.com", "short"));
    assert_eq!(
        (status, body(&short)),
        (
            400,
            json!({"error": "Password must be at least 8 characters"})
        )
    );
}

#[test]
fn an_undecodable_body_is_a_plain_text_400() {
    let (_postgres, service) = started();
    for (path, raw) in [
        ("/auth/signup", "{not json"),
        ("/auth/signup", r#"{"email":"a@example.com"}"#),
        ("/auth/signup", r#"{"email":1,"password":true}"#),
        ("/auth/signup", ""),
        ("/auth/login", "{not json"),
    ] {
        let (status, content_type, text) = service.post(path, raw.as_bytes(), JSON);
        assert_eq!(
            (status, content_type.as_deref()),
            (400, TEXT),
            "{path} {raw:?}"
        );
        assert!(text.starts_with("Invalid value for: body ("), "{text}");
    }
}

#[test]
fn the_content_type_is_not_checked() {
    let (_postgres, service) = started();
    let raw = credentials("tp@example.com", "correct-horse-1");
    assert_eq!(
        service
            .post("/auth/signup", raw.as_bytes(), Some("text/plain"))
            .0,
        201
    );
    let raw = credentials("nct@example.com", "correct-horse-1");
    assert_eq!(service.post("/auth/signup", raw.as_bytes(), None).0, 201);
}

#[test]
fn login_returns_a_token_pair_for_the_right_password_only() {
    let (_postgres, service) = started();
    let (_, _, created) = service.post_json(
        "/auth/signup",
        &credentials("grace@example.com", "correct-horse-1"),
    );
    let user_id = body(&created)["userId"].clone();

    let (status, content_type, logged_in) = service.post_json(
        "/auth/login",
        &credentials(" GRACE@example.com", "correct-horse-1"),
    );
    assert_eq!((status, content_type.as_deref()), (200, JSON));
    let logged_in = body(&logged_in);
    assert_eq!(logged_in["userId"], user_id);
    assert_eq!(logged_in["token"].as_str().unwrap().len(), 43);
    assert_eq!(logged_in["refreshToken"].as_str().unwrap().len(), 43);

    let refused = json!({"error": "Email or password is incorrect"});
    let (status, _, wrong) = service.post_json(
        "/auth/login",
        &credentials("grace@example.com", "wrong-password"),
    );
    assert_eq!((status, body(&wrong)), (401, refused.clone()));
    let (status, _, unknown) = service.post_json(
        "/auth/login",
        &credentials("nobody@example.com", "wrong-password"),
    );
    assert_eq!((status, body(&unknown)), (401, refused));
    let (status, _, empty) =
        service.post_json("/auth/login", &credentials("grace@example.com", ""));
    assert_eq!(
        (status, body(&empty)),
        (400, json!({"error": "Enter your password"}))
    );
}
