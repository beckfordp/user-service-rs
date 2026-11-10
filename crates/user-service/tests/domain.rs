// SPDX-License-Identifier: MIT

//! Credentials parsing (RUST-2.5 spec FR1). Sources: capture 20261109 (messages,
//! normalisation), ADR 0028 Context (UTF-16 length, ASCII-only trimming and `\s`).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use user_service::domain::{AuthError, Credentials, Email, Password};

#[test]
fn emails_are_trimmed_of_ascii_control_and_space_only_and_lower_cased() {
    assert_eq!(
        Email::parse("  Capture.KAT@Example.com ").unwrap().as_str(),
        "capture.kat@example.com"
    );
    assert_eq!(
        Email::parse("\tA@B.EXAMPLE\n").unwrap().as_str(),
        "a@b.example"
    );
    assert_eq!(
        Email::parse("ÄDA@EXAMPLE.COM").unwrap().as_str(),
        "äda@example.com"
    );
    // U+00A0 is above U+0020: kept, and not whitespace to an ASCII-only \s.
    assert_eq!(
        Email::parse("\u{a0}ada@example.com").unwrap().as_str(),
        "\u{a0}ada@example.com"
    );
}

#[test]
fn emails_must_look_like_local_at_host_dot_tld() {
    for bad in [
        "not-an-email",
        "no-domain@",
        "@no-local.com",
        "a b@example.com",
        "",
        "a@b",
        "a@@b.com",
        "a@b.",
        "a@.b",
        "   ",
    ] {
        assert_eq!(Email::parse(bad), Err(AuthError::InvalidEmail), "{bad:?}");
    }
    for good in ["a@b.c", "a.b@c.d.e", "x@y.z.", "first+tag@sub.example.com"] {
        assert!(Email::parse(good).is_ok(), "{good:?}");
    }
}

#[test]
fn signup_passwords_need_8_utf16_code_units() {
    assert!(Password::for_signup("12345678").is_ok());
    assert!(
        Password::for_signup("😀😀😀😀").is_ok(),
        "4 emoji are 8 UTF-16 units"
    );
    assert_eq!(
        Password::for_signup("ééééééé").err(),
        Some(AuthError::PasswordTooShort)
    );
    assert_eq!(
        Password::for_signup("short").err(),
        Some(AuthError::PasswordTooShort)
    );
}

#[test]
fn login_passwords_need_only_be_non_empty() {
    assert!(Password::for_login("x").is_ok());
    assert_eq!(
        Password::for_login("").err(),
        Some(AuthError::PasswordMissing)
    );
}

#[test]
fn passwords_never_print() {
    let password = Password::for_signup("correct-horse-battery-1").unwrap();
    assert_eq!(format!("{password:?}"), "Password(<redacted>)");
}

#[test]
fn the_email_is_checked_before_the_password() {
    assert_eq!(
        Credentials::signup("bad", "short").err(),
        Some(AuthError::InvalidEmail)
    );
    assert_eq!(
        Credentials::login("bad", "").err(),
        Some(AuthError::InvalidEmail)
    );
    assert_eq!(
        Credentials::signup("a@b.example", "short").err(),
        Some(AuthError::PasswordTooShort)
    );
    assert_eq!(
        Credentials::login("a@b.example", "").err(),
        Some(AuthError::PasswordMissing)
    );
    let credentials = Credentials::signup(" A@B.EXAMPLE ", "12345678").unwrap();
    assert_eq!(credentials.email.as_str(), "a@b.example");
    assert_eq!(credentials.password.expose(), "12345678");
}

#[test]
fn every_error_has_the_scala_services_message() {
    let messages = [
        (AuthError::InvalidEmail, "Enter a valid email address"),
        (
            AuthError::PasswordTooShort,
            "Password must be at least 8 characters",
        ),
        (AuthError::PasswordMissing, "Enter your password"),
        (
            AuthError::DuplicateEmail,
            "An account with this email already exists",
        ),
        (
            AuthError::InvalidCredentials,
            "Email or password is incorrect",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}
