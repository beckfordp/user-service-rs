// SPDX-License-Identifier: MIT

//! Metrics end to end (RUST-3.3 spec FR1-FR6): the binary's scrape after
//! traffic like the capture's. Sources: capture 20261109 metrics.txt,
//! http/users-*.txt; purerest `ServerMetrics`, `PoolMetrics`; user-service
//! `UserStore`, `UserJourneyMetrics`; the JDK's `UUID.fromString`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines
)]

mod support;

use serde_json::{Value, json};
use support::{Postgres, Service};

fn started(extra: &[(&'static str, String)]) -> (Postgres, Service) {
    let postgres = Postgres::start();
    let mut env = postgres.env();
    env.push(("AUTH_PBKDF2_ITERATIONS", "1000".to_owned()));
    env.extend(extra.iter().cloned());
    let service = Service::start(&env);
    (postgres, service)
}

fn scrape(service: &Service) -> String {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(format!("http://127.0.0.1:{}/metrics", service.metrics_port))
        .call()
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "text/plain; version=0.0.4; charset=utf-8"
    );
    response.body_mut().read_to_string().unwrap()
}

/// `(labels, value)` of each sample named exactly `name`.
fn samples(text: &str, name: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.strip_prefix(name))
        .filter_map(|rest| rest.strip_prefix('{'))
        .filter_map(|rest| rest.split_once("} "))
        .map(|(labels, value)| (labels.to_owned(), value.to_owned()))
        .collect()
}

fn credentials(email: &str, password: &str) -> String {
    json!({"email": email, "password": password}).to_string()
}

#[test]
fn traffic_like_the_captures_is_recorded_as_the_scala_service_records_it() {
    let (_postgres, service) = started(&[]);
    let (_, _, created) = service.post_json(
        "/auth/signup",
        &credentials("kat@example.com", "correct-horse-1"),
    );
    let user_id = serde_json::from_str::<Value>(&created).unwrap()["userId"]
        .as_str()
        .unwrap()
        .to_owned();
    service.post_json(
        "/auth/signup",
        &credentials("kat@example.com", "correct-horse-1"),
    );
    service.post_json(
        "/auth/login",
        &credentials("kat@example.com", "wrong-password-1"),
    );
    service.post_json(
        "/auth/login",
        &credentials("kat@example.com", "correct-horse-1"),
    );
    service.get(&format!("/users/{user_id}")).unwrap();
    service.get("/users/not-a-uuid").unwrap();
    service.get("/no-such-path").unwrap();
    service.get("/health/ready").unwrap();
    service.request(
        "OPTIONS",
        "/auth/signup",
        &[
            ("Origin", "http://example.com"),
            ("Access-Control-Request-Method", "POST"),
        ],
    );
    let text = scrape(&service);
    let scope = "otel_scope_name=\"user-service\"";

    let http: Vec<String> = samples(&text, "http_server_request_duration_seconds_count")
        .into_iter()
        .map(|(labels, value)| format!("{labels} {value}"))
        .collect();
    for expected in [
        format!(
            "http_request_method=\"GET\",http_response_status_code=\"200\",http_route=\"/users/{{id}}\",{scope} 1"
        ),
        format!(
            "http_request_method=\"GET\",http_response_status_code=\"400\",http_route=\"/users/not-a-uuid\",{scope} 1"
        ),
        format!(
            "http_request_method=\"POST\",http_response_status_code=\"201\",http_route=\"/auth/signup\",{scope} 1"
        ),
        format!(
            "http_request_method=\"POST\",http_response_status_code=\"409\",http_route=\"/auth/signup\",{scope} 1"
        ),
        format!(
            "http_request_method=\"POST\",http_response_status_code=\"401\",http_route=\"/auth/login\",{scope} 1"
        ),
        format!("http_request_method=\"GET\",http_route=\"/no-such-path\",{scope} 1"),
    ] {
        assert!(http.contains(&expected), "{expected} not in {http:#?}");
    }
    assert!(http.iter().all(|s| !s.contains("OPTIONS")), "{http:#?}");

    let db: Vec<String> = samples(&text, "db_client_operation_duration_seconds_count")
        .into_iter()
        .map(|(labels, value)| format!("{labels} {value}"))
        .collect();
    assert_eq!(
        db,
        [
            format!("db_operation=\"insert\",db_system=\"postgresql\",{scope} 3"),
            format!("db_operation=\"ping\",db_system=\"postgresql\",{scope} 1"),
            format!("db_operation=\"select\",db_system=\"postgresql\",{scope} 3"),
        ]
    );

    let pool = format!("db_client_connection_pool_name=\"user\",{scope}");
    assert_eq!(
        samples(&text, "db_client_connection_count"),
        [(
            format!(
                "db_client_connection_pool_name=\"user\",db_client_connection_state=\"used\",{scope}"
            ),
            "0.0".to_owned()
        )]
    );
    assert_eq!(
        samples(&text, "db_client_connection_pending_requests"),
        [(pool.clone(), "0.0".to_owned())]
    );
    assert_eq!(
        samples(&text, "db_client_connection_wait_time_seconds_count"),
        [(pool, "7".to_owned())]
    );
    assert_eq!(
        samples(&text, "users_registrations_total"),
        [
            (format!("{scope},outcome=\"created\""), "1.0".to_owned()),
            (format!("{scope},outcome=\"duplicate\""), "1.0".to_owned()),
        ]
    );
    assert_eq!(
        samples(&text, "users_logins_total"),
        [(scope.to_owned(), "1.0".to_owned())]
    );
    for family in [
        "db_client_connection_count gauge",
        "db_client_connection_pending_requests gauge",
        "db_client_connection_wait_time_seconds histogram",
        "db_client_operation_duration_seconds histogram",
        "http_server_request_duration_seconds histogram",
        "users_logins_total counter",
        "users_registrations_total counter",
    ] {
        assert!(
            text.contains(&format!("# TYPE {family}\n")),
            "{family}: {text}"
        );
    }
}

