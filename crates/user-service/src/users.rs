// SPDX-License-Identifier: MIT

//! `GET /users/{id}` (RUST-2.6; ADM-3, ADR 0025): public identity for
//! recipient resolution. Source: capture 20261109 http/users-*.txt.

use std::fmt::Write as _;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use schemars::JsonSchema;
use serde::Serialize;
use telemetry::jvm::parse_uuid;
use time::{OffsetDateTime, UtcOffset};

use crate::auth::internal;
use crate::responses::{TextError, json_error};
use crate::server::AppState;

/// Public identity for one account.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    /// User id (UUID).
    #[schemars(example = &"5b6c7d8e-9f0a-4b1c-8d2e-3f4a5b6c7d8e")]
    pub user_id: String,
    /// Normalized email address stored at signup.
    #[schemars(example = &"user@example.com")]
    pub email: String,
    /// When the account was created (ISO-8601 instant).
    #[schemars(extend("format" = "date-time"), example = &"2026-10-28T12:00:00Z")]
    pub created_at: String,
}

/// `GET /users/{id}`.
pub async fn get_user(State(state): State<AppState>, Path(raw): Path<String>) -> Response {
    // tapir's UUID codec is `java.util.UUID.fromString`: lenient groups, no
    // undashed, braced or URN forms (RUST-3.3; harness test_rest.py).
    let Some(id) = parse_uuid(&raw) else {
        return TextError {
            status: StatusCode::BAD_REQUEST,
            message: "Invalid value for: path parameter id".to_owned(),
        }
        .into_response();
    };
    // user-service `UserRoutes`: the path and user_id name the canonical id.
    let user_id = id.to_string();
    let path = format!("/users/{user_id}");
    let context = [
        ("method", "GET"),
        ("path", path.as_str()),
        ("user_id", user_id.as_str()),
    ];
    state.logger.info(&context, "Received request");
    match state.store.find_by_id(id).await {
        Ok(Some(account)) => {
            state.logger.info(&context, "Request completed");
            Json(UserResponse {
                user_id: account.id.to_string(),
                email: account.email,
                created_at: java_instant(account.created_at),
            })
            .into_response()
        }
        Ok(None) => {
            state.logger.warn(&context, "User not found");
            json_error(StatusCode::NOT_FOUND, "User not found")
        }
        Err(error) => internal(&state, &context, error),
    }
}

/// How `java.time.Instant.toString` prints an instant: UTC, seconds always,
/// the fraction in 3, 6 or 9 digits as needed, then `Z`.
#[must_use]
pub fn java_instant(value: OffsetDateTime) -> String {
    let utc = value.to_offset(UtcOffset::UTC);
    let mut text = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        utc.year(),
        u8::from(utc.month()),
        utc.day(),
        utc.hour(),
        utc.minute(),
        utc.second()
    );
    let nanos = utc.nanosecond();
    let _ = if nanos == 0 {
        Ok(())
    } else if nanos.is_multiple_of(1_000_000) {
        write!(text, ".{:03}", nanos / 1_000_000)
    } else if nanos.is_multiple_of(1_000) {
        write!(text, ".{:06}", nanos / 1_000)
    } else {
        write!(text, ".{nanos:09}")
    };
    text.push('Z');
    text
}
