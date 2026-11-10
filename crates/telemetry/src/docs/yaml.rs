// SPDX-License-Identifier: MIT

//! YAML for the served OpenAPI document. Block style, every string quoted:
//! an unquoted `2026-10-28T12:00:00Z` or `yes` would read back as a timestamp
//! or a boolean in YAML 1.1 readers (`PyYAML`, js-yaml's default schema), where
//! tapir's spec has strings.

use serde_json::Value;

/// `value` as a YAML document.
#[must_use]
pub fn to_yaml(value: &Value) -> String {
    let mut out = String::new();
    match value {
        Value::Object(map) if !map.is_empty() => write_map(&mut out, map, 0),
        Value::Array(items) if !items.is_empty() => write_seq(&mut out, items, 0),
        other => {
            out.push_str(&scalar(other));
            out.push('\n');
        }
    }
    out
}

fn indent(out: &mut String, level: usize) {
    out.extend(std::iter::repeat_n(' ', level));
}

fn write_map(out: &mut String, map: &serde_json::Map<String, Value>, level: usize) {
    for (key, value) in map {
        indent(out, level);
        out.push_str(&key_text(key));
        out.push(':');
        write_value(out, value, level);
    }
}

fn write_seq(out: &mut String, items: &[Value], level: usize) {
    for item in items {
        indent(out, level);
        out.push('-');
        match item {
            Value::Object(map) if !map.is_empty() => {
                // The first key on the dash's line, the rest under it.
                let mut first = true;
                for (key, value) in map {
                    if first {
                        out.push(' ');
                        first = false;
                    } else {
                        indent(out, level + 2);
                    }
                    out.push_str(&key_text(key));
                    out.push(':');
                    write_value(out, value, level + 2);
                }
            }
            Value::Array(inner) if !inner.is_empty() => {
                out.push('\n');
                write_seq(out, inner, level + 2);
            }
            other => {
                out.push(' ');
                out.push_str(&scalar(other));
                out.push('\n');
            }
        }
    }
}

/// After `key:` — a nested block on the next lines, or a scalar on this one.
/// Sequences sit at the key's own indentation, as tapir's YAML has them.
fn write_value(out: &mut String, value: &Value, level: usize) {
    match value {
        Value::Object(map) if !map.is_empty() => {
            out.push('\n');
            write_map(out, map, level + 2);
        }
        Value::Array(items) if !items.is_empty() => {
            out.push('\n');
            write_seq(out, items, level);
        }
        other => {
            out.push(' ');
            out.push_str(&scalar(other));
            out.push('\n');
        }
    }
}

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => quoted(s),
        Value::Array(_) => "[]".to_owned(),
        Value::Object(_) => "{}".to_owned(),
    }
}

/// Single-quoted (`''` for a quote), or JSON-style double-quoted when the
/// text has a line break or a control character.
fn quoted(text: &str) -> String {
    if text.chars().any(char::is_control) {
        // A JSON string is a valid YAML double-quoted scalar.
        serde_json::to_string(text).unwrap_or_default()
    } else {
        format!("'{}'", text.replace('\'', "''"))
    }
}

/// Keys stay plain when nothing could misread them (`description`, `$ref`,
/// `application/json`); anything else is quoted.
fn key_text(key: &str) -> String {
    let plain = key
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '$' || c == '_')
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '$' | '_' | '-' | '.' | '/'))
        && !matches!(
            key.to_ascii_lowercase().as_str(),
            "y" | "n" | "yes" | "no" | "on" | "off" | "true" | "false" | "null"
        );
    if plain { key.to_owned() } else { quoted(key) }
}
