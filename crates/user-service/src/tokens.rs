// SPDX-License-Identifier: MIT

//! Opaque tokens (ADR 0028 decision 5): 32 random bytes, base64url without
//! padding, returned once; only the hex SHA-256 is stored.

use std::fmt::Write as _;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

/// The two kinds of token, as stored in `auth_tokens.kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Returned by signup and login.
    Access,
    /// Returned by login.
    Refresh,
}

impl TokenKind {
    /// The stored spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
        }
    }
}

/// A token just issued: the value for the caller, the hash for the store.
#[derive(Debug, Clone)]
pub struct IssuedToken {
    /// The raw value; never stored or logged.
    pub value: String,
    /// Lower-case hex SHA-256 of `value`.
    pub hash: String,
    /// Which kind.
    pub kind: TokenKind,
    /// `now + ttl`.
    pub expires_at: OffsetDateTime,
}

/// Issues a token of `kind` living `ttl` from `now`.
#[must_use]
pub fn issue(kind: TokenKind, ttl: Duration, now: OffsetDateTime) -> IssuedToken {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let value = URL_SAFE_NO_PAD.encode(bytes);
    IssuedToken {
        hash: sha256_hex(&value),
        value,
        kind,
        expires_at: now + ttl,
    }
}

/// Lower-case hex SHA-256 of the UTF-8 text.
#[must_use]
pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}
