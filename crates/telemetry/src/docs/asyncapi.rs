// SPDX-License-Identifier: MIT

//! AsyncAPI as purekafka gives Kafka services (RUST-4.2): the 3.0.0 document
//! `AsyncApiDoc.generate` builds from the service's channels, served at
//! `/docs/asyncapi.json` (`Docs.routes`, with `/docs/asyncapi` redirecting to
//! `/docs/asyncapi/`), and the HTML html-template renders from it, served from
//! a directory as user-service's `AppRoutes` serves it (http4s `fileService`).

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use axum::Router;
use axum::extract::Path as UrlPath;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use time::macros::format_description;

use super::method_not_allowed;

const CDC_DESCRIPTION: &str = "Published via Debezium CDC from an outbox table (ADR 0008 in gluon) - no direct producer call.";

/// Which way a channel's events go, from the service's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The service publishes them; `cdc` when through the outbox and Debezium.
    Publish {
        /// Published via the outbox (ADR 0008) rather than a producer call.
        cdc: bool,
    },
    /// The service consumes them.
    Subscribe,
}

/// One event type on one topic (purekafka `AsyncApiChannel`).
#[derive(Debug, Clone)]
pub struct Channel {
    topic: String,
    event_type: String,
    direction: Direction,
    payload: Value,
}

impl Channel {
    /// `T` published on `topic` through the outbox.
    #[must_use]
    pub fn publish_cdc<T: JsonSchema>(topic: &str, event_type: &str) -> Self {
        Self::new::<T>(topic, event_type, Direction::Publish { cdc: true })
    }

    /// `T` consumed from `topic`.
    #[must_use]
    pub fn subscribe<T: JsonSchema>(topic: &str, event_type: &str) -> Self {
        Self::new::<T>(topic, event_type, Direction::Subscribe)
    }

    fn new<T: JsonSchema>(topic: &str, event_type: &str, direction: Direction) -> Self {
        Self {
            topic: topic.to_owned(),
            event_type: event_type.to_owned(),
            direction,
            payload: payload_schema::<T>(),
        }
    }

    const fn sends(&self) -> bool {
        matches!(self.direction, Direction::Publish { .. })
    }
}

/// tapir's key order (`TapirSchemaToJsonSchema`): html-template renders
/// document order, so it shows in the HTML.
const KEY_ORDER: [&str; 9] = [
    "$schema",
    "title",
    "description",
    "examples",
    "type",
    "format",
    "required",
    "properties",
    "items",
];

fn tapir_order(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by_key(|(key, _)| {
                KEY_ORDER
                    .iter()
                    .position(|k| k == key)
                    .unwrap_or(KEY_ORDER.len())
            });
            let ordered = entries
                .into_iter()
                .map(|(key, value)| {
                    // Property names are data, not keywords: keep their order.
                    let value = if key == "properties" {
                        match value {
                            Value::Object(props) => Value::Object(
                                props
                                    .into_iter()
                                    .map(|(k, v)| (k, tapir_order(v)))
                                    .collect(),
                            ),
                            other => other,
                        }
                    } else {
                        tapir_order(value)
                    };
                    (key, value)
                })
                .collect();
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(tapir_order).collect()),
        other => other,
    }
}

/// `T`'s payload schema as tapir writes it for AsyncAPI: JSON Schema 2020-12
/// (`http://json-schema.org/draft/2020-12/schema#`), open (tapir states no
/// `additionalProperties`), in tapir's key order. The registry's copy is the
/// draft-07, closed one (`events::user_registered_schema` in the service).
#[must_use]
pub fn payload_schema<T: JsonSchema>() -> Value {
    let mut schema = SchemaSettings::draft2020_12()
        .into_generator()
        .root_schema_for::<T>()
        .to_value();
    if let Value::Object(map) = &mut schema {
        map.remove("additionalProperties");
        map.insert(
            "$schema".into(),
            json!("http://json-schema.org/draft/2020-12/schema#"),
        );
    }
    tapir_order(schema)
}

/// AsyncAPI 3.0 keys allow `[A-Za-z0-9_-]`; anything else becomes `_`.
fn key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn on<'a>(channels: &'a [Channel], topic: &'a str) -> impl Iterator<Item = &'a Channel> {
    channels.iter().filter(move |c| c.topic == topic)
}

