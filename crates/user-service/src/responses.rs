// SPDX-License-Identifier: MIT

//! The two error shapes on the wire (capture 20261109): JSON `{"error": …}` for
//! domain errors, and tapir's plain text for framework errors.

use axum::extract::{FromRequest, Request};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::de::DeserializeOwned;

/// Exactly what the Scala service sends for text errors (not axum's `utf-8`).
pub const TEXT_PLAIN: &str = "text/plain; charset=UTF-8";

/// A framework error as tapir writes it: plain text.
#[derive(Debug, Clone)]
pub struct TextError {
    /// The status.
    pub status: StatusCode,
    /// The whole body.
    pub message: String,
}

impl IntoResponse for TextError {
    fn into_response(self) -> Response {
        (
            self.status,
            [(header::CONTENT_TYPE, TEXT_PLAIN)],
            self.message,
        )
            .into_response()
    }
}

/// Error body.
#[derive(Debug, serde::Serialize, schemars::JsonSchema)]
pub struct ErrorResponse {
    /// Human-readable reason.
    #[schemars(example = &"Enter a valid email address")]
    pub error: String,
}

/// A domain error: `{"error": "<message>"}` with `application/json`.
pub fn json_error(status: StatusCode, message: impl std::fmt::Display) -> Response {
    (
        status,
        axum::Json(ErrorResponse {
            error: message.to_string(),
        }),
    )
        .into_response()
}

/// A JSON request body, read whatever the Content-Type says (capture:
/// `text/plain` and no Content-Type both succeed). Undecodable →
/// 400 `Invalid value for: body (<detail>)`, as tapir; the detail is the
/// JSON library's own wording (user's decision, 2026-11-09).
pub struct JsonBody<T>(pub T);

impl<S, T> FromRequest<S> for JsonBody<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = TextError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|error| TextError {
                status: StatusCode::BAD_REQUEST,
                message: format!("Invalid value for: body ({error})"),
            })?;
        serde_json::from_slice(&bytes)
            .map(JsonBody)
            .map_err(|error| TextError {
                status: StatusCode::BAD_REQUEST,
                message: format!("Invalid value for: body ({error})"),
            })
    }
}
