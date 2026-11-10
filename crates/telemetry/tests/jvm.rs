// SPDX-License-Identifier: MIT

//! `java.util.UUID.fromString` semantics (RUST-3.3 spec FR2, FR6), checked
//! against known-answer vectors from the JDK (`scripts/java-uuid-vectors.sh`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use telemetry::jvm::parse_uuid;

#[test]
fn parse_uuid_agrees_with_the_jdk_on_every_vector() {
    let text = include_str!("fixtures/java-uuid-vectors.jsonl");
    let mut lines = text.lines();
    let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert!(header["jdk"].is_string(), "first line names the JDK");
    let mut checked = 0;
    let mut disagreements = Vec::new();
    for line in lines {
        let vector: Value = serde_json::from_str(line).unwrap();
        let input = vector["input"].as_str().unwrap();
        let expected = vector["uuid"].as_str();
        let actual = parse_uuid(input).map(|uuid| uuid.to_string());
        if actual.as_deref() != expected {
            disagreements.push(format!("{input:?}: jdk {expected:?}, ours {actual:?}"));
        }
        checked += 1;
    }
    assert!(checked > 600, "{checked} vectors");
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

#[test]
fn the_canonical_form_round_trips() {
    let uuid = parse_uuid("691B03E8-F1D7-45D9-A561-5183FFAE9D0B").unwrap();
    assert_eq!(uuid.to_string(), "691b03e8-f1d7-45d9-a561-5183ffae9d0b");
}
