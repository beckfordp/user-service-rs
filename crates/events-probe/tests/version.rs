// SPDX-License-Identifier: MIT

//! Placeholder (RUST-2.2): the crate builds, links, and reports its package version.

#[test]
fn version_is_the_package_version() {
    assert_eq!(events_probe::version(), "0.1.0");
}
