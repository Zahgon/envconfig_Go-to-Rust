//! Go's `time.Duration`.
//!
//! Reproduced rather than mapped onto `std::time::Duration`, because the
//! observable behaviour is the *syntax* Go accepts (`2m`, `1h30m`, `100ms`,
//! `1.5h`, `-2m`) and the exact error text, neither of which any Rust type
//! provides.

use std::error::Error;
use std::fmt;

/// A duration in nanoseconds, matching Go's `time.Duration`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(pub i64);

/// One nanosecond.
pub const NANOSECOND: Duration = Duration(1);
/// One microsecond.
pub const MICROSECOND: Duration = Duration(1_000);
/// One millisecond.
pub const MILLISECOND: Duration = Duration(1_000_000);
/// One second.
pub const SECOND: Duration = Duration(1_000_000_000);
/// One minute.
pub const MINUTE: Duration = Duration(60 * 1_000_000_000);
/// One hour.
pub const HOUR: Duration = Duration(3_600 * 1_000_000_000);

impl Duration {
    /// The duration as a whole number of nanoseconds.
    pub fn nanoseconds(self) -> i64 {
        self.0
    }
}

impl std::ops::Mul<i64> for Duration {
    type Output = Duration;
    fn mul(self, rhs: i64) -> Duration {
        Duration(self.0 * rhs)
    }
}

/// The error returned when a duration string cannot be parsed, rendered with
/// Go's message text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseDurationError {
    /// `time: invalid duration "…"`
    Invalid(String),
    /// `time: missing unit in duration "…"`
    MissingUnit(String),
    /// `time: unknown unit "…" in duration "…"`
    UnknownUnit {
        /// The unrecognised unit.
        unit: String,
        /// The whole input.
        input: String,
    },
}

impl fmt::Display for ParseDurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => write!(f, "time: invalid duration {s:?}"),
            Self::MissingUnit(s) => write!(f, "time: missing unit in duration {s:?}"),
            Self::UnknownUnit { unit, input } => {
                write!(f, "time: unknown unit {unit:?} in duration {input:?}")
            }
        }
    }
}

impl Error for ParseDurationError {}

fn unit_scale(u: &str) -> Option<i64> {
    Some(match u {
        "ns" => 1,
        "us" | "\u{00b5}s" | "\u{03bc}s" => 1_000,
        "ms" => 1_000_000,
        "s" => 1_000_000_000,
        "m" => 60 * 1_000_000_000,
        "h" => 3_600 * 1_000_000_000,
        _ => return None,
    })
}

/// Go's `time.ParseDuration`.
///
/// Grammar: an optional sign, then one or more decimal numbers each with a
/// unit suffix. The bare string `0` is accepted with no unit.
pub fn parse_duration(input: &str) -> Result<Duration, ParseDurationError> {
    let orig = input;
    let mut s = input;

    let neg = match s.as_bytes().first() {
        Some(b'-') => {
            s = &s[1..];
            true
        }
        Some(b'+') => {
            s = &s[1..];
            false
        }
        _ => false,
    };

    if s == "0" {
        return Ok(Duration(0));
    }
    if s.is_empty() {
        return Err(ParseDurationError::Invalid(orig.to_owned()));
    }

    let mut total: i64 = 0;
    while !s.is_empty() {
        // Integer part.
        let int_end = s.bytes().take_while(u8::is_ascii_digit).count();
        let int_part = &s[..int_end];
        s = &s[int_end..];

        // Optional fractional part.
        let mut frac_part = "";
        if s.as_bytes().first() == Some(&b'.') {
            s = &s[1..];
            let frac_end = s.bytes().take_while(u8::is_ascii_digit).count();
            frac_part = &s[..frac_end];
            s = &s[frac_end..];
        }

        if int_part.is_empty() && frac_part.is_empty() {
            return Err(ParseDurationError::Invalid(orig.to_owned()));
        }

        // Unit: everything up to the next digit or `.`.
        let unit_end = s
            .char_indices()
            .find(|(_, c)| c.is_ascii_digit() || *c == '.')
            .map_or(s.len(), |(i, _)| i);
        let unit = &s[..unit_end];
        s = &s[unit_end..];

        if unit.is_empty() {
            return Err(ParseDurationError::MissingUnit(orig.to_owned()));
        }
        let scale = unit_scale(unit).ok_or_else(|| ParseDurationError::UnknownUnit {
            unit: unit.to_owned(),
            input: orig.to_owned(),
        })?;

        let whole: i64 = if int_part.is_empty() {
            0
        } else {
            int_part
                .parse()
                .map_err(|_| ParseDurationError::Invalid(orig.to_owned()))?
        };
        let mut value = whole
            .checked_mul(scale)
            .ok_or_else(|| ParseDurationError::Invalid(orig.to_owned()))?;

        if !frac_part.is_empty() {
            // Scale the fraction by the unit without losing precision to
            // floating point for the common cases.
            let mut divisor: i64 = 1;
            let mut frac: i64 = 0;
            for c in frac_part.bytes() {
                if divisor > i64::MAX / 10 {
                    break;
                }
                divisor *= 10;
                frac = frac * 10 + i64::from(c - b'0');
            }
            value = value
                .checked_add(frac.saturating_mul(scale) / divisor)
                .ok_or_else(|| ParseDurationError::Invalid(orig.to_owned()))?;
        }

        total = total
            .checked_add(value)
            .ok_or_else(|| ParseDurationError::Invalid(orig.to_owned()))?;
    }

    Ok(Duration(if neg { -total } else { total }))
}