/// The document for `channels`: one channel per topic (topics with a send
/// first), one operation per action per topic (`<topic>-send` /
/// `<topic>-receive` when a topic has both), sends before receives.
#[must_use]
pub fn document(title: &str, version: &str, channels: &[Channel]) -> Value {
    let mut topics: Vec<&str> = Vec::new();
    for channel in channels {
        if !topics.contains(&channel.topic.as_str()) {
            topics.push(&channel.topic);
        }
    }
    let (sending, receive_only): (Vec<&str>, Vec<&str>) = topics
        .iter()
        .partition(|topic| on(channels, topic).any(Channel::sends));
    let mut channel_map = Map::new();
    for topic in sending.iter().chain(&receive_only) {
        let mut messages = Map::new();
        for channel in on(channels, topic) {
            let mut message = Map::new();
            message.insert("name".into(), json!(channel.event_type));
            message.insert("title".into(), json!(channel.event_type));
            message.insert("payload".into(), channel.payload.clone());
            if channel.direction == (Direction::Publish { cdc: true }) {
                message.insert("description".into(), json!(CDC_DESCRIPTION));
            }
            messages
                .entry(key(&channel.event_type))
                .or_insert(Value::Object(message));
        }
        channel_map.insert(key(topic), json!({"address": topic, "messages": messages}));
    }
    let mut operations = Map::new();
    for (action, wanted) in [("send", true), ("receive", false)] {
        for topic in &topics {
            let chosen: Vec<&Channel> = on(channels, topic)
                .filter(|c| c.sends() == wanted)
                .collect();
            if chosen.is_empty() {
                continue;
            }
            let both =
                on(channels, topic).any(Channel::sends) && on(channels, topic).any(|c| !c.sends());
            let channel_key = key(topic);
            let operation_key = if both {
                format!("{channel_key}-{action}")
            } else {
                channel_key.clone()
            };
            let messages: Vec<Value> = chosen
                .iter()
                .map(|c| json!({"$ref": format!("#/channels/{channel_key}/messages/{}", key(&c.event_type))}))
                .collect();
            operations.insert(
                operation_key,
                json!({
                    "action": action,
                    "channel": {"$ref": format!("#/channels/{channel_key}")},
                    "messages": messages,
                }),
            );
        }
    }
    json!({
        "asyncapi": "3.0.0",
        "info": {"title": title, "version": version},
        "channels": channel_map,
        "operations": operations,
    })
}

fn http_date(ms: u128) -> String {
    let format = format_description!(
        "[weekday repr:short], [day] [month repr:short] [year] [hour]:[minute]:[second] GMT"
    );
    i128::try_from(ms)
        .ok()
        .and_then(|ms| OffsetDateTime::from_unix_timestamp_nanos(ms * 1_000_000).ok())
        .and_then(|at| at.format(&format).ok())
        .unwrap_or_default()
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
    {
        "html" => "text/html",
        "css" => "text/css",
        "js" => "text/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// The file `relative` names under `root`, if it is one and stays inside.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    if !relative
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
    {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let path = root.join(relative).canonicalize().ok()?;
    (path.starts_with(&root) && path.is_file()).then_some(path)
}

async fn file(root: &Path, relative: &str, ranges: bool) -> Response {
    let Some(path) = resolve(root, relative) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (Ok(bytes), Ok(metadata)) = (
        tokio::fs::read(&path).await,
        tokio::fs::metadata(&path).await,
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_millis());
    let mut response = (
        [
            (header::LAST_MODIFIED, http_date(modified)),
            (header::CONTENT_TYPE, content_type(&path).to_owned()),
            (header::ETAG, format!("\"{modified:x}-{:x}\"", bytes.len())),
        ],
        bytes,
    )
        .into_response();
    if ranges {
        response.headers_mut().insert(
            header::ACCEPT_RANGES,
            header::HeaderValue::from_static("bytes"),
        );
    }
    response
}

/// `/docs/asyncapi.json`, `/docs/asyncapi` and `/docs/asyncapi/…` (from
/// `html_dir`).
pub fn router(document: &Value, html_dir: PathBuf) -> Router {
    let json = serde_json::to_string(document).unwrap_or_default();
    let index_dir = html_dir.clone();
    Router::new()
        .route(
            "/docs/asyncapi.json",
            get(move || async move { ([(header::CONTENT_TYPE, "application/json")], json).into_response() })
                .head(method_not_allowed),
        )
        .route(
            "/docs/asyncapi",
            get(|| async { (StatusCode::MOVED_PERMANENTLY, [(header::LOCATION, "/docs/asyncapi/")]).into_response() })
                .head(method_not_allowed),
        )
        .route(
            "/docs/asyncapi/",
            get(move || async move { file(&index_dir, "index.html", false).await }).head(method_not_allowed),
        )
        .route(
            "/docs/asyncapi/{*path}",
            get(move |UrlPath(path): UrlPath<String>| async move { file(&html_dir, &path, true).await })
                .head(method_not_allowed),
        )
}
