//! Go's `time.Time`, limited to the RFC 3339 behaviour `envconfig` exposes.
//!
//! Reproduced rather than mapped onto a date-time crate because the error
//! message is observable: it reaches the caller inside `ParseError`, and the
//! test suite compares it against Go's `time.ParseError` text exactly.

use std::error::Error;
use std::fmt;

/// Go's `time.RFC3339` layout string.
pub const RFC3339: &str = "2006-01-02T15:04:05Z07:00";

/// An instant, stored as seconds and nanoseconds since the Unix epoch plus the
/// zone offset it was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    unix_seconds: i64,
    nanoseconds: u32,
    /// Seconds east of UTC.
    offset: i32,
}

/// Go's `time.ParseError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The layout that was being applied.
    pub layout: String,
    /// The whole value being parsed.
    pub value: String,
    /// The layout element that failed.
    pub layout_elem: String,
    /// The remaining input at the point of failure.
    pub value_elem: String,
    /// Set for range errors; when non-empty it replaces the default text.
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.message.is_empty() {
            write!(
                f,
                "parsing time {:?} as {:?}: cannot parse {:?} as {:?}",
                self.value, self.layout, self.value_elem, self.layout_elem
            )
        } else {
            write!(f, "parsing time {:?}{}", self.value, self.message)
        }
    }
}

impl Error for ParseError {}

/// Go's zero `time.Time` is January 1 of year 1, not the Unix epoch.
impl Default for Time {
    fn default() -> Self {
        Self {
            unix_seconds: days_from_civil(1, 1, 1) * 86_400,
            nanoseconds: 0,
            offset: 0,
        }
    }
}

impl Time {
    /// Builds a `Time` from calendar fields in UTC, like Go's `time.Date` with
    /// `time.UTC`.
    pub fn date(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32, nsec: u32) -> Self {
        let days = days_from_civil(year, month, day);
        Self {
            unix_seconds: days * 86_400
                + i64::from(hour) * 3_600
                + i64::from(min) * 60
                + i64::from(sec),
            nanoseconds: nsec,
            offset: 0,
        }
    }

    /// Seconds since the Unix epoch.
    pub fn unix(self) -> i64 {
        self.unix_seconds
    }

    /// Go's `Time.Equal`: compares instants, ignoring the recorded zone.
    pub fn equal(self, other: Self) -> bool {
        self.unix_seconds == other.unix_seconds && self.nanoseconds == other.nanoseconds
    }

    /// Renders the instant in its recorded zone using the RFC 3339 layout.
    pub fn format_rfc3339(self) -> String {
        let local = self.unix_seconds + i64::from(self.offset);
        let days = local.div_euclid(86_400);
        let secs = local.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
        let zone = if self.offset == 0 {
            "Z".to_owned()
        } else {
            let sign = if self.offset < 0 { '-' } else { '+' };
            let a = self.offset.abs();
            format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60)
        };
        // Go's RFC3339 layout carries no fractional-seconds element, so
        // formatting drops any sub-second precision the value retains.
        // `RFC3339Nano` is the layout that keeps it.
        format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}{zone}")
    }

    /// Renders the instant with sub-second precision, like Go's
    /// `time.RFC3339Nano`.
    pub fn format_rfc3339_nano(self) -> String {
        let base = self.format_rfc3339();
        if self.nanoseconds == 0 {
            return base;
        }
        let mut frac = format!("{:09}", self.nanoseconds);
        while frac.ends_with('0') {
            frac.pop();
        }
        let cut = base.len() - self.zone_len();
        format!("{}.{}{}", &base[..cut], frac, &base[cut..])
    }

    /// The sub-second component, in nanoseconds.
    pub fn nanosecond(self) -> u32 {
        self.nanoseconds
    }

    fn zone_len(self) -> usize {
        if self.offset == 0 {
            1
        } else {
            6
        }
    }
}

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format_rfc3339())
    }
}

/// Cursor over the input, reporting failures the way Go's parser does.
struct Cursor<'a> {
    value: &'a str,
    rest: &'a str,
}

impl<'a> Cursor<'a> {
    fn fail(&self, layout_elem: &str) -> ParseError {
        ParseError {
            layout: RFC3339.to_owned(),
            value: self.value.to_owned(),
            layout_elem: layout_elem.to_owned(),
            value_elem: self.rest.to_owned(),
            message: String::new(),
        }
    }

    fn out_of_range(&self, what: &str) -> ParseError {
        ParseError {
            layout: RFC3339.to_owned(),
            value: self.value.to_owned(),
            layout_elem: String::new(),
            value_elem: String::new(),
            message: format!(": {what} out of range"),
        }
    }

