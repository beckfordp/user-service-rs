// SPDX-License-Identifier: MIT

//! Metrics as purerest exposes them through the OTel Java Prometheus exporter
//! (RUST-3.3 spec FR1-FR4). Sources: capture 20261109 metrics.txt; purerest
//! `ServerMetrics`, `PoolMetrics`, `LatencyBuckets`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use telemetry::metrics::{
    CONTENT_TYPE, LATENCY_BUCKETS, Metrics, PoolMetrics, ServerMetrics, route_template,
    server_metrics,
};
use telemetry::tracing::Unmatched;
use tower::ServiceExt;

const SCOPE: &str = "otel_scope_name=\"user-service\"";

#[test]
fn the_latency_buckets_are_purerests() {
    assert_eq!(
        LATENCY_BUCKETS,
        [
            0.005, 0.01, 0.025, 0.05, 0.075, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.75, 1.0,
            1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 7.5, 10.0
        ]
    );
}

#[test]
fn the_text_is_what_the_java_exporter_writes() {
    let metrics = Metrics::new("user-service");
    let registrations = metrics.counter("users.registrations");
    registrations.add(1, &[("outcome", "created")]);
    registrations.add(1, &[("outcome", "duplicate")]);
    registrations.add(1, &[("outcome", "created")]);
    let used = metrics.up_down_counter("db.client.connection.count");
    let attrs = [
        ("db.client.connection.pool.name", "user"),
        ("db.client.connection.state", "used"),
    ];
    used.add(1, &attrs);
    used.add(-1, &attrs);
    let wait = metrics.histogram("db.client.connection.wait_time", "s", &[0.005, 1.0, 10.0]);
    let pool = [("db.client.connection.pool.name", "user")];
    wait.record(0.002, &pool);
    wait.record(2.5, &pool);
    let _never_used = metrics.counter("users.logins");
    let p = "db_client_connection_pool_name=\"user\"";
    assert_eq!(
        metrics.render(),
        format!(
            "# TYPE db_client_connection_count gauge\n\
             db_client_connection_count{{{p},db_client_connection_state=\"used\",{SCOPE}}} 0.0\n\
             # TYPE db_client_connection_wait_time_seconds histogram\n\
             db_client_connection_wait_time_seconds_bucket{{{p},{SCOPE},le=\"0.005\"}} 1\n\
             db_client_connection_wait_time_seconds_bucket{{{p},{SCOPE},le=\"1.0\"}} 1\n\
             db_client_connection_wait_time_seconds_bucket{{{p},{SCOPE},le=\"10.0\"}} 2\n\
             db_client_connection_wait_time_seconds_bucket{{{p},{SCOPE},le=\"+Inf\"}} 2\n\
             db_client_connection_wait_time_seconds_count{{{p},{SCOPE}}} 2\n\
             db_client_connection_wait_time_seconds_sum{{{p},{SCOPE}}} 2.502\n\
             # TYPE users_registrations_total counter\n\
             users_registrations_total{{{SCOPE},outcome=\"created\"}} 2.0\n\
             users_registrations_total{{{SCOPE},outcome=\"duplicate\"}} 1.0\n"
        )
    );
}

#[test]
fn series_sort_by_label_names_then_values() {
    // The capture lists the status-less /no-such-path series after every
    // series with a status code, whatever its method (metrics.txt).
    let metrics = Metrics::new("user-service");
    let counter = metrics.counter("c");
    counter.add(
        1,
        &[
            ("http.request.method", "POST"),
            ("http.response.status_code", "409"),
        ],
    );
    counter.add(
        1,
        &[
            ("http.request.method", "GET"),
            ("http.route", "/no-such-path"),
        ],
    );
    counter.add(
        1,
        &[
            ("http.request.method", "DELETE"),
            ("http.response.status_code", "405"),
        ],
    );
    let text = metrics.render();
    let order: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert!(order[0].contains("DELETE"), "{text}");
    assert!(order[1].contains("POST"), "{text}");
    assert!(order[2].contains("/no-such-path"), "{text}");
}

#[test]
fn values_print_as_java_doubles_and_labels_are_escaped() {
    let metrics = Metrics::new("s");
    let gauge = metrics.up_down_counter("big");
    gauge.add(12_345_678, &[("label", "a\"b\\c\nd")]);
    let hist = metrics.histogram("tiny", "s", &[1.0]);
    hist.record(0.000_1, &[]);
    let text = metrics.render();
    assert!(
        text.contains("big{label=\"a\\\"b\\\\c\\nd\",otel_scope_name=\"s\"} 1.2345678E7\n"),
        "{text}"
    );
    assert!(
        text.contains("tiny_seconds_sum{otel_scope_name=\"s\"} 1.0E-4\n"),
        "{text}"
    );
}

#[test]
fn the_route_is_the_raw_path_with_jvm_uuid_segments_as_id() {
    assert_eq!(
        route_template("/users/691b03e8-f1d7-45d9-a561-5183ffae9d0b"),
        "/users/{id}"
    );
    assert_eq!(
        route_template("/users/691B03E8-F1D7-45D9-A561-5183FFAE9D0B"),
        "/users/{id}"
    );
    assert_eq!(route_template("/users/1-2-3-4-5"), "/users/{id}");
    assert_eq!(route_template("/users/not-a-uuid"), "/users/not-a-uuid");
    assert_eq!(
        route_template("/users/0123456789abcdef0123456789abcdef"),
        "/users/0123456789abcdef0123456789abcdef"
    );
    assert_eq!(route_template("/docs/"), "/docs/");
    assert_eq!(route_template("/"), "/");
    assert_eq!(
        route_template("/a/1-2-3-4-5/b/1-2-3-4-5/"),
        "/a/{id}/b/{id}/"
    );
}

