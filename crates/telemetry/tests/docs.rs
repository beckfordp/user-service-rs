// SPDX-License-Identifier: MIT

//! API docs as purerest serves them through tapir (RUST-4.1 spec FR1-FR3):
//! the OpenAPI builder, its YAML, and Swagger UI. Sources: capture 20261109
//! openapi.yaml, http/docs*.txt; tapir 1.11.x `SwaggerUI`, `tapir-files`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use schemars::JsonSchema;
use serde_json::{Value, json};
use telemetry::docs::openapi::{Api, Operation, operation_id};
use telemetry::docs::{swagger, yaml};
use tower::ServiceExt;

#[test]
fn yaml_quotes_every_string_so_yaml_1_1_readers_agree() {
    let value = json!({
        "openapi": "3.1.0",
        "info": {"title": "User Service", "version": "1.0"},
        "paths": {"/a/{id}": {"get": {
            "tags": ["auth"],
            "responses": {"200": {"description": ""}},
            "example": {"createdAt": "2026-10-28T12:00:00Z", "flag": "yes", "it's": "don't"}
        }}},
        "n": 1,
        "b": true,
        "none": null,
        "empty": [],
        "nothing": {},
        "list": [{"$ref": "#/x"}, {"$ref": "#/y"}],
        "multi": "two\nlines"
    });
    assert_eq!(
        yaml::to_yaml(&value),
        "openapi: '3.1.0'
info:
  title: 'User Service'
  version: '1.0'
paths:
  '/a/{id}':
    get:
      tags:
      - 'auth'
      responses:
        '200':
          description: ''
      example:
        createdAt: '2026-10-28T12:00:00Z'
        flag: 'yes'
        'it''s': 'don''t'
'n': 1
b: true
none: null
empty: []
nothing: {}
list:
- $ref: '#/x'
- $ref: '#/y'
multi: \"two\\nlines\"
"
    );
}

#[test]
fn operation_ids_are_tapirs_defaults() {
    assert_eq!(operation_id("post", "/auth/signup"), "postAuthSignup");
    assert_eq!(operation_id("get", "/users/{id}"), "getUsersId");
    assert_eq!(operation_id("get", "/health/ready"), "getHealthReady");
    assert_eq!(operation_id("get", "/health"), "getHealth");
}

/// A test DTO: doc comments become descriptions, as tapir's `@description`.
#[derive(JsonSchema)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct ThingResponse {
    /// The thing's id.
    #[schemars(example = &"abc")]
    thing_id: String,
    /// When it was made.
    #[schemars(extend("format" = "date-time"), example = &"2026-10-28T12:00:00Z")]
    created_at: String,
}

/// Error body.
#[derive(JsonSchema)]
#[allow(dead_code)]
struct ErrorResponse {
    /// Human-readable reason.
    #[schemars(example = &"Nope")]
    error: String,
}

#[test]
fn the_document_has_tapirs_shape() {
    let api = Api::new("Thing Service", "1.0")
        .schema::<ThingResponse>()
        .schema::<ErrorResponse>()
        .operation(
            Operation::get("/things/{id}")
                .tag("things")
                .summary("Get a thing")
                .description("Returns one thing.")
                .path_parameter("id", "Thing id.", json!("abc"))
                .response(200, "", Some(("ThingResponse", json!({"thingId": "abc"}))))
                .response(
                    400,
                    "Id is invalid.",
                    Some(("ErrorResponse", json!({"error": "Bad id"}))),
                )
                .response(
                    400,
                    "Id is too long.",
                    Some(("ErrorResponse", json!({"error": "Too long"}))),
                )
                .response(
                    404,
                    "No such thing.",
                    Some(("ErrorResponse", json!({"error": "Nope"}))),
                ),
        )
        .operation(
            Operation::post("/things")
                .tag("things")
                .summary("Make a thing")
                .description("Makes one.")
                .request_body("ThingResponse", json!({"thingId": "abc"}))
                .response(201, "", None)
                .response(503, "Down.", None),
        );
    assert_eq!(
        api.to_value(),
        json!({
            "openapi": "3.1.0",
            "info": {"title": "Thing Service", "version": "1.0"},
            "paths": {
                "/things/{id}": {"get": {
                    "tags": ["things"],
                    "summary": "Get a thing",
                    "description": "Returns one thing.",
                    "operationId": "getThingsId",
                    "parameters": [{
                        "name": "id", "in": "path", "description": "Thing id.",
                        "required": true, "schema": {"type": "string"}, "example": "abc"
                    }],
                    "responses": {
                        "200": {"description": "", "content": {"application/json": {
                            "schema": {"$ref": "#/components/schemas/ThingResponse"},
                            "example": {"thingId": "abc"}}}},
                        "400": {"description": "Id is invalid.", "content": {"application/json": {
                            "schema": {"anyOf": [
                                {"$ref": "#/components/schemas/ErrorResponse"},
                                {"$ref": "#/components/schemas/ErrorResponse"}]},
                            "example": {"error": "Bad id"}}}},
                        "404": {"description": "No such thing.", "content": {"application/json": {
                            "schema": {"$ref": "#/components/schemas/ErrorResponse"},
                            "example": {"error": "Nope"}}}}
                    }
                }},
                "/things": {"post": {
                    "tags": ["things"],
                    "summary": "Make a thing",
                    "description": "Makes one.",
                    "operationId": "postThings",
                    "requestBody": {
                        "content": {"application/json": {
                            "schema": {"$ref": "#/components/schemas/ThingResponse"},
                            "example": {"thingId": "abc"}}},
                        "required": true
                    },
                    "responses": {"201": {"description": ""}, "503": {"description": "Down."}}
                }}
            },
            "components": {"schemas": {
                "ErrorResponse": {
                    "title": "ErrorResponse", "description": "Error body.", "type": "object",
                    "required": ["error"],
                    "properties": {"error": {"description": "Human-readable reason.",
                        "examples": ["Nope"], "type": "string"}}
                },
                "ThingResponse": {
                    "title": "ThingResponse", "description": "A test DTO: doc comments become descriptions, as tapir's `@description`.",
                    "type": "object",
                    "required": ["thingId", "createdAt"],
                    "properties": {
                        "thingId": {"description": "The thing's id.", "examples": ["abc"], "type": "string"},
                        "createdAt": {"description": "When it was made.", "examples": ["2026-10-28T12:00:00Z"],
                            "type": "string", "format": "date-time"}
                    }
                }
            }}
        })
    );
    let document = api.to_value();
    let paths: Vec<&String> = document["paths"].as_object().unwrap().keys().collect();
    assert_eq!(
        paths,
        ["/things/{id}", "/things"],
        "operations keep their order"
    );
}