#[test]
fn a_failed_operation_carries_its_error_type() {
    let (postgres, service) = started(&[]);
    postgres.stop();
    let (status, _, _) = service.get("/health/ready").unwrap();
    assert_eq!(status, 503);
    let text = scrape(&service);
    let ping = samples(&text, "db_client_operation_duration_seconds_count");
    assert!(
        ping.iter()
            .any(|(labels, _)| labels.contains("db_operation=\"ping\"")
                && labels.contains("error_type=\"")),
        "{ping:#?}"
    );
    postgres.start_again();
}

#[test]
fn the_scope_is_the_service_name() {
    let (_postgres, service) = started(&[("USER_SERVICE_NAME", "renamed-user-service".to_owned())]);
    service.get("/health").unwrap();
    assert!(scrape(&service).contains("otel_scope_name=\"renamed-user-service\""));
}

#[test]
fn ids_parse_as_java_uuid_fromstring_does() {
    let (_postgres, service) = started(&[]);
    for (id, status) in [
        ("1-2-3-4-5", 404),
        ("+1-+2-+3-+4-+5", 404),
        ("%EF%BC%A1-1-1-1-1", 404),
        ("0123456789abcdef0123456789abcdef", 400),
        ("%7B00000000-0000-0000-0000-000000000000%7D", 400),
        ("urn:uuid:00000000-0000-0000-0000-000000000000", 400),
    ] {
        let (got, _, body) = service.get(&format!("/users/{id}")).unwrap();
        assert_eq!(got, status, "{id}: {body}");
    }
    let text = scrape(&service);
    assert!(text.contains("http_route=\"/users/{id}\""), "{text}");
    assert!(
        text.contains("http_route=\"/users/0123456789abcdef0123456789abcdef\""),
        "{text}"
    );
}
