// SPDX-License-Identifier: MIT

//! Swagger UI under `/docs`, as tapir's `SwaggerUI` serves it (1.11.x):
//! `/docs` redirects to `./docs/` (query kept), `docs.yaml` is the spec,
//! `swagger-initializer.js` is rewritten to load it, and the webjar's files
//! (`assets/swagger-ui`, vendored by `scripts/vendor-swagger-ui.sh`) are
//! served as `tapir-files` serves resources: content type by extension,
//! `ETag` `"<hex mtime ms>-<hex length>"`, `Last-Modified`, 304 on a matching
//! `If-None-Match`, an empty 404 for anything else under `/docs/`.

use axum::Router;
use axum::extract::{Path, RawQuery};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use time::OffsetDateTime;
use time::macros::format_description;

use super::method_not_allowed;

struct Asset {
    name: &'static str,
    bytes: &'static [u8],
}

const ASSETS: [Asset; 9] = [
    Asset {
        name: "index.html",
        bytes: include_bytes!("../../assets/swagger-ui/index.html"),
    },
    Asset {
        name: "index.css",
        bytes: include_bytes!("../../assets/swagger-ui/index.css"),
    },
    Asset {
        name: "swagger-initializer.js",
        bytes: include_bytes!("../../assets/swagger-ui/swagger-initializer.js"),
    },
    Asset {
        name: "swagger-ui.css",
        bytes: include_bytes!("../../assets/swagger-ui/swagger-ui.css"),
    },
    Asset {
        name: "swagger-ui-bundle.js",
        bytes: include_bytes!("../../assets/swagger-ui/swagger-ui-bundle.js"),
    },
    Asset {
        name: "swagger-ui-standalone-preset.js",
        bytes: include_bytes!("../../assets/swagger-ui/swagger-ui-standalone-preset.js"),
    },
    Asset {
        name: "favicon-16x16.png",
        bytes: include_bytes!("../../assets/swagger-ui/favicon-16x16.png"),
    },
    Asset {
        name: "favicon-32x32.png",
        bytes: include_bytes!("../../assets/swagger-ui/favicon-32x32.png"),
    },
    Asset {
        name: "oauth2-redirect.html",
        bytes: include_bytes!("../../assets/swagger-ui/oauth2-redirect.html"),
    },
];

/// When the webjar was published (its files' `Last-Modified`), from the
/// vendored MANIFEST.
fn published_ms() -> i128 {
    include_str!("../../assets/swagger-ui/MANIFEST")
        .lines()
        .find(|line| line.starts_with("index.html "))
        .and_then(|line| line.rsplit(' ').next())
        .and_then(|ms| ms.parse().ok())
        .unwrap_or(0)
}

/// The `tapir-files` content type for a file name (tapir's MIME table).
fn content_type(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html",
        "css" => "text/css; charset=UTF-8",
        "js" => "application/javascript; charset=UTF-8",
        "png" => "image/png",
        "json" | "map" => "application/json; charset=UTF-8",
        _ => "application/octet-stream",
    }
}

fn http_date(ms: i128) -> String {
    let format = format_description!(
        "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT"
    );
    OffsetDateTime::from_unix_timestamp_nanos(ms * 1_000_000)
        .ok()
        .and_then(|at| at.format(&format).ok())
        .unwrap_or_default()
}

fn serve(asset: &Asset, request_headers: &HeaderMap) -> Response {
    let modified = published_ms();
    let etag = format!("\"{modified:x}-{:x}\"", asset.bytes.len());
    let last_modified = http_date(modified);
    let matches = request_headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|tag| tag.trim() == etag || tag.trim() == "*");
    if matches {
        return (
            StatusCode::NOT_MODIFIED,
            [(header::ETAG, etag), (header::LAST_MODIFIED, last_modified)],
        )
            .into_response();
    }
    (
        [
            (header::LAST_MODIFIED, last_modified),
            (header::CONTENT_TYPE, content_type(asset.name).to_owned()),
            (header::ETAG, etag),
            (header::ACCEPT_RANGES, "bytes".to_owned()),
        ],
        asset.bytes,
    )
        .into_response()
}

fn file(name: &str, request_headers: &HeaderMap) -> Response {
    match ASSETS.iter().find(|asset| asset.name == name) {
        Some(asset) => serve(asset, request_headers),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The initializer as tapir rewrites it: the webjar's petstore URL replaced by
/// `./docs.yaml`, and `showExtensions: false` added.
fn initializer() -> String {
    let original = ASSETS
        .iter()
        .find(|asset| asset.name == "swagger-initializer.js")
        .map(|asset| String::from_utf8_lossy(asset.bytes).into_owned())
        .unwrap_or_default();
    original
        .replace("https://petstore.swagger.io/v2/swagger.json", "./docs.yaml")
        .replace(
            "window.ui = SwaggerUIBundle({",
            "window.ui = SwaggerUIBundle({\n    showExtensions: false,",
        )
}

/// The `/docs` routes, serving `yaml` as the spec.
pub fn router(yaml: String) -> Router {
    let initializer = initializer();
    Router::new()
        .route(
            "/docs",
            get(|RawQuery(query): RawQuery| async move {
                let location = match query {
                    Some(query) if !query.is_empty() => format!("./docs/?{query}"),
                    _ => "./docs/".to_owned(),
                };
                (
                    StatusCode::PERMANENT_REDIRECT,
                    [(header::LOCATION, location)],
                )
                    .into_response()
            })
            .head(method_not_allowed),
        )
        .route(
            "/docs/docs.yaml",
            get(move || async move {
                // No charset: what the Scala service sends (RUST-5.0 run), not
                // tapir's yamlEndpoint as read from its source.
                ([(header::CONTENT_TYPE, "application/yaml")], yaml).into_response()
            })
            .head(method_not_allowed),
        )
        .route(
            "/docs/swagger-initializer.js",
            get(move || async move {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=UTF-8")],
                    initializer,
                )
                    .into_response()
            })
            .head(method_not_allowed),
        )
        .route(
            "/docs/",
            get(|headers: HeaderMap| async move { file("index.html", &headers) })
                .head(method_not_allowed),
        )
        .route(
            "/docs/{*name}",
            get(
                |Path(name): Path<String>, headers: HeaderMap| async move { file(&name, &headers) },
            )
            .head(method_not_allowed),
        )
}
