// SPDX-License-Identifier: MIT

//! The service's API description (RUST-4.1): what tapir generated from the
//! Scala endpoints (capture 20261109 openapi.yaml), stated once here next to
//! the DTO types it derives the schemas from, and served with Swagger UI.

use std::path::PathBuf;

use axum::Router;
use serde_json::{Value, json};
use telemetry::docs::openapi::{Api, Operation};
use telemetry::docs::{asyncapi, swagger};

use crate::auth::{LoginRequest, LoginResponse, SignupRequest, SignupResponse};
use crate::events;
use crate::responses::ErrorResponse;
use crate::users::UserResponse;

const USER_ID: &str = "5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e";
const ACCESS: &str = "example-access-token-not-valid";

/// A JSON body: its schema component and example. (Always `Some`: the
/// builder takes `None` for a response without one.)
#[allow(clippy::unnecessary_wraps)]
fn body(schema: &'static str, example: Value) -> Option<(&'static str, Value)> {
    Some((schema, example))
}

fn error(message: &str) -> Option<(&'static str, Value)> {
    body("ErrorResponse", json!({ "error": message }))
}

fn credentials() -> Value {
    json!({"email": "user@example.com", "password": "correct-horse-battery-staple"})
}

/// The OpenAPI document, as `AppRoutes` gives tapir its endpoints: signup,
/// login, the user read, then the two health checks.
#[must_use]
pub fn api() -> Api {
    Api::new("User Service", "1.0")
        .schema::<ErrorResponse>()
        .schema::<LoginRequest>()
        .schema::<LoginResponse>()
        .schema::<SignupRequest>()
        .schema::<SignupResponse>()
        .schema::<UserResponse>()
        .operation(signup())
        .operation(login())
        .operation(get_user())
        .operation(health())
        .operation(ready())
}

fn signup() -> Operation {
    Operation::post("/auth/signup")
        .tag("auth")
        .summary("Sign up")
        .description(
            "Creates an account and returns its first access token (201). The email is trimmed \
             and lower-cased; an invalid email or a password under 8 characters → 400, an \
             already-registered email → 409. Writes the account, token and `UserRegistered` \
             outbox row in one transaction (event published asynchronously). Not idempotent — \
             a retry with the same email → 409.",
        )
        .request_body("SignupRequest", credentials())
        .response(
            201,
            "",
            body(
                "SignupResponse",
                json!({"userId": USER_ID, "token": ACCESS}),
            ),
        )
        .response(
            400,
            "Email is invalid.",
            error("Enter a valid email address"),
        )
        .response(
            400,
            "Password is shorter than 8 characters.",
            error("Password must be at least 8 characters"),
        )
        .response(
            409,
            "Email is already registered.",
            error("An account with this email already exists"),
        )
}

fn login() -> Operation {
    Operation::post("/auth/login")
        .tag("auth")
        .summary("Log in")
        .description(
            "Checks email + password and returns a fresh access token and refresh token (200). \
             An invalid email or empty password → 400. An unknown email and a wrong password \
             both → 401 with the same body; an unknown email still runs one password hash so \
             timing doesn't reveal registered emails. Tokens are opaque; only their SHA-256 \
             hashes are stored.",
        )
        .request_body("LoginRequest", credentials())
        .response(
            200,
            "",
            body(
                "LoginResponse",
                json!({
                    "userId": USER_ID,
                    "token": ACCESS,
                    "refreshToken": "example-refresh-token-not-valid",
                }),
            ),
        )
        .response(
            400,
            "Email is invalid.",
            error("Enter a valid email address"),
        )
        .response(400, "Password is empty.", error("Enter your password"))
        .response(
            401,
            "Unknown email or wrong password.",
            error("Email or password is incorrect"),
        )
}

fn get_user() -> Operation {
    Operation::get("/users/{id}")
        .tag("users")
        .summary("Get a user")
        .description(
            "Returns the public identity (id, email, createdAt) for one account. An unknown id \
             returns 404; a malformed id returns 400. Unauthenticated local/demo read.",
        )
        .path_parameter(
            "id",
            "User id (UUID). A malformed id returns 400.",
            json!(USER_ID),
        )
        .response(
            200,
            "",
            body(
                "UserResponse",
                json!({
                    "userId": USER_ID,
                    "email": "user@example.com",
                    "createdAt": "2026-10-28T12:00:00Z",
                }),
            ),
        )
        .response(404, "No account has this id.", error("User not found"))
}

fn health() -> Operation {
    Operation::get("/health")
        .tag("health")
        .summary("Liveness check")
        .description("200 while the process is up. Checks no dependencies.")
        .response(200, "", None)
}

fn ready() -> Operation {
    Operation::get("/health/ready")
        .tag("health")
        .summary("Readiness check")
        .description(
            "200 when Postgres answers a ping, 503 when it doesn't. Checks no other dependency.",
        )
        .response(200, "", None)
        .response(503, "Postgres didn't answer a ping.", None)
}

/// `/docs` (Swagger UI), `/docs/docs.yaml`, and the AsyncAPI document and
/// HTML (`/docs/asyncapi…`, the HTML from `asyncapi_html_dir`).
pub fn router<S: Clone + Send + Sync + 'static>(asyncapi_html_dir: PathBuf) -> Router<S> {
    swagger::router(api().to_yaml())
        .merge(asyncapi::router(&events::asyncapi(), asyncapi_html_dir))
        .with_state(())
}
