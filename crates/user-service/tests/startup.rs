// SPDX-License-Identifier: MIT

//! Startup (RUST-2.3 spec FR2-FR4): the binary reads the config contract,
//! serves /health on the configured port, refuses bad config, stops on SIGTERM.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::net::TcpListener;

use support::{Postgres, Service, free_port};

#[test]
fn serves_health_on_the_configured_port_and_stops_on_sigterm() {
    let postgres = Postgres::start();
    let mut service = Service::start(&postgres.env());
    assert_eq!(service.get("/health").unwrap(), (200, None, String::new()));
    service.terminate();
    let status = service.wait_exit();
    assert!(status.success(), "SIGTERM should stop it cleanly: {status}");
}

#[test]
fn bad_config_exits_1_listing_every_problem_and_binds_nothing() {
    let port = free_port();
    let mut service = Service::spawn(&[
        ("USER_SERVICE_PORT", "notaport".to_owned()),
        ("AUTH_PBKDF2_ITERATIONS", "0".to_owned()),
        ("OTEL_TRACES_SAMPLER_ARG", "abc".to_owned()),
        ("USER_SERVICE_METRICS_PORT", port.to_string()),
    ]);
    assert_eq!(service.wait_exit().code(), Some(1));
    let stderr = service.stderr();
    for variable in [
        "USER_SERVICE_PORT",
        "AUTH_PBKDF2_ITERATIONS",
        "OTEL_TRACES_SAMPLER_ARG",
    ] {
        assert!(
            stderr.contains(variable),
            "{variable} missing from: {stderr}"
        );
    }
    assert!(
        TcpListener::bind(("127.0.0.1", port)).is_ok(),
        "nothing bound"
    );
}
