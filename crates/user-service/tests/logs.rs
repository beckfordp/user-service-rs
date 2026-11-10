// SPDX-License-Identifier: MIT

//! Logs end to end (RUST-3.4 spec FR1-FR4): the binary's stdout. Sources:
//! capture 20261109 stdout.log; user-service `Main`, `AuthRoutes`,
//! `UserRoutes`; product guidelines (no secrets in logs).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

mod support;

use std::thread::sleep;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use support::{Postgres, Service};

const PASSWORD: &str = "a-password-nobody-logs-1";

fn started(extra: &[(&'static str, String)]) -> (Postgres, Service) {
    let postgres = Postgres::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    env.extend(extra.iter().cloned());
    let service = Service::start(&env);
    (postgres, service)
}

/// Every stdout line as JSON, once one satisfies `until` (within 10 s).
fn json_lines(service: &mut Service, until: impl Fn(&Value) -> bool) -> Vec<Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let lines: Vec<Value> = service
            .stdout()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
            .collect();
        if lines.iter().any(&until) {
            return lines;
        }
        assert!(Instant::now() < deadline, "no such line in {lines:#?}");
        sleep(Duration::from_millis(50));
    }
}

fn post(service: &Service, path: &str, email: &str, password: &str, trace: &str) -> Value {
    let body = json!({"email": email, "password": password}).to_string();
    let traceparent = format!("00-{trace}-00f067aa0ba902b7-01");
    let (_, _, text) = service.request_with_body(
        "POST",
        path,
        &[
            ("Content-Type", "application/json"),
            ("traceparent", &traceparent),
        ],
        &body,
    );
    serde_json::from_str(&text).unwrap_or(Value::Null)
}

fn get(service: &Service, path: &str, trace: &str) {
    let traceparent = format!("00-{trace}-00f067aa0ba902b7-01");
    service.request("GET", path, &[("traceparent", &traceparent)]);
}

fn trace(n: u8) -> String {
    format!("{n:02x}{}", "0".repeat(30))
}

/// (message, level, `user_id`) of a trace's lines.
fn outline(lines: &[Value], trace_id: &str) -> Vec<(String, String, Option<String>)> {
    lines
        .iter()
        .filter(|line| line["trace_id"] == trace_id)
        .map(|line| {
            (
                line["message"].as_str().unwrap().to_owned(),
                line["level"].as_str().unwrap().to_owned(),
                line.get("user_id")
                    .map(|id| id.as_str().unwrap().to_owned()),
            )
        })
        .collect()
}

fn line(message: &str, level: &str, user: Option<&str>) -> (String, String, Option<String>) {
    (
        message.to_owned(),
        level.to_owned(),
        user.map(ToOwned::to_owned),
    )
}

#[test]
fn startup_names_the_ports() {
    let (_postgres, mut service) = started(&[]);
    let lines = json_lines(&mut service, |l| l["message"] == "user-service starting");
    let start = lines
        .iter()
        .find(|l| l["message"] == "user-service starting")
        .unwrap();
    assert_eq!(start["port"], service.port.to_string());
    assert_eq!(start["metrics_port"], service.metrics_port.to_string());
    assert_eq!(start["logger_name"], "user-service");
    assert_eq!(start["level"], "INFO");
}

