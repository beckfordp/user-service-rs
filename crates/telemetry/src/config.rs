// SPDX-License-Identifier: MIT

//! Config helpers every service shares (`PureConfig`'s readings, as purerest
//! services get them).

use std::time::Duration;

/// Parses a HOCON duration as Typesafe Config does: a number and an optional
/// unit (`ns`, `us`, `ms`, `s`, `m`, `h`, `d` or their long forms); no unit
/// means milliseconds.
///
/// # Errors
/// A message saying what was wrong with `text`.
pub fn parse_duration(text: &str) -> Result<Duration, String> {
    let trimmed = text.trim();
    let split = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    let (number, unit) = trimmed.split_at(split);
    let amount: u64 = number
        .parse()
        .map_err(|_| format!("{text:?} is not a duration (e.g. 1h, 30d, 500ms)"))?;
    let unit_nanos: u64 = match unit.trim() {
        "ns" | "nano" | "nanos" | "nanosecond" | "nanoseconds" => 1,
        "us" | "micro" | "micros" | "microsecond" | "microseconds" => 1_000,
        "" | "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => 1_000_000,
        "s" | "second" | "seconds" => 1_000_000_000,
        "m" | "minute" | "minutes" => 60_000_000_000,
        "h" | "hour" | "hours" => 3_600_000_000_000,
        "d" | "day" | "days" => 86_400_000_000_000,
        other => return Err(format!("unknown duration unit {other:?} in {text:?}")),
    };
    amount
        .checked_mul(unit_nanos)
        .map(Duration::from_nanos)
        .ok_or_else(|| format!("{text:?} is too long"))
}
