// SPDX-License-Identifier: MIT

//! JVM behaviour the Scala services' contracts inherited without saying so.

use uuid::Uuid;

/// The zero of every run of Unicode decimal digits (`Nd`) in the Basic
/// Multilingual Plane, per JDK 21 (`Character.getType == DECIMAL_DIGIT_NUMBER`,
/// each run `0`–`9` contiguous). Supplementary-plane digits never count: Java
/// parses UTF-16 units, and a surrogate is no digit.
const DIGIT_ZEROS: [u32; 37] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

/// `Character.digit(c, 16)`: ASCII hex, fullwidth Latin A–F/a–f, and any
/// BMP decimal digit.
fn hex_digit(c: char) -> Option<u32> {
    if let Some(value) = c.to_digit(16) {
        return Some(value);
    }
    let code = u32::from(c);
    match code {
        0xFF21..=0xFF26 => Some(code - 0xFF21 + 10),
        0xFF41..=0xFF46 => Some(code - 0xFF41 + 10),
        _ => DIGIT_ZEROS
            .iter()
            .find(|&&zero| (zero..zero + 10).contains(&code))
            .map(|zero| code - zero),
    }
}

/// `Long.parseLong(group, 16)`: an optional sign, at least one digit, within
/// `i64`. (A group can't start with `-`: that is the separator.)
fn parse_long_hex(group: &str) -> Option<i64> {
    let digits = group.strip_prefix('+').unwrap_or(group);
    if digits.is_empty() {
        return None;
    }
    let mut value: i64 = 0;
    for c in digits.chars() {
        value = value
            .checked_mul(16)?
            .checked_add(i64::from(hex_digit(c)?))?;
    }
    Some(value)
}

/// What `java.util.UUID.fromString` returns for `text`, or `None` where it
/// throws: at most 36 UTF-16 units; exactly four `-`; five groups, each a
/// `Long.parseLong` hex number, masked to 32, 16, 16, 16 and 48 bits. So
/// `1-2-3-4-5` is a UUID and `0123456789abcdef0123456789abcdef` isn't.
/// tapir's UUID path codec and purerest's `http_route` template both use it.
#[must_use]
pub fn parse_uuid(text: &str) -> Option<Uuid> {
    if text.encode_utf16().count() > 36 {
        return None;
    }
    let groups: Vec<&str> = text.split('-').collect();
    let [time_low, time_mid, time_high, clock_seq, node] = groups.as_slice() else {
        return None;
    };
    let mask = |group: &str, bits: u32| -> Option<u64> {
        // The mask keeps the low bits, as Java's `& 0xffff…L` does.
        parse_long_hex(group).map(|v| v.cast_unsigned() & ((1_u64 << bits) - 1))
    };
    let most = (mask(time_low, 32)? << 32) | (mask(time_mid, 16)? << 16) | mask(time_high, 16)?;
    let least = (mask(clock_seq, 16)? << 48) | mask(node, 48)?;
    Some(Uuid::from_u64_pair(most, least))
}
