// SPDX-License-Identifier: MIT

//! The user-service binary: the wire-compatible port of the Scala user-service (gluon ADR 0028).

pub mod auth;
pub mod config;
pub mod cors;
pub mod db;
pub mod docs;
pub mod domain;
pub mod events;
pub mod hashing;
pub mod responses;
pub mod server;
pub mod store;
pub mod tokens;
pub mod users;

/// The package version, as built.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
