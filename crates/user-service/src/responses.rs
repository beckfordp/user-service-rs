// SPDX-License-Identifier: MIT

//! The two error shapes on the wire (capture 20261109): JSON `{"error": …}` for
//! domain errors, and tapir's plain text for framework errors.

use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::de::DeserializeOwned;

/// tapir's plain text for framework errors, shared by every service (gluon-rs
/// `service-basics`).
pub use service_basics::text_error::{TEXT_PLAIN, TextError};

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