impl fmt::Display for Duration {
    /// Reproduces Go's `Duration.String`: `2m0s`, `1h30m0s`, `100ms`, `0s`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == 0 {
            return f.write_str("0s");
        }
        let neg = self.0 < 0;
        // Widen so that `i64::MIN` does not overflow on negation.
        let magnitude = i128::from(self.0).unsigned_abs();

        let rendered = if magnitude < 1_000_000_000 {
            // Sub-second: pick the largest unit that keeps the value >= 1.
            let (unit, prec) = if magnitude < 1_000 {
                ("ns", 0)
            } else if magnitude < 1_000_000 {
                ("\u{00b5}s", 3)
            } else {
                ("ms", 6)
            };
            format!("{}{unit}", fmt_frac(magnitude, prec))
        } else {
            let total_seconds = magnitude / 1_000_000_000;
            let frac = magnitude % 1_000_000_000;
            let seconds = total_seconds % 60;
            let minutes = (total_seconds / 60) % 60;
            let hours = total_seconds / 3_600;

            let mut tail = format!("{}s", fmt_frac(seconds * 1_000_000_000 + frac, 9));
            if minutes > 0 || hours > 0 {
                tail = format!("{minutes}m{tail}");
            }
            if hours > 0 {
                tail = format!("{hours}h{tail}");
            }
            tail
        };

        if neg {
            f.write_str("-")?;
        }
        f.write_str(&rendered)
    }
}

/// Renders `value` scaled by `10^prec`, trimming trailing zeros in the
/// fractional part, the way Go's duration formatter does.
fn fmt_frac(value: u128, prec: u32) -> String {
    if prec == 0 {
        return value.to_string();
    }
    let scale = 10u128.pow(prec);
    let whole = value / scale;
    let frac = value % scale;
    if frac == 0 {
        return whole.to_string();
    }
    let mut s = format!("{frac:0>width$}", width = prec as usize);
    while s.ends_with('0') {
        s.pop();
    }
    format!("{whole}.{s}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values captured from the Go implementation.
    #[test]
    fn parse_matches_go() {
        assert_eq!(parse_duration("2m").unwrap().0, 120_000_000_000);
        assert_eq!(parse_duration("1h30m").unwrap().0, 5_400_000_000_000);
        assert_eq!(parse_duration("100ms").unwrap().0, 100_000_000);
        assert_eq!(parse_duration("1.5h").unwrap().0, 5_400_000_000_000);
        assert_eq!(parse_duration("-2m").unwrap().0, -120_000_000_000);
        assert_eq!(parse_duration("0").unwrap().0, 0);
        assert_eq!(parse_duration("300ns").unwrap().0, 300);
        assert_eq!(parse_duration("1us").unwrap().0, 1_000);
        assert_eq!(parse_duration("1\u{00b5}s").unwrap().0, 1_000);
    }

    #[test]
    fn error_messages_match_go() {
        assert_eq!(
            parse_duration("bad").unwrap_err().to_string(),
            "time: invalid duration \"bad\""
        );
        assert_eq!(
            parse_duration("").unwrap_err().to_string(),
            "time: invalid duration \"\""
        );
        assert_eq!(
            parse_duration("10").unwrap_err().to_string(),
            "time: missing unit in duration \"10\""
        );
        assert_eq!(
            parse_duration("10q").unwrap_err().to_string(),
            "time: unknown unit \"q\" in duration \"10q\""
        );
    }

    #[test]
    fn display_matches_go() {
        assert_eq!(Duration(120_000_000_000).to_string(), "2m0s");
        assert_eq!(Duration(5_400_000_000_000).to_string(), "1h30m0s");
        assert_eq!(Duration(100_000_000).to_string(), "100ms");
        assert_eq!(Duration(0).to_string(), "0s");
        assert_eq!(Duration(-120_000_000_000).to_string(), "-2m0s");
        assert_eq!(Duration(300).to_string(), "300ns");
        assert_eq!(Duration(1_500_000_000).to_string(), "1.5s");
    }

    #[test]
    fn minute_constant_matches_the_test_suite_expectation() {
        assert_eq!(MINUTE * 2, parse_duration("2m").unwrap());
    }
    #[test]
    fn a_leading_plus_is_accepted() {
        assert_eq!(parse_duration("+2m").unwrap().0, 120_000_000_000);
    }

    #[test]
    fn multiple_units_accumulate() {
        assert_eq!(parse_duration("1h2m3s").unwrap().0, 3_723 * 1_000_000_000);
        assert_eq!(parse_duration("1.5s").unwrap().0, 1_500_000_000);
    }

    #[test]
    fn unit_constants_are_consistent() {
        assert_eq!(NANOSECOND.nanoseconds(), 1);
        assert_eq!(MICROSECOND.nanoseconds(), 1_000);
        assert_eq!(MILLISECOND.nanoseconds(), 1_000_000);
        assert_eq!(SECOND.nanoseconds(), 1_000_000_000);
        assert_eq!(HOUR, MINUTE * 60);
    }

    #[test]
    fn overflow_is_reported_not_wrapped() {
        assert!(parse_duration("9223372036854775808h").is_err());
    }

    #[test]
    fn sub_second_units_render_like_go() {
        assert_eq!(Duration(1_500).to_string(), "1.5\u{00b5}s");
        assert_eq!(Duration(1_000).to_string(), "1\u{00b5}s");
        assert_eq!(Duration(-300).to_string(), "-300ns");
    }
}
