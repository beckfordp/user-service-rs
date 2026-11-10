// SPDX-License-Identifier: MIT

//! The binary's startup (RUST-6.2 spec FR1): without its env contract it
//! refuses to start and names every missing variable.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

#[test]
fn without_its_config_the_probe_exits_naming_what_is_missing() {
    let output = Command::new(env!("CARGO_BIN_EXE_events-probe"))
        .env_clear()
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("PROBE_KAFKA_BOOTSTRAP is required"),
        "{stderr}"
    );
    assert!(stderr.contains("PROBE_LOOKUP_URL is required"), "{stderr}");
}

#[test]
fn the_library_knows_its_version() {
    assert_eq!(events_probe::version(), env!("CARGO_PKG_VERSION"));
}
