// SPDX-License-Identifier: MIT

//! Password hashing (ADR 0028 decision 5): PBKDF2-HMAC-SHA256 over the UTF-8
//! password, 16-byte salt, 32-byte key, stored as
//! `pbkdf2$<iterations>$<salt>$<hash>` (base64url, no padding) — the Scala
//! service's format, so either implementation verifies the other's hashes.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::domain::Password;

const SALT_BYTES: usize = 16;
const KEY_BYTES: usize = 32;

/// The outcome of checking a password against a stored hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordCheck {
    /// The password is right.
    Match,
    /// The password is wrong, or there is no account.
    Mismatch,
    /// The stored value isn't in the expected format (a corrupt or foreign row).
    Malformed,
}

/// Hashes and verifies passwords; CPU-heavy, so callers run it on `spawn_blocking`.
#[derive(Clone)]
pub struct PasswordHasher {
    iterations: u32,
    dummy_salt: [u8; SALT_BYTES],
    dummy_key: [u8; KEY_BYTES],
}

impl PasswordHasher {
    /// A hasher creating new hashes with `iterations` rounds.
    #[must_use]
    pub fn new(iterations: u32) -> Self {
        let mut dummy_salt = [0; SALT_BYTES];
        let mut dummy_key = [0; KEY_BYTES];
        rand::rng().fill_bytes(&mut dummy_salt);
        rand::rng().fill_bytes(&mut dummy_key);
        Self {
            iterations,
            dummy_salt,
            dummy_key,
        }
    }

    /// A new stored hash for `password`, with a fresh random salt.
    #[must_use]
    pub fn hash(&self, password: &Password) -> String {
        let mut salt = [0; SALT_BYTES];
        rand::rng().fill_bytes(&mut salt);
        let key = derive(password, &salt, self.iterations);
        format!(
            "pbkdf2${}${}${}",
            self.iterations,
            URL_SAFE_NO_PAD.encode(salt),
            URL_SAFE_NO_PAD.encode(key)
        )
    }

    /// Checks `password` against `stored`, in constant time. No account
    /// (`None`) or an unreadable row still runs one full derivation with this
    /// hasher's iteration count, so timing doesn't reveal which emails exist.
    #[must_use]
    pub fn verify(&self, password: &Password, stored: Option<&str>) -> PasswordCheck {
        let Some(stored) = stored else {
            self.dummy_check(password);
            return PasswordCheck::Mismatch;
        };
        let Some((iterations, salt, key)) = parse_stored(stored) else {
            self.dummy_check(password);
            return PasswordCheck::Malformed;
        };
        if bool::from(derive(password, &salt, iterations).ct_eq(&key)) {
            PasswordCheck::Match
        } else {
            PasswordCheck::Mismatch
        }
    }

    fn dummy_check(&self, password: &Password) {
        let derived = derive(password, &self.dummy_salt, self.iterations);
        let _ = derived.ct_eq(&self.dummy_key);
    }
}

fn derive(password: &Password, salt: &[u8], iterations: u32) -> [u8; KEY_BYTES] {
    let mut key = [0; KEY_BYTES];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.expose().as_bytes(), salt, iterations, &mut key);
    key
}

/// `pbkdf2$<iterations>$<salt>$<hash>`; anything else is malformed.
fn parse_stored(stored: &str) -> Option<(u32, Vec<u8>, Vec<u8>)> {
    let mut parts = stored.split('$');
    let (Some("pbkdf2"), Some(iterations), Some(salt), Some(key), None) = (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) else {
        return None;
    };
    let iterations = iterations.parse::<u32>().ok().filter(|n| *n > 0)?;
    let salt = URL_SAFE_NO_PAD
        .decode(salt)
        .ok()
        .filter(|s| !s.is_empty())?;
    let key = URL_SAFE_NO_PAD.decode(key).ok().filter(|k| !k.is_empty())?;
    Some((iterations, salt, key))
}
