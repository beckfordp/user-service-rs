// SPDX-License-Identifier: MIT

//! `java.time.Instant.toString` formatting (RUST-2.6). Source: capture
//! http/users-200.txt (`2026-11-09T18:43:19.511602Z`).

use time::macros::datetime;
use user_service::users::java_instant;

#[test]
fn instants_print_as_java_does() {
    assert_eq!(
        java_instant(datetime!(2026-11-09 18:43:19.511602 UTC)),
        "2026-11-09T18:43:19.511602Z"
    );
    assert_eq!(
        java_instant(datetime!(2026-11-09 18:43:19.511 UTC)),
        "2026-11-09T18:43:19.511Z"
    );
    assert_eq!(
        java_instant(datetime!(2026-11-09 18:43:19.0005 UTC)),
        "2026-11-09T18:43:19.000500Z"
    );
    assert_eq!(
        java_instant(datetime!(2026-11-09 18:43:00 UTC)),
        "2026-11-09T18:43:00Z"
    );
    assert_eq!(
        java_instant(datetime!(2026-11-09 19:43:19.511602 +01:00)),
        "2026-11-09T18:43:19.511602Z"
    );
    assert_eq!(
        java_instant(datetime!(2026-11-09 18:43:19.123456789 UTC)),
        "2026-11-09T18:43:19.123456789Z"
    );
}
