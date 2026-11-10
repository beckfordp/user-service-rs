// SPDX-License-Identifier: MIT

//! The baseline migration, the pool and `/health/ready` (RUST-2.4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::thread::sleep;
use std::time::{Duration, Instant};

use support::{Postgres, Service, free_port};

#[test]
fn startup_migrates_the_schema_and_a_restart_reuses_it() {
    let postgres = Postgres::start();
    let service = Service::start(&postgres.env());
    let tables = postgres.query(
        "select string_agg(table_name, ',' order by table_name) from information_schema.tables \
         where table_schema = 'public'",
    );
    assert_eq!(
        tables.trim(),
        "_sqlx_migrations,auth_tokens,outbox_events,user_account"
    );
    drop(service);
    let again = Service::start(&postgres.env());
    assert_eq!(again.get("/health/ready").unwrap().0, 200);
}

#[test]
fn readiness_follows_postgres() {
    let postgres = Postgres::start();
    let service = Service::start(&postgres.env());
    assert_eq!(
        service.get("/health/ready").unwrap(),
        (200, None, String::new())
    );

    postgres.stop();
    assert_eq!(
        service.get("/health/ready").unwrap(),
        (503, None, String::new())
    );
    assert_eq!(
        service.get("/health").unwrap().0,
        200,
        "liveness doesn't depend on the database"
    );

    postgres.start_again();
    let deadline = Instant::now() + Duration::from_secs(30);
    while service.get("/health/ready").unwrap().0 != 200 {
        assert!(
            Instant::now() < deadline,
            "not ready again after Postgres came back"
        );
        sleep(Duration::from_millis(100));
    }
}

#[test]
fn an_unreachable_database_at_startup_exits_1() {
    let mut service = Service::spawn(&[
        ("POSTGRES_HOST", "127.0.0.1".to_owned()),
        ("POSTGRES_PORT", free_port().to_string()),
    ]);
    let status = service.wait_exit();
    assert_eq!(status.code(), Some(1));
    let stderr = service.stderr();
    assert!(stderr.contains("database"), "{stderr}");
}