async fn call(app: &Router, method: &str, uri: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn app(metrics: &Metrics) -> Router {
    Router::new()
        .route("/users/{id}", get(|| async { StatusCode::OK }))
        .route("/boom", get(|| async { StatusCode::INTERNAL_SERVER_ERROR }))
        .fallback(|| async { (StatusCode::NOT_FOUND, Unmatched).into_response() })
        .layer(axum::middleware::from_fn_with_state(
            ServerMetrics::new(metrics),
            server_metrics,
        ))
}

fn series(text: &str, family: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with(&format!("{family}{{")))
        .map(ToOwned::to_owned)
        .collect()
}

#[tokio::test]
async fn each_request_is_timed_by_method_route_and_status() {
    let metrics = Metrics::new("user-service");
    let app = app(&metrics);
    call(&app, "GET", "/users/691b03e8-f1d7-45d9-a561-5183ffae9d0b").await;
    call(&app, "GET", "/users/1-2-3-4-5").await;
    call(&app, "GET", "/boom").await;
    call(&app, "GET", "/no-such-path").await;
    let text = metrics.render();
    assert!(text.contains("# TYPE http_server_request_duration_seconds histogram\n"));
    assert_eq!(
        series(&text, "http_server_request_duration_seconds_count"),
        [
            format!(
                "http_server_request_duration_seconds_count{{http_request_method=\"GET\",http_response_status_code=\"200\",http_route=\"/users/{{id}}\",{SCOPE}}} 2"
            ),
            format!(
                "http_server_request_duration_seconds_count{{http_request_method=\"GET\",http_response_status_code=\"500\",http_route=\"/boom\",{SCOPE}}} 1"
            ),
            format!(
                "http_server_request_duration_seconds_count{{http_request_method=\"GET\",http_route=\"/no-such-path\",{SCOPE}}} 1"
            ),
        ]
    );
    assert_eq!(
        series(&text, "http_server_request_duration_seconds_bucket").len(),
        3 * (LATENCY_BUCKETS.len() + 1)
    );
}

#[tokio::test]
async fn the_endpoint_serves_the_text() {
    let metrics = Metrics::new("user-service");
    metrics.counter("users.logins").add(1, &[]);
    let endpoint = telemetry::metrics::router(metrics.clone());
    for path in ["/metrics", "/"] {
        let response = endpoint
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], CONTENT_TYPE);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(String::from_utf8(body.to_vec()).unwrap(), metrics.render());
    }
}

#[tokio::test]
async fn pool_acquisitions_are_measured() {
    let metrics = Metrics::new("user-service");
    let pool = PoolMetrics::new(&metrics, "user");
    let (release, waiting) = tokio::sync::oneshot::channel::<u32>();
    let acquiring = {
        let pool = pool.clone();
        tokio::spawn(async move {
            let held = pool
                .acquire(async { Ok::<_, String>(waiting.await.unwrap()) })
                .await
                .unwrap();
            *held
        })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    let text = metrics.render();
    assert!(
        text.contains(&format!("db_client_connection_pending_requests{{db_client_connection_pool_name=\"user\",{SCOPE}}} 1.0")),
        "{text}"
    );
    release.send(7).unwrap();
    assert_eq!(acquiring.await.unwrap(), 7);

    let held = pool.acquire(async { Ok::<_, String>(1) }).await.unwrap();
    let text = metrics.render();
    assert!(text.contains(&format!("db_client_connection_count{{db_client_connection_pool_name=\"user\",db_client_connection_state=\"used\",{SCOPE}}} 1.0")), "{text}");
    drop(held);
    let failed = pool
        .acquire(async { Err::<u32, _>("refused".to_owned()) })
        .await;
    assert!(failed.is_err());
    let text = metrics.render();
    assert!(text.contains(&format!("db_client_connection_count{{db_client_connection_pool_name=\"user\",db_client_connection_state=\"used\",{SCOPE}}} 0.0")), "{text}");
    assert!(text.contains(&format!("db_client_connection_pending_requests{{db_client_connection_pool_name=\"user\",{SCOPE}}} 0.0")), "{text}");
    assert!(
        text.contains(&format!("db_client_connection_wait_time_seconds_count{{db_client_connection_pool_name=\"user\",{SCOPE}}} 2\n")),
        "only successful acquisitions are timed: {text}"
    );
}

#[test]
fn a_gauge_holds_the_last_value_set() {
    let metrics = Metrics::new("events-probe");
    let lag = metrics.gauge("purekafka.consumer.lag");
    lag.set(3.0, &[("partition", "0")]);
    lag.set(1.0, &[("partition", "0")]);
    assert!(
        metrics.render().contains(
            "purekafka_consumer_lag{otel_scope_name=\"events-probe\",partition=\"0\"} 1.0\n"
        ),
        "{}",
        metrics.render()
    );
}
