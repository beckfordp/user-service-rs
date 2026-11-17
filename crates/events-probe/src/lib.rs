// SPDX-License-Identifier: MIT

//! A throwaway Kafka consumer, run only inside the contract harness, to
//! measure the consumer-side cost (gluon ADR 0028 decision 8): it consumes
//! `user-events` as notification-service's consumers do, on gluon-rs's
//! `kafka-event-consumer` (purekafka's consumer side) and `telemetry`
//! (purerest's). Retired after gluon RPN-13.

pub mod config;
pub mod handler;
pub mod lookup;
pub mod run;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
