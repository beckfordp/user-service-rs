// SPDX-License-Identifier: MIT

//! Cross-cutting behaviour every service needs: tracing, metrics, the log
//! format and the error envelope; the seed of a Rust purerest equivalent
//! (gluon ADR 0028 decision 4).

pub mod client;
pub mod config;
pub mod docs;
pub mod jvm;
pub mod logging;
pub mod metrics;
pub mod resilience;
pub mod tracing;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