async fn get(app: &axum::Router, uri: &str, headers: &[(&str, &str)]) -> axum::response::Response {
    let mut request = Request::builder().uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body(response: axum::response::Response) -> Vec<u8> {
    to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec()
}

fn header(response: &axum::response::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_owned())
}

fn app() -> axum::Router {
    swagger::router("openapi: '3.1.0'\n".to_owned())
}

#[tokio::test]
async fn docs_redirects_relatively_and_keeps_the_query() {
    let response = get(&app(), "/docs", &[]).await;
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(header(&response, "location").as_deref(), Some("./docs/"));
    assert_eq!(body(response).await, b"");
    let response = get(&app(), "/docs?urls.primaryName=user", &[]).await;
    assert_eq!(
        header(&response, "location").as_deref(),
        Some("./docs/?urls.primaryName=user")
    );
}

#[tokio::test]
async fn the_page_is_swagger_ui_5_20_7s_with_tapirs_headers() {
    for path in ["/docs/", "/docs/index.html"] {
        let response = get(&app(), path, &[]).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            header(&response, "content-type").as_deref(),
            Some("text/html")
        );
        assert_eq!(
            header(&response, "etag").as_deref(),
            Some("\"196104dbfc8-2de\"")
        );
        assert_eq!(
            header(&response, "last-modified").as_deref(),
            Some("Mon, 07 Apr 2025 12:50:53 GMT")
        );
        assert_eq!(header(&response, "accept-ranges").as_deref(), Some("bytes"));
        assert_eq!(
            body(response).await,
            include_bytes!("../assets/swagger-ui/index.html")
        );
    }
}

#[tokio::test]
async fn a_matching_etag_is_not_modified() {
    let response = get(
        &app(),
        "/docs/index.css",
        &[("if-none-match", "\"196104dbfc8-ca\"")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(body(response).await, b"");
    let response = get(&app(), "/docs/index.css", &[("if-none-match", "\"other\"")]).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn assets_have_tapirs_content_types() {
    for (path, content_type) in [
        (
            "/docs/swagger-ui-bundle.js",
            "application/javascript; charset=UTF-8",
        ),
        (
            "/docs/swagger-ui-standalone-preset.js",
            "application/javascript; charset=UTF-8",
        ),
        ("/docs/swagger-ui.css", "text/css; charset=UTF-8"),
        ("/docs/index.css", "text/css; charset=UTF-8"),
        ("/docs/favicon-32x32.png", "image/png"),
        ("/docs/oauth2-redirect.html", "text/html"),
    ] {
        let response = get(&app(), path, &[]).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        assert_eq!(
            header(&response, "content-type").as_deref(),
            Some(content_type),
            "{path}"
        );
        assert!(!body(response).await.is_empty(), "{path}");
    }
}

#[tokio::test]
async fn the_initializer_loads_docs_yaml_without_extensions() {
    let response = get(&app(), "/docs/swagger-initializer.js", &[]).await;
    assert_eq!(
        header(&response, "content-type").as_deref(),
        Some("text/javascript; charset=UTF-8")
    );
    let text = String::from_utf8(body(response).await).unwrap();
    assert!(text.contains("url: \"./docs.yaml\","), "{text}");
    assert!(
        text.contains("window.ui = SwaggerUIBundle({\n    showExtensions: false,\n"),
        "{text}"
    );
    assert!(!text.contains("petstore"));
}

#[tokio::test]
async fn the_spec_is_served_as_yaml() {
    let response = get(&app(), "/docs/docs.yaml", &[]).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header(&response, "content-type").as_deref(),
        Some("application/yaml")
    );
    assert_eq!(body(response).await, b"openapi: '3.1.0'\n");
}

#[tokio::test]
async fn an_unknown_file_is_an_empty_404() {
    for path in [
        "/docs/no-such-file.js",
        "/docs/swagger-ui-bundle.js.map",
        "/docs/a/b",
    ] {
        let response = get(&app(), path, &[]).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        assert!(body(response).await.is_empty(), "{path}");
    }
}

#[tokio::test]
async fn head_is_not_allowed() {
    for path in ["/docs/docs.yaml", "/docs/", "/docs/index.html", "/docs"] {
        let response = app()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED, "{path}");
    }
}

#[test]
fn serde_json_keeps_insertion_order() {
    // The document's key order is tapir's (preserve_order); a sanity check.
    let value: Value = serde_json::from_str(r#"{"b": 1, "a": 2}"#).unwrap();
    assert_eq!(
        value.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["b", "a"]
    );
}
