// SPDX-License-Identifier: MIT

//! Shared by the integration tests: gluon-rs `service-testing`'s throwaway
//! PostgreSQL cluster, this service's binary under test, and an OTLP
//! collector (`otlp`).

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod otlp;

use std::ops::{Deref, DerefMut};

// Not every test file uses both.
#[allow(unused_imports)]
pub use service_testing::{Postgres, free_port};

/// The user-service binary, on fresh `USER_SERVICE_PORT` and
/// `USER_SERVICE_METRICS_PORT`.
pub struct Service {
    inner: service_testing::Service,
    /// Its Prometheus port.
    pub metrics_port: u16,
}

impl Service {
    /// Starts it with exactly `extra` (plus its ports); does not wait.
    pub fn spawn(extra: &[(&str, String)]) -> Self {
        Self::with(extra, service_testing::Service::spawn)
    }

    /// Starts it and waits until `/health` answers.
    pub fn start(extra: &[(&str, String)]) -> Self {
        Self::with(extra, service_testing::Service::start)
    }

    fn with(
        extra: &[(&str, String)],
        run: fn(&str, &str, &[(&str, String)]) -> service_testing::Service,
    ) -> Self {
        let metrics_port = free_port();
        let mut env = vec![("USER_SERVICE_METRICS_PORT", metrics_port.to_string())];
        env.extend(extra.iter().map(|(key, value)| (*key, value.clone())));
        let inner = run(
            env!("CARGO_BIN_EXE_user-service"),
            "USER_SERVICE_PORT",
            &env,
        );
        Self {
            inner,
            metrics_port,
        }
    }
}

impl Deref for Service {
    type Target = service_testing::Service;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Service {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
