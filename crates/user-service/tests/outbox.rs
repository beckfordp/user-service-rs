// SPDX-License-Identifier: MIT

//! The outbox row written with the account (RUST-3.1 spec FR1): the binary
//! against a throwaway Postgres. Sources: ADR 0008; capture 20261109
//! db-outbox-events.jsonl.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use serde_json::{Value, json};
use support::{Postgres, Service};

fn started() -> (Postgres, Service) {
    let postgres = Postgres::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    let service = Service::start(&env);
    (postgres, service)
}

fn signup(service: &Service, email: &str) -> u16 {
    let body = json!({"email": email, "password": "correct-horse-1"}).to_string();
    service.post_json("/auth/signup", &body).0
}

#[test]
fn a_signup_writes_one_row_with_the_envelope() {
    let (postgres, service) = started();
    assert_eq!(signup(&service, " Kat@Example.com "), 201);
    let user_id = postgres.query(r#"SELECT id FROM "user_account""#);
    let user_id = user_id.trim();
    let rows = postgres.query(
        "SELECT aggregate_id, correlation_id, trace_parent ~ '^00-[0-9a-f]{32}-[0-9a-f]{16}-01$', \
         payload::text FROM outbox_events",
    );
    let rows: Vec<&str> = rows.lines().collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let columns: Vec<&str> = rows[0].splitn(4, '|').collect();
    assert_eq!(columns[..3], [user_id, user_id, "t"]);
    let envelope: Value = serde_json::from_str(columns[3]).unwrap();
    assert_eq!(envelope["eventType"], "UserRegistered");
    assert_eq!(
        envelope["payload"],
        json!({"userId": user_id, "email": "kat@example.com"})
    );
}

#[test]
fn a_refused_signup_writes_no_row() {
    let (postgres, service) = started();
    assert_eq!(signup(&service, "kat@example.com"), 201);
    assert_eq!(signup(&service, "KAT@example.com"), 409);
    assert_eq!(signup(&service, "not-an-email"), 400);
    assert_eq!(
        postgres.query("SELECT count(*) FROM outbox_events").trim(),
        "1"
    );
}
