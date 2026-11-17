// SPDX-License-Identifier: MIT

//! The handlers against an in-memory store: signup, login and `GET
//! /users/{id}` in every outcome, store failures included, and readiness
//! (captures `http/signup-*.txt`, `login-*.txt`, `users-*.txt`). The Postgres
//! store and the started binary are `tests/auth.rs` and its neighbours; wire
//! parity is the harness's (`test_user_rest.py`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::future::{Future, ready};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SdkTracerProvider;
use serde_json::{Value, json};
use telemetry::logging::Logger;
use telemetry::metrics::Metrics;
use time::macros::datetime;
use tower::ServiceExt;
use user_service::config::AuthConfig;
use user_service::domain::{Credentials, Email};
use user_service::events::OutboxEvent;
use user_service::hashing::PasswordHasher;
use user_service::server::{AppState, router};
use user_service::store::{CreateOutcome, PublicAccount, StoreError, StoredAccount, UserStore};
use user_service::tokens::IssuedToken;
use uuid::Uuid;

const ADA: &str = "00000000-0000-4000-8000-00000000000a";
const ITERATIONS: u32 = 1000;

/// An in-memory `user_account`: accounts by address; `fail` makes every call
/// an error, `down` fails readiness. Counts the tokens stored.
#[derive(Clone, Default)]
struct Memory {
    accounts: Arc<Mutex<HashMap<String, (Uuid, String)>>>,
    tokens: Arc<Mutex<usize>>,
    fail: bool,
    down: bool,
}

impl Memory {
    fn failing() -> Self {
        Self {
            fail: true,
            ..Self::default()
        }
    }

    /// An account for `email` whose stored hash is `hash`.
    fn with(email: &str, hash: String) -> Self {
        let memory = Self::default();
        memory
            .accounts
            .lock()
            .unwrap()
            .insert(email.to_owned(), (Uuid::parse_str(ADA).unwrap(), hash));
        memory
    }

    fn outcome<T>(&self, value: T) -> impl Future<Output = Result<T, StoreError>> + Send
    where
        T: Send,
    {
        ready(if self.fail {
            Err(StoreError::Timeout(Duration::from_secs(1)))
        } else {
            Ok(value)
        })
    }
}

impl UserStore for Memory {
    fn ping(&self, _within: Duration) -> impl Future<Output = bool> + Send {
        ready(!self.down)
    }

    fn create_account(
        &self,
        id: Uuid,
        email: &Email,
        password_hash: &str,
        _access_token: &IssuedToken,
        _event: &OutboxEvent,
    ) -> impl Future<Output = Result<CreateOutcome, StoreError>> + Send {
        let mut accounts = self.accounts.lock().unwrap();
        let outcome = if self.fail || accounts.contains_key(email.as_str()) {
            CreateOutcome::Duplicate
        } else {
            accounts.insert(email.as_str().to_owned(), (id, password_hash.to_owned()));
            *self.tokens.lock().unwrap() += 1;
            CreateOutcome::Created {
                created_at: datetime!(2026-11-17 12:00 UTC),
            }
        };
        self.outcome(outcome)
    }

    fn find_by_email(
        &self,
        email: &Email,
    ) -> impl Future<Output = Result<Option<StoredAccount>, StoreError>> + Send {
        let found = self
            .accounts
            .lock()
            .unwrap()
            .get(email.as_str())
            .map(|(id, password_hash)| StoredAccount {
                id: *id,
                password_hash: password_hash.clone(),
            });
        self.outcome(found)
    }

    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<PublicAccount>, StoreError>> + Send {
        let found = self
            .accounts
            .lock()
            .unwrap()
            .iter()
            .find(|(_, (stored, _))| *stored == id)
            .map(|(email, _)| PublicAccount {
                id,
                email: email.clone(),
                created_at: datetime!(2026-11-17 12:00 UTC),
            });
        self.outcome(found)
    }

