// SPDX-License-Identifier: MIT

//! API documentation as purerest serves it (`Docs.routes`, tapir's
//! `SwaggerInterpreter`; RUST-4.1): an OpenAPI document built from the same
//! operation and DTO definitions the service is written from, as YAML, and
//! Swagger UI around it.

pub mod asyncapi;
pub mod openapi;
pub mod swagger;
pub mod yaml;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// tapir's answer to a known path with another method (HEAD included):
/// 405, `text/plain; charset=UTF-8`, "Method Not Allowed".
pub async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        [(header::CONTENT_TYPE, "text/plain; charset=UTF-8")],
        "Method Not Allowed",
    )
        .into_response()
}
