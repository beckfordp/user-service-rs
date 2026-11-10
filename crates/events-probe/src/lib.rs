// SPDX-License-Identifier: MIT

//! A throwaway Kafka consumer, run only inside the contract harness, to
//! measure the consumer-side cost (gluon ADR 0028 decision 8): it consumes
//! `user-events` as notification-service's consumers do, with purekafka's and
//! purerest's behaviour rebuilt on the `telemetry` crate.

pub mod config;
pub mod dead_letter;
pub mod handler;
pub mod kafka;
pub mod lookup;
pub mod run;
pub mod supervisor;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