    fn record_login_tokens(
        &self,
        _user_id: Uuid,
        _access: &IssuedToken,
        _refresh: &IssuedToken,
    ) -> impl Future<Output = Result<(), StoreError>> + Send {
        if !self.fail {
            *self.tokens.lock().unwrap() += 2;
        }
        self.outcome(())
    }
}

/// The routes over `store`, and the log lines they write.
fn app(store: Memory) -> (Router, Arc<Mutex<Vec<u8>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let state = AppState {
        store,
        metrics: Metrics::new("user-service"),
        hasher: PasswordHasher::new(ITERATIONS),
        auth: AuthConfig {
            access_token_ttl: Duration::from_hours(1),
            refresh_token_ttl: Duration::from_hours(30 * 24),
            pbkdf2_iterations: ITERATIONS,
        },
        tracer: SdkTracerProvider::builder().build().tracer("user-service"),
        logger: Logger::to_writer("user-service", lines.clone()),
        asyncapi_html_dir: std::env::temp_dir(),
    };
    (router(state), lines)
}

/// A stored hash of `password`, as signup writes it.
fn hash_of(password: &str) -> String {
    let credentials = Credentials::signup("ada@example.com", password).unwrap();
    PasswordHasher::new(ITERATIONS).hash(&credentials.password)
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    body: impl Into<Option<Value>>,
) -> (StatusCode, String) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body.into() {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    let response = app.oneshot(request.unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn json_body(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| panic!("not JSON: {text}"))
}

/// The log lines written, as (level, message) pairs.
fn logged(lines: &Arc<Mutex<Vec<u8>>>) -> Vec<(String, String)> {
    String::from_utf8(lines.lock().unwrap().clone())
        .unwrap()
        .lines()
        .map(|line| {
            let line = json_body(line);
            (
                line["level"].as_str().unwrap().to_owned(),
                line["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn credentials(email: &str, password: &str) -> Value {
    json!({"email": email, "password": password})
}

#[tokio::test]
async fn signup_creates_the_account_and_its_first_token() {
    let store = Memory::default();
    let (app, lines) = app(store.clone());
    let (status, body) = send(
        app,
        "POST",
        "/auth/signup",
        credentials(" Ada@Example.com ", "correct-horse-1"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let body = json_body(&body);
    assert_eq!(body["token"].as_str().unwrap().len(), 43);
    assert!(
        store
            .accounts
            .lock()
            .unwrap()
            .contains_key("ada@example.com")
    );
    assert_eq!(*store.tokens.lock().unwrap(), 1);
    assert_eq!(logged(&lines).last().unwrap().1, "Account created");
}

#[tokio::test]
async fn a_taken_address_is_a_409() {
    let (app, lines) = app(Memory::with("ada@example.com", hash_of("correct-horse-1")));
    let (status, body) = send(
        app,
        "POST",
        "/auth/signup",
        credentials("ada@example.com", "another-password"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        json_body(&body),
        json!({"error": "An account with this email already exists"})
    );
    assert_eq!(
        logged(&lines).last().unwrap(),
        &("WARN".to_owned(), "Duplicate signup".to_owned())
    );
}

#[tokio::test]
async fn invalid_signups_are_400s_and_store_nothing() {
    for (email, password, error) in [
        (
            "not-an-email",
            "correct-horse-1",
            "Enter a valid email address",
        ),
        (
            "ada@example.com",
            "short",
            "Password must be at least 8 characters",
        ),
    ] {
        let store = Memory::default();
        let (app, lines) = app(store.clone());
        let (status, body) = send(app, "POST", "/auth/signup", credentials(email, password)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json_body(&body), json!({"error": error}));
        assert!(store.accounts.lock().unwrap().is_empty());
        assert_eq!(logged(&lines).last().unwrap().1, "Invalid signup request");
    }
}

#[tokio::test]
async fn a_failing_store_is_a_plain_500_and_an_error_line() {
    for (method, uri, body, logged_as) in [
        (
            "POST",
            "/auth/signup".to_owned(),
            Some(credentials("ada@example.com", "correct-horse-1")),
            "Request failed",
        ),
        (
            "POST",
            "/auth/login".to_owned(),
            Some(credentials("ada@example.com", "correct-horse-1")),
            "Request failed",
        ),
        ("GET", format!("/users/{ADA}"), None, "Request failed"),
    ] {
        let (app, lines) = app(Memory::failing());
        let (status, text) = send(app, method, &uri, body).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{uri}");
        assert_eq!(text, "Internal Server Error");
        assert_eq!(
            logged(&lines).last().unwrap(),
            &("ERROR".to_owned(), logged_as.to_owned()),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn login_issues_an_access_and_a_refresh_token() {
    let store = Memory::with("ada@example.com", hash_of("correct-horse-1"));
    let (app, lines) = app(store.clone());
    let (status, body) = send(
        app,
        "POST",
        "/auth/login",
        credentials("ADA@example.com", "correct-horse-1"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let body = json_body(&body);
    assert_eq!(body["userId"], ADA);
    assert_ne!(body["token"], body["refreshToken"]);
    assert_eq!(*store.tokens.lock().unwrap(), 2);
    assert_eq!(logged(&lines).last().unwrap().1, "Login succeeded");
}

#[tokio::test]
async fn a_wrong_password_and_an_unknown_address_answer_alike() {
    for (email, password) in [
        ("ada@example.com", "wrong-password"),
        ("bob@example.com", "correct-horse-1"),
    ] {
        let store = Memory::with("ada@example.com", hash_of("correct-horse-1"));
        let (app, lines) = app(store.clone());
        let (status, body) = send(app, "POST", "/auth/login", credentials(email, password)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(
            json_body(&body),
            json!({"error": "Email or password is incorrect"})
        );
        assert_eq!(*store.tokens.lock().unwrap(), 0);
        assert_eq!(
            logged(&lines).last().unwrap(),
            &("WARN".to_owned(), "Login failed".to_owned())
        );
    }
}

#[tokio::test]
async fn a_malformed_stored_hash_is_a_401_and_an_error_line() {
    let (app, lines) = app(Memory::with("ada@example.com", "not-a-hash".to_owned()));
    let (status, body) = send(
        app,
        "POST",
        "/auth/login",
        credentials("ada@example.com", "correct-horse-1"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        json_body(&body),
        json!({"error": "Email or password is incorrect"})
    );
    assert_eq!(
        logged(&lines).last().unwrap(),
        &(
            "ERROR".to_owned(),
            "Stored password hash is malformed".to_owned()
        )
    );
}

#[tokio::test]
async fn an_empty_login_password_is_a_400() {
    let (app, _lines) = app(Memory::default());
    let (status, body) = send(
        app,
        "POST",
        "/auth/login",
        credentials("ada@example.com", ""),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json_body(&body), json!({"error": "Enter your password"}));
}

#[tokio::test]
async fn a_user_is_found_by_id_or_is_a_404() {
    let (app, _lines) = app(Memory::with("ada@example.com", hash_of("correct-horse-1")));
    let (status, body) = send(app.clone(), "GET", &format!("/users/{ADA}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json_body(&body),
        json!({"userId": ADA, "email": "ada@example.com", "createdAt": "2026-11-17T12:00:00Z"})
    );
    let (status, body) = send(
        app,
        "GET",
        "/users/00000000-0000-4000-8000-00000000000b",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json_body(&body), json!({"error": "User not found"}));
}

#[tokio::test]
async fn a_malformed_user_id_is_tapirs_400() {
    let (app, _lines) = app(Memory::default());
    let (status, text) = send(app, "GET", "/users/not-a-uuid", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "Invalid value for: path parameter id");
}

#[tokio::test]
async fn readiness_is_the_stores_ping() {
    let (up, _lines) = app(Memory::default());
    assert_eq!(
        send(up, "GET", "/health/ready", None).await.0,
        StatusCode::OK
    );
    let (down, _lines) = app(Memory {
        down: true,
        ..Memory::default()
    });
    assert_eq!(
        send(down, "GET", "/health/ready", None).await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}
