// SPDX-License-Identifier: MIT

//! CORS exactly as the Scala service's http4s `CORS.policy` answers (capture
//! 20261109 http/cors-preflight.txt, cors-simple.txt): any origin, these five
//! methods, `Content-Type`, `X-Total-Count` exposed, no credentials. Only
//! requests with an `Origin` get CORS headers.

use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// The middleware.
pub async fn cors(request: Request, next: Next) -> Response {
    let has_origin = request.headers().contains_key(header::ORIGIN);
    if has_origin
        && request.method() == Method::OPTIONS
        && request
            .headers()
            .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD)
    {
        return (
            StatusCode::OK,
            [
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
                (
                    header::ACCESS_CONTROL_ALLOW_METHODS,
                    "PATCH, PUT, GET, POST, DELETE",
                ),
                (header::ACCESS_CONTROL_ALLOW_HEADERS, "Content-Type"),
                (
                    header::VARY,
                    "Access-Control-Request-Method, Access-Control-Request-Headers",
                ),
            ],
        )
            .into_response();
    }
    let mut response = next.run(request).await;
    if has_origin {
        let headers = response.headers_mut();
        headers.insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        );
        headers.insert(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            HeaderValue::from_static("X-Total-Count"),
        );
    }
    response
}
