// SPDX-License-Identifier: MIT

//! Credentials: parsed once at the boundary (`rust.md` rule 3), pure (rule 1).
//!
//! Sources: capture 20261109 (error messages, normalisation) and ADR 0028
//! Context: Java's `String.trim` strips characters up to U+0020 only, its regex
//! `\s` is ASCII-only, and `String.length` counts UTF-16 code units.

use std::fmt;

/// Why credentials were refused; `Display` is the exact wire message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
    /// 400: not `local@host.tld`.
    #[error("Enter a valid email address")]
    InvalidEmail,
    /// 400: a signup password under 8 UTF-16 code units.
    #[error("Password must be at least 8 characters")]
    PasswordTooShort,
    /// 400: an empty login password.
    #[error("Enter your password")]
    PasswordMissing,
    /// 409: the normalised address is already registered.
    #[error("An account with this email already exists")]
    DuplicateEmail,
    /// 401: unknown email or wrong password, deliberately indistinguishable.
    #[error("Email or password is incorrect")]
    InvalidCredentials,
}

/// A normalised email address: trimmed (characters up to U+0020 only),
/// lower-cased, shaped `local@host.tld`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    /// Parses raw input as the Scala service does.
    ///
    /// # Errors
    /// [`AuthError::InvalidEmail`] when it isn't `local@host.tld`.
    pub fn parse(raw: &str) -> Result<Self, AuthError> {
        let normalised = raw.trim_matches(|c: char| c <= '\u{20}').to_lowercase();
        if looks_like_an_email(&normalised) {
            Ok(Self(normalised))
        } else {
            Err(AuthError::InvalidEmail)
        }
    }

    /// A value read back from the database, already normalised when stored.
    #[must_use]
    pub fn from_stored(value: String) -> Self {
        Self(value)
    }

    /// The address as stored and returned.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Java's `\s`: space, tab, newline, vertical tab, form feed, carriage return.
fn is_java_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r')
}

/// `^[^\s@]+@[^\s@]+\.[^\s@]+$` with Java's ASCII-only `\s`.
fn looks_like_an_email(text: &str) -> bool {
    if text.chars().any(is_java_space) {
        return false;
    }
    let Some((local, domain)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.contains('@')
        && domain
            .char_indices()
            .any(|(index, c)| c == '.' && index > 0 && index + 1 < domain.len())
}

/// A plaintext password on its way to the hasher; never printed.
#[derive(Clone)]
pub struct Password(String);

impl Password {
    /// The signup rule: at least 8 UTF-16 code units (Java's `String.length`).
    ///
    /// # Errors
    /// [`AuthError::PasswordTooShort`].
    pub fn for_signup(raw: &str) -> Result<Self, AuthError> {
        if raw.encode_utf16().count() >= 8 {
            Ok(Self(raw.to_owned()))
        } else {
            Err(AuthError::PasswordTooShort)
        }
    }

    /// The login rule: not empty (the hash decides the rest).
    ///
    /// # Errors
    /// [`AuthError::PasswordMissing`].
    pub fn for_login(raw: &str) -> Result<Self, AuthError> {
        if raw.is_empty() {
            Err(AuthError::PasswordMissing)
        } else {
            Ok(Self(raw.to_owned()))
        }
    }

    /// The plaintext, for the hasher only.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(<redacted>)")
    }
}

/// An email and a password, parsed in that order.
#[derive(Debug, Clone)]
pub struct Credentials {
    /// The normalised address.
    pub email: Email,
    /// The password.
    pub password: Password,
}

impl Credentials {
    /// Signup input.
    ///
    /// # Errors
    /// The first problem: the email, then the password.
    pub fn signup(email: &str, password: &str) -> Result<Self, AuthError> {
        Ok(Self {
            email: Email::parse(email)?,
            password: Password::for_signup(password)?,
        })
    }

    /// Login input.
    ///
    /// # Errors
    /// The first problem: the email, then the password.
    pub fn login(email: &str, password: &str) -> Result<Self, AuthError> {
        Ok(Self {
            email: Email::parse(email)?,
            password: Password::for_login(password)?,
        })
    }
}
