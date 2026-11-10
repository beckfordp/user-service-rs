// SPDX-License-Identifier: MIT

//! Password hashing and tokens (RUST-2.5 spec FR2-FR3). Sources: ADR 0028
//! decision 5; capture db-user-account.txt (a Scala-made hash of CAPTURE.md's password).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use time::macros::datetime;
use user_service::domain::Password;
use user_service::hashing::{PasswordCheck, PasswordHasher};
use user_service::tokens::{TokenKind, issue, sha256_hex};

const SCALA_HASH: &str =
    "pbkdf2$210000$I0d_rX5N6NLp3Allk1Z5ew$NRZ6suRMJFGRTlEcj32skfkNperlROT1Wj5rvBiEHBY";

fn password(text: &str) -> Password {
    Password::for_login(text).unwrap()
}

#[test]
fn a_hash_made_by_the_scala_service_verifies() {
    let hasher = PasswordHasher::new(210_000);
    assert_eq!(
        hasher.verify(&password("correct-horse-battery-1"), Some(SCALA_HASH)),
        PasswordCheck::Match
    );
    assert_eq!(
        hasher.verify(&password("wrong-password-1"), Some(SCALA_HASH)),
        PasswordCheck::Mismatch
    );
}

#[test]
fn new_hashes_have_the_scala_format_and_verify() {
    let hasher = PasswordHasher::new(1000);
    let stored = hasher.hash(&password("😀 non-ASCII too"));
    let parts: Vec<&str> = stored.split('$').collect();
    assert_eq!(parts[..2], ["pbkdf2", "1000"]);
    assert_eq!(
        (parts[2].len(), parts[3].len()),
        (22, 43),
        "16-byte salt, 32-byte hash, base64url"
    );
    assert_eq!(
        hasher.verify(&password("😀 non-ASCII too"), Some(&stored)),
        PasswordCheck::Match
    );
    assert_ne!(
        hasher.hash(&password("same")),
        hasher.hash(&password("same")),
        "fresh salt each time"
    );
}

#[test]
fn the_stored_iteration_count_wins_over_the_configured_one() {
    let stored = PasswordHasher::new(1000).hash(&password("correct-horse-battery-1"));
    assert_eq!(
        PasswordHasher::new(2000).verify(&password("correct-horse-battery-1"), Some(&stored)),
        PasswordCheck::Match
    );
}

#[test]
fn a_missing_or_malformed_stored_hash_never_matches() {
    let hasher = PasswordHasher::new(1000);
    assert_eq!(hasher.verify(&password("x"), None), PasswordCheck::Mismatch);
    for bad in [
        "",
        "bcrypt$10$abc",
        "pbkdf2$0$AAAA$AAAA",
        "pbkdf2$x$AAAA$AAAA",
        "pbkdf2$1$!!$AAAA",
        "pbkdf2$1$AAAA",
    ] {
        assert_eq!(
            hasher.verify(&password("x"), Some(bad)),
            PasswordCheck::Malformed,
            "{bad:?}"
        );
    }
}

#[test]
fn tokens_are_43_base64url_characters_stored_as_sha256_hex() {
    let now = datetime!(2026-11-09 18:43:19.5 UTC);
    let token = issue(TokenKind::Access, Duration::from_secs(3600), now);
    assert_eq!(token.value.len(), 43);
    assert!(
        token
            .value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    );
    assert_eq!(token.hash, sha256_hex(&token.value));
    assert_eq!(token.hash.len(), 64);
    assert_eq!(token.expires_at, datetime!(2026-11-09 19:43:19.5 UTC));
    assert_eq!(TokenKind::Access.as_str(), "access");
    assert_eq!(TokenKind::Refresh.as_str(), "refresh");
    assert_ne!(
        issue(TokenKind::Refresh, Duration::from_secs(1), now).value,
        token.value
    );
    assert_eq!(
        sha256_hex("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