#[test]
fn each_outcome_logs_as_the_scala_service_does() {
    let (_postgres, mut service) = started(&[]);
    let email = "kat@example.com";
    let created = post(&service, "/auth/signup", email, PASSWORD, &trace(1));
    let user_id = created["userId"].as_str().unwrap().to_owned();
    post(&service, "/auth/signup", email, PASSWORD, &trace(2));
    post(
        &service,
        "/auth/signup",
        "not-an-email",
        PASSWORD,
        &trace(3),
    );
    post(
        &service,
        "/auth/login",
        email,
        "wrong-password-1",
        &trace(4),
    );
    let login = post(&service, "/auth/login", email, PASSWORD, &trace(5));
    get(
        &service,
        &format!("/users/{}", user_id.to_uppercase()),
        &trace(6),
    );
    get(
        &service,
        "/users/00000000-0000-0000-0000-000000000000",
        &trace(7),
    );
    post(&service, "/auth/login", email, "", &trace(8));
    let lines = json_lines(&mut service, |l| {
        l["trace_id"] == trace(8) && l["level"] == "WARN"
    });
    let id = Some(user_id.as_str());
    let nobody = Some("00000000-0000-0000-0000-000000000000");
    let received = line("Received request", "INFO", None);
    assert_eq!(
        outline(&lines, &trace(1)),
        [received.clone(), line("Account created", "INFO", id)]
    );
    assert_eq!(
        outline(&lines, &trace(2)),
        [received.clone(), line("Duplicate signup", "WARN", None)]
    );
    assert_eq!(
        outline(&lines, &trace(3)),
        [
            received.clone(),
            line("Invalid signup request", "WARN", None)
        ]
    );
    assert_eq!(
        outline(&lines, &trace(4)),
        [received.clone(), line("Login failed", "WARN", None)]
    );
    assert_eq!(
        outline(&lines, &trace(5)),
        [received.clone(), line("Login succeeded", "INFO", id)]
    );
    assert_eq!(
        outline(&lines, &trace(6)),
        [
            line("Received request", "INFO", id),
            line("Request completed", "INFO", id)
        ]
    );
    assert_eq!(
        outline(&lines, &trace(7)),
        [
            line("Received request", "INFO", nobody),
            line("User not found", "WARN", nobody)
        ]
    );
    assert_eq!(
        outline(&lines, &trace(8)),
        [received, line("Invalid login request", "WARN", None)]
    );
    let invalid = lines
        .iter()
        .find(|l| l["message"] == "Invalid signup request")
        .unwrap();
    assert_eq!(invalid["error"], "Enter a valid email address");
    let canonical = lines
        .iter()
        .filter(|l| l["trace_id"] == trace(6))
        .map(|l| l["path"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        canonical,
        [format!("/users/{user_id}"), format!("/users/{user_id}")]
    );
    for l in &lines {
        if l["trace_id"] == trace(1) {
            assert_eq!(
                (&l["method"], &l["path"]),
                (&json!("POST"), &json!("/auth/signup"))
            );
            assert_eq!(l["span_id"].as_str().unwrap().len(), 16);
        }
    }

    let stdout = service.stdout();
    assert!(
        stdout.lines().all(|l| l.starts_with("{\"@timestamp\":")),
        "{stdout}"
    );
    for secret in [
        PASSWORD,
        created["token"].as_str().unwrap(),
        login["token"].as_str().unwrap(),
        login["refreshToken"].as_str().unwrap(),
    ] {
        assert!(!stdout.contains(secret), "{secret} logged");
    }
}

#[test]
fn undecodable_requests_log_nothing() {
    let (_postgres, mut service) = started(&[]);
    let traceparent = format!("00-{}-00f067aa0ba902b7-01", trace(9));
    service.request_with_body(
        "POST",
        "/auth/signup",
        &[
            ("Content-Type", "application/json"),
            ("traceparent", &traceparent),
        ],
        "{not json",
    );
    get(&service, "/users/not-a-uuid", &trace(9));
    post(
        &service,
        "/auth/signup",
        "kat@example.com",
        PASSWORD,
        &trace(10),
    );
    let lines = json_lines(&mut service, |l| l["message"] == "Account created");
    assert_eq!(outline(&lines, &trace(9)), []);
}

#[test]
fn the_logger_is_named_after_the_service() {
    let (_postgres, mut service) =
        started(&[("USER_SERVICE_NAME", "renamed-user-service".to_owned())]);
    let lines = json_lines(&mut service, |l| l["message"] == "user-service starting");
    assert!(
        lines
            .iter()
            .all(|l| l["logger_name"] == "renamed-user-service"),
        "{lines:#?}"
    );
}
