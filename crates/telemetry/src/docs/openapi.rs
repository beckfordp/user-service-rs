// SPDX-License-Identifier: MIT

//! An OpenAPI 3.1 document in the shape tapir's `OpenAPIDocsInterpreter`
//! gives purerest services (capture 20261109 openapi.yaml): operations in
//! declaration order, tapir's default operation ids, component schemas
//! derived from the DTO types (doc comments as descriptions), and error
//! variants sharing a status merged as tapir merges them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::{Map, Value, json};

use super::yaml;

/// One response variant: status, description, and a JSON body (schema
/// component name and example), if any.
#[derive(Debug, Clone)]
struct Variant {
    status: u16,
    description: String,
    body: Option<(String, Value)>,
}

/// One operation, described as tapir endpoints are.
#[derive(Debug, Clone)]
pub struct Operation {
    method: &'static str,
    path: String,
    tags: Vec<String>,
    summary: String,
    description: String,
    parameters: Vec<Value>,
    request: Option<(String, Value)>,
    responses: Vec<Variant>,
}

impl Operation {
    fn new(method: &'static str, path: &str) -> Self {
        Self {
            method,
            path: path.to_owned(),
            tags: Vec::new(),
            summary: String::new(),
            description: String::new(),
            parameters: Vec::new(),
            request: None,
            responses: Vec::new(),
        }
    }

    /// A GET operation on `path` (`{name}` for a path parameter).
    #[must_use]
    pub fn get(path: &str) -> Self {
        Self::new("get", path)
    }

    /// A POST operation on `path`.
    #[must_use]
    pub fn post(path: &str) -> Self {
        Self::new("post", path)
    }

    /// Adds a tag.
    #[must_use]
    pub fn tag(mut self, tag: &str) -> Self {
        self.tags.push(tag.to_owned());
        self
    }

    /// Sets the summary.
    #[must_use]
    pub fn summary(mut self, summary: &str) -> Self {
        summary.clone_into(&mut self.summary);
        self
    }

    /// Sets the description.
    #[must_use]
    pub fn description(mut self, description: &str) -> Self {
        description.clone_into(&mut self.description);
        self
    }

    /// A required string path parameter.
    #[must_use]
    pub fn path_parameter(mut self, name: &str, description: &str, example: Value) -> Self {
        let mut parameter = Map::new();
        parameter.insert("name".into(), json!(name));
        parameter.insert("in".into(), json!("path"));
        parameter.insert("description".into(), json!(description));
        parameter.insert("required".into(), json!(true));
        parameter.insert("schema".into(), json!({"type": "string"}));
        parameter.insert("example".into(), example);
        self.parameters.push(Value::Object(parameter));
        self
    }

    /// A required JSON request body: its schema component and an example.
    #[must_use]
    pub fn request_body(mut self, schema: &str, example: Value) -> Self {
        self.request = Some((schema.to_owned(), example));
        self
    }

    /// A response variant. Several on one status merge as tapir merges them:
    /// the first one's description and example, `anyOf` every one's schema.
    #[must_use]
    pub fn response(mut self, status: u16, description: &str, body: Option<(&str, Value)>) -> Self {
        self.responses.push(Variant {
            status,
            description: description.to_owned(),
            body: body.map(|(schema, example)| (schema.to_owned(), example)),
        });
        self
    }

    fn to_value(&self) -> Value {
        let mut operation = Map::new();
        operation.insert("tags".into(), json!(self.tags));
        operation.insert("summary".into(), json!(self.summary));
        operation.insert("description".into(), json!(self.description));
        operation.insert(
            "operationId".into(),
            json!(operation_id(self.method, &self.path)),
        );
        if !self.parameters.is_empty() {
            operation.insert("parameters".into(), Value::Array(self.parameters.clone()));
        }
        if let Some((schema, example)) = &self.request {
            operation.insert(
                "requestBody".into(),
                json!({"content": json_content(schema_ref(schema), example), "required": true}),
            );
        }
        let mut responses = Map::new();
        let mut statuses: Vec<u16> = Vec::new();
        for variant in &self.responses {
            if !statuses.contains(&variant.status) {
                statuses.push(variant.status);
            }
        }
        for status in statuses {
            let variants: Vec<&Variant> = self
                .responses
                .iter()
                .filter(|v| v.status == status)
                .collect();
            let first = variants[0];
            let mut response = Map::new();
            response.insert("description".into(), json!(first.description));
            let bodies: Vec<&(String, Value)> =
                variants.iter().filter_map(|v| v.body.as_ref()).collect();
            if let Some((schema, example)) = bodies.first() {
                let schema = if bodies.len() == 1 {
                    schema_ref(schema)
                } else {
                    json!({"anyOf": bodies.iter().map(|(s, _)| schema_ref(s)).collect::<Vec<_>>()})
                };
                response.insert("content".into(), json_content(schema, example));
            }
            responses.insert(status.to_string(), Value::Object(response));
        }
        operation.insert("responses".into(), Value::Object(responses));
        Value::Object(operation)
    }
}

fn schema_ref(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

fn json_content(schema: Value, example: &Value) -> Value {
    let mut media = Map::new();
    media.insert("schema".into(), schema);
    media.insert("example".into(), example.clone());
    let mut content = Map::new();
    content.insert("application/json".into(), Value::Object(media));
    Value::Object(content)
}

/// tapir's default operation id: the method, then each path segment
/// capitalised (`{id}` as `Id`): `post /auth/signup` → `postAuthSignup`.
#[must_use]
pub fn operation_id(method: &str, path: &str) -> String {
    let mut id = method.to_ascii_lowercase();
    for segment in path.split('/').filter(|s| !s.is_empty()) {
        let name = segment.trim_start_matches('{').trim_end_matches('}');
        let mut chars = name.chars();
        if let Some(first) = chars.next() {
            id.extend(first.to_uppercase());
            id.push_str(chars.as_str());
        }
    }
    id
}

/// The document: title, version, operations and component schemas.
#[derive(Debug, Clone)]
pub struct Api {
    title: String,
    version: String,
    operations: Vec<Operation>,
    schemas: BTreeMap<String, Value>,
}

impl Api {
    /// An empty document.
    #[must_use]
    pub fn new(title: &str, version: &str) -> Self {
        Self {
            title: title.to_owned(),
            version: version.to_owned(),
            operations: Vec::new(),
            schemas: BTreeMap::new(),
        }
    }

    /// Adds an operation (documents keep declaration order).
    #[must_use]
    pub fn operation(mut self, operation: Operation) -> Self {
        self.operations.push(operation);
        self
    }

    /// Adds `T`'s schema as a component, named after the type.
    #[must_use]
    pub fn schema<T: JsonSchema>(mut self) -> Self {
        let mut schema = SchemaSettings::draft2020_12()
            .into_generator()
            .root_schema_for::<T>()
            .to_value();
        if let Value::Object(map) = &mut schema {
            map.remove("$schema");
        }
        self.schemas.insert(T::schema_name().into_owned(), schema);
        self
    }

    /// The document as JSON.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let mut paths = Map::new();
        for operation in &self.operations {
            let item = paths
                .entry(operation.path.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(methods) = item {
                methods.insert(operation.method.to_owned(), operation.to_value());
            }
        }
        json!({
            "openapi": "3.1.0",
            "info": {"title": self.title, "version": self.version},
            "paths": paths,
            "components": {"schemas": self.schemas},
        })
    }

    /// The document as YAML, as served at `/docs/docs.yaml`.
    #[must_use]
    pub fn to_yaml(&self) -> String {
        yaml::to_yaml(&self.to_value())
    }
}