    fn digits(&mut self, n: usize, layout_elem: &str) -> Result<u32, ParseError> {
        if self.rest.len() < n || !self.rest.as_bytes()[..n].iter().all(u8::is_ascii_digit) {
            return Err(self.fail(layout_elem));
        }
        let v = self.rest[..n]
            .parse::<u32>()
            .map_err(|_| self.fail(layout_elem))?;
        self.rest = &self.rest[n..];
        Ok(v)
    }

    fn literal(&mut self, lit: &str, layout_elem: &str) -> Result<(), ParseError> {
        if !self.rest.starts_with(lit) {
            return Err(self.fail(layout_elem));
        }
        self.rest = &self.rest[lit.len()..];
        Ok(())
    }
}

/// Go's `time.Parse(time.RFC3339, value)`.
pub fn parse_rfc3339(value: &str) -> Result<Time, ParseError> {
    let mut c = Cursor { value, rest: value };

    let year = c.digits(4, "2006")? as i64;
    c.literal("-", "-")?;
    let month = c.digits(2, "01")?;
    c.literal("-", "-")?;
    let day = c.digits(2, "02")?;
    c.literal("T", "T")?;
    let hour = c.digits(2, "15")?;
    c.literal(":", ":")?;
    let min = c.digits(2, "04")?;
    c.literal(":", ":")?;
    let sec = c.digits(2, "05")?;

    // Optional fractional seconds.
    let mut nsec = 0u32;
    if c.rest.starts_with('.') {
        let digits = c.rest[1..].bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return Err(c.fail("."));
        }
        let frac = &c.rest[1..=digits];
        let mut scaled = frac.to_owned();
        scaled.truncate(9);
        while scaled.len() < 9 {
            scaled.push('0');
        }
        nsec = scaled.parse().unwrap_or(0);
        c.rest = &c.rest[1 + digits..];
    }

    // Zone: `Z` or `±hh:mm`.
    let offset = if c.rest.starts_with('Z') {
        c.rest = &c.rest[1..];
        0i32
    } else {
        let sign = match c.rest.as_bytes().first() {
            Some(b'+') => 1,
            Some(b'-') => -1,
            _ => return Err(c.fail("Z07:00")),
        };
        c.rest = &c.rest[1..];
        let zh = c.digits(2, "Z07:00")? as i32;
        c.literal(":", "Z07:00")?;
        let zm = c.digits(2, "Z07:00")? as i32;
        sign * (zh * 3600 + zm * 60)
    };

    if !c.rest.is_empty() {
        return Err(ParseError {
            layout: RFC3339.to_owned(),
            value: value.to_owned(),
            layout_elem: String::new(),
            value_elem: c.rest.to_owned(),
            message: format!(": extra text: {:?}", c.rest),
        });
    }

    if !(1..=12).contains(&month) {
        return Err(c.out_of_range("month"));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(c.out_of_range("day"));
    }
    if hour > 23 {
        return Err(c.out_of_range("hour"));
    }
    if min > 59 {
        return Err(c.out_of_range("minute"));
    }
    if sec > 59 {
        return Err(c.out_of_range("second"));
    }

    let days = days_from_civil(year, month, day);
    let local = days * 86_400 + i64::from(hour) * 3_600 + i64::from(min) * 60 + i64::from(sec);
    Ok(Time {
        unix_seconds: local - i64::from(offset),
        nanoseconds: nsec,
        offset,
    })
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since the Unix epoch for a proleptic Gregorian date.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let m = i64::from(m);
    let d = i64::from(d);
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of [`days_from_civil`].
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_value_used_by_the_test_suite() {
        let t = parse_rfc3339("2016-08-16T18:57:05Z").unwrap();
        assert!(t.equal(Time::date(2016, 8, 16, 18, 57, 5, 0)));
        assert_eq!(t.format_rfc3339(), "2016-08-16T18:57:05Z");
    }

    /// The exact message the Go suite compares against.
    #[test]
    fn error_message_matches_go() {
        let err = parse_rfc3339("I'M NOT A DATE").unwrap_err();
        assert_eq!(
            err.to_string(),
            "parsing time \"I'M NOT A DATE\" as \"2006-01-02T15:04:05Z07:00\": cannot parse \"I'M NOT A DATE\" as \"2006\""
        );
        assert_eq!(err.layout_elem, "2006");
        assert_eq!(err.value_elem, "I'M NOT A DATE");
    }

    #[test]
    fn handles_offsets_and_fractions() {
        let t = parse_rfc3339("2016-08-16T18:57:05+02:00").unwrap();
        assert_eq!(t.unix(), Time::date(2016, 8, 16, 16, 57, 5, 0).unix());
        assert_eq!(t.format_rfc3339(), "2016-08-16T18:57:05+02:00");

        // Parsing keeps the sub-second precision, but the RFC3339 layout does
        // not render it, exactly as Go behaves.
        let t = parse_rfc3339("2016-08-16T18:57:05.25Z").unwrap();
        assert_eq!(t.nanosecond(), 250_000_000);
        assert_eq!(t.format_rfc3339(), "2016-08-16T18:57:05Z");
        assert_eq!(t.format_rfc3339_nano(), "2016-08-16T18:57:05.25Z");
    }

    #[test]
    fn round_trips_across_eras() {
        for s in [
            "1970-01-01T00:00:00Z",
            "1969-12-31T23:59:59Z",
            "2000-02-29T12:00:00Z",
            "1900-03-01T00:00:00Z",
            "2100-12-31T23:59:59Z",
        ] {
            assert_eq!(parse_rfc3339(s).unwrap().format_rfc3339(), s, "{s}");
        }
    }

    #[test]
    fn rejects_out_of_range_and_trailing_text() {
        assert!(parse_rfc3339("2016-13-16T18:57:05Z").is_err());
        assert!(parse_rfc3339("2016-02-30T18:57:05Z").is_err());
        assert!(parse_rfc3339("2016-08-16T25:57:05Z").is_err());
        assert!(parse_rfc3339("2016-08-16T18:57:05Zjunk").is_err());
        assert!(parse_rfc3339("2016-08-16 18:57:05Z").is_err());
    }
    #[test]
    fn range_errors_use_go_message_form() {
        let err = parse_rfc3339("2016-13-16T18:57:05Z").unwrap_err();
        assert_eq!(
            err.to_string(),
            "parsing time \"2016-13-16T18:57:05Z\": month out of range"
        );
        assert!(parse_rfc3339("2016-08-16T18:60:05Z")
            .unwrap_err()
            .to_string()
            .contains("minute out of range"));
        assert!(parse_rfc3339("2016-08-16T18:57:60Z")
            .unwrap_err()
            .to_string()
            .contains("second out of range"));
    }

    #[test]
    fn each_layout_element_is_reported_by_name() {
        for (value, elem) in [
            ("2016/08-16T18:57:05Z", "-"),
            ("2016-0x-16T18:57:05Z", "01"),
            ("2016-08-16 18:57:05Z", "T"),
            ("2016-08-16Txx:57:05Z", "15"),
            ("2016-08-16T18-57:05Z", ":"),
            ("2016-08-16T18:57:05q", "Z07:00"),
        ] {
            let err = parse_rfc3339(value).unwrap_err();
            assert_eq!(err.layout_elem, elem, "for {value}");
        }
    }

    #[test]
    fn trailing_text_is_reported() {
        let err = parse_rfc3339("2016-08-16T18:57:05Zjunk").unwrap_err();
        assert!(err.to_string().contains("extra text"), "{err}");
    }

    #[test]
    fn a_dot_with_no_digits_fails() {
        assert!(parse_rfc3339("2016-08-16T18:57:05.Z").is_err());
    }

    #[test]
    fn display_renders_rfc3339() {
        let t = parse_rfc3339("2016-08-16T18:57:05Z").unwrap();
        assert_eq!(t.to_string(), "2016-08-16T18:57:05Z");
        assert_eq!(t.unix(), 1_471_373_825);
    }

    #[test]
    fn negative_offsets_round_trip() {
        let t = parse_rfc3339("2016-08-16T18:57:05-05:30").unwrap();
        assert_eq!(t.format_rfc3339(), "2016-08-16T18:57:05-05:30");
    }

    #[test]
    fn fractional_seconds_are_truncated_to_nanoseconds() {
        let t = parse_rfc3339("2016-08-16T18:57:05.1234567891Z").unwrap();
        assert_eq!(t.nanosecond(), 123_456_789);
        assert_eq!(t.format_rfc3339_nano(), "2016-08-16T18:57:05.123456789Z");
    }

    #[test]
    fn rfc3339_formatting_drops_sub_second_precision() {
        // Go's `time.RFC3339` layout has no fractional-seconds element.
        for (input, want) in [
            ("2016-08-16T18:57:05.25Z", "2016-08-16T18:57:05Z"),
            ("2016-08-16T18:57:05.999999999Z", "2016-08-16T18:57:05Z"),
            ("2016-08-16T18:57:05.5+02:00", "2016-08-16T18:57:05+02:00"),
        ] {
            assert_eq!(
                parse_rfc3339(input).unwrap().format_rfc3339(),
                want,
                "{input}"
            );
        }
    }

    #[test]
    fn nano_formatting_places_the_fraction_before_the_zone() {
        let t = parse_rfc3339("2016-08-16T18:57:05.5+02:00").unwrap();
        assert_eq!(t.format_rfc3339_nano(), "2016-08-16T18:57:05.5+02:00");
    }
    #[test]
    fn the_zero_value_matches_gos_zero_time() {
        let zero = Time::default();
        assert_eq!(zero.format_rfc3339(), "0001-01-01T00:00:00Z");
        assert_eq!(zero.unix(), -62_135_596_800);
    }
}
