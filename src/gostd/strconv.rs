//! Go `strconv` semantics.
//!
//! Reproduced rather than mapped onto Rust's `str::parse`, because two of the
//! behaviours are observable and Rust's parsers do not share them: integer
//! parsing with `base == 0` detects the base from a `0x`/`0o`/`0b`/`0` prefix
//! and accepts `_` digit separators, and boolean parsing accepts Go's exact
//! set of six true and six false spellings.

use std::error::Error;
use std::fmt;

/// The error produced by the parsing functions in this module, rendered with
/// Go's `strconv.NumError` message format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumError {
    /// The function that failed, e.g. `ParseInt`.
    pub func: &'static str,
    /// The input that could not be parsed.
    pub num: String,
    /// Either `invalid syntax` or `value out of range`.
    pub err: &'static str,
}

/// Go's `strconv.ErrSyntax` message.
pub const ERR_SYNTAX: &str = "invalid syntax";
/// Go's `strconv.ErrRange` message.
pub const ERR_RANGE: &str = "value out of range";

impl fmt::Display for NumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "strconv.{}: parsing {:?}: {}",
            self.func, self.num, self.err
        )
    }
}

impl Error for NumError {}

fn syntax(func: &'static str, num: &str) -> NumError {
    NumError {
        func,
        num: num.to_owned(),
        err: ERR_SYNTAX,
    }
}

fn range(func: &'static str, num: &str) -> NumError {
    NumError {
        func,
        num: num.to_owned(),
        err: ERR_RANGE,
    }
}

/// Strips a base prefix, returning the detected base and the remaining digits.
fn detect_base(s: &str) -> (u32, &str) {
    let b = s.as_bytes();
    if b.len() >= 3 && b[0] == b'0' {
        match b[1] {
            b'x' | b'X' => return (16, &s[2..]),
            b'o' | b'O' => return (8, &s[2..]),
            b'b' | b'B' => return (2, &s[2..]),
            _ => {}
        }
    }
    if b.len() >= 2 && b[0] == b'0' {
        match b[1] {
            b'x' | b'X' | b'o' | b'O' | b'b' | b'B' => return (0, ""), // prefix with no digits
            _ => return (8, &s[1..]),
        }
    }
    (10, s)
}

/// Go's `strconv.underscoreOK`: an `_` may appear only between digits, or
/// between a base prefix and a digit. Go checks this against the whole input —
/// sign and base prefix included — which is why this is applied before the
/// prefix is stripped rather than to the digits alone.
fn underscore_ok(s: &str) -> bool {
    // The class of the previous byte: `^` start of input, `0` digit or base
    // prefix, `_` underscore, `!` anything else.
    let mut saw = b'^';
    let mut b = s.as_bytes();

    // Optional sign.
    if matches!(b.first(), Some(b'+' | b'-')) {
        b = &b[1..];
    }

    // Optional base prefix, which counts as a digit for the rule above.
    let mut i = 0;
    let mut hex = false;
    if b.len() >= 2 && b[0] == b'0' && matches!(lower(b[1]), b'b' | b'o' | b'x') {
        i = 2;
        saw = b'0';
        hex = lower(b[1]) == b'x';
    }

    while i < b.len() {
        let c = b[i];
        if c.is_ascii_digit() || (hex && matches!(lower(c), b'a'..=b'f')) {
            saw = b'0';
        } else if c == b'_' {
            // An underscore must follow a digit …
            if saw != b'0' {
                return false;
            }
            saw = b'_';
        } else if saw == b'_' {
            // … and must be followed by one.
            return false;
        } else {
            saw = b'!';
        }
        i += 1;
    }
    saw != b'_'
}

/// Go's `strconv.lower`: ASCII case folding by setting the case bit.
fn lower(c: u8) -> u8 {
    c | (b'x' - b'X')
}

/// Removes Go's `_` digit separators. Their placement is validated separately
/// by [`underscore_ok`], against the whole input.
fn strip_underscores(digits: &str) -> String {
    digits.replace('_', "")
}

/// Splits an optional leading sign from `s`.
fn split_sign(s: &str) -> (bool, &str) {
    match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    }
}

/// Go's `strconv.ParseUint(s, 0, bit_size)`.
pub fn parse_uint(s: &str, bit_size: u32) -> Result<u128, NumError> {
    const FUNC: &str = "ParseUint";
    if s.is_empty() {
        return Err(syntax(FUNC, s));
    }
    // A sign is never valid for an unsigned value; Go reports invalid syntax.
    if s.starts_with('-') || s.starts_with('+') {
        return Err(syntax(FUNC, s));
    }
    if s.contains('_') && !underscore_ok(s) {
        return Err(syntax(FUNC, s));
    }
    let (base, digits) = detect_base(s);
    if base == 0 || digits.is_empty() {
        return Err(syntax(FUNC, s));
    }
    // `0x_FF` and `0_755` are valid in Go: a separator may sit between the
    // base prefix and the first digit.
    let digits = strip_underscores(digits);
    if digits.is_empty() {
        return Err(syntax(FUNC, s));
    }
    let mut acc: u128 = 0;
    for c in digits.chars() {
        let d = c.to_digit(base).ok_or_else(|| syntax(FUNC, s))?;
        acc = acc
            .checked_mul(base as u128)
            .and_then(|a| a.checked_add(d as u128))
            .ok_or_else(|| range(FUNC, s))?;
    }
    let max = uint_max(bit_size);
    if acc > max {
        return Err(range(FUNC, s));
    }
    Ok(acc)
}

/// Go's `strconv.ParseInt(s, 0, bit_size)`.
pub fn parse_int(s: &str, bit_size: u32) -> Result<i128, NumError> {
    const FUNC: &str = "ParseInt";
    if s.is_empty() {
        return Err(syntax(FUNC, s));
    }
    let (neg, rest) = split_sign(s);
    if rest.is_empty() {
        return Err(syntax(FUNC, s));
    }
    // Parse the magnitude at full width, then range-check it against the
    // declared bit size; `parse_uint`'s second argument is the bit size, not
    // the base, which is always detected from the prefix.
    let magnitude = parse_uint(rest, 127).map_err(|e| NumError {
        func: FUNC,
        num: s.to_owned(),
        err: e.err,
    })?;

    let (min, max) = int_bounds(bit_size);
    if neg {
        if magnitude > min.unsigned_abs() {
            return Err(range(FUNC, s));
        }
        Ok((magnitude as i128).wrapping_neg())
    } else {
        if magnitude > max as u128 {
            return Err(range(FUNC, s));
        }
        Ok(magnitude as i128)
    }
}

fn uint_max(bit_size: u32) -> u128 {
    if bit_size >= 128 {
        u128::MAX
    } else {
        (1u128 << bit_size) - 1
    }
}

fn int_bounds(bit_size: u32) -> (i128, i128) {
    if bit_size >= 128 {
        (i128::MIN, i128::MAX)
    } else {
        let half = 1i128 << (bit_size - 1);
        (-half, half - 1)
    }
}

/// Go's `strconv.ParseBool`.
pub fn parse_bool(s: &str) -> Result<bool, NumError> {
    match s {
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(syntax("ParseBool", s)),
    }
}

/// True for the infinity spellings Go's `strconv.special` accepts: `inf` and
/// `infinity`, in any case, with an optional sign. These reach ±Inf without a
/// range error, unlike a finite literal that overflows.
fn is_special_form(s: &str) -> bool {
    let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
    unsigned.eq_ignore_ascii_case("inf") || unsigned.eq_ignore_ascii_case("infinity")
}

/// Go's `strconv.ParseFloat`.
pub fn parse_float(s: &str, bit_size: u32) -> Result<f64, NumError> {
    const FUNC: &str = "ParseFloat";
    // Go accepts `_` separators here too when the value is not a special form.
    let cleaned = if s.contains('_') {
        if !underscore_ok(s) {
            return Err(syntax(FUNC, s));
        }
        strip_underscores(s)
    } else {
        s.to_owned()
    };
    // Go's `special` accepts a sign only on infinity; Rust's parser also takes
    // `-nan`, which Go reports as invalid syntax.
    if cleaned.len() > 1
        && matches!(cleaned.as_bytes()[0], b'+' | b'-')
        && cleaned[1..].eq_ignore_ascii_case("nan")
    {
        return Err(syntax(FUNC, s));
    }
    let v: f64 = cleaned.parse().map_err(|_| syntax(FUNC, s))?;
    // Go returns ±Inf *and* `ErrRange` when a finite literal is more than half
    // an ULP beyond the largest value of the target size. Underflow to zero is
    // not an error, and an explicit infinity is not a range error either.
    let special = is_special_form(&cleaned);
    if v.is_infinite() && !special {
        return Err(range(FUNC, s));
    }
    if bit_size == 32 {
        let narrowed = v as f32;
        if narrowed.is_infinite() && !special {
            return Err(range(FUNC, s));
        }
        return Ok(narrowed as f64);
    }
    Ok(v)
}

/// Go's `isTrue` helper: `ParseBool` with the error discarded.
pub fn is_true(s: &str) -> bool {
    parse_bool(s).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Values captured from the Go implementation.
    #[test]
    fn parse_int_detects_the_base() {
        assert_eq!(parse_int("8080", 64).unwrap(), 8080);
        assert_eq!(parse_int("0x10", 64).unwrap(), 16);
        assert_eq!(parse_int("010", 64).unwrap(), 8);
        assert_eq!(parse_int("0b101", 64).unwrap(), 5);
        assert_eq!(parse_int("0o17", 64).unwrap(), 15);
        assert_eq!(parse_int("1_0", 64).unwrap(), 10);
        assert_eq!(parse_int("-30", 64).unwrap(), -30);
        assert_eq!(parse_int("+30", 64).unwrap(), 30);
    }

    #[test]
    fn parse_uint_rejects_signs() {
        assert_eq!(parse_uint("30", 32).unwrap(), 30);
        assert_eq!(parse_uint("0x10", 32).unwrap(), 16);
        let e = parse_uint("-30", 32).unwrap_err();
        assert_eq!(
            e.to_string(),
            "strconv.ParseUint: parsing \"-30\": invalid syntax"
        );
    }

    #[test]
    fn error_messages_match_go() {
        assert_eq!(
            parse_int("string", 64).unwrap_err().to_string(),
            "strconv.ParseInt: parsing \"string\": invalid syntax"
        );
        assert_eq!(
            parse_bool("string").unwrap_err().to_string(),
            "strconv.ParseBool: parsing \"string\": invalid syntax"
        );
        assert_eq!(
            parse_float("string", 32).unwrap_err().to_string(),
            "strconv.ParseFloat: parsing \"string\": invalid syntax"
        );
    }

    #[test]
    fn range_checks_follow_bit_size() {
        assert!(parse_int("128", 8).is_err());
        assert_eq!(parse_int("127", 8).unwrap(), 127);
        assert_eq!(parse_int("-128", 8).unwrap(), -128);
        assert!(parse_uint("256", 8).is_err());
        assert_eq!(parse_uint("255", 8).unwrap(), 255);
        assert_eq!(
            parse_int("128", 8).unwrap_err().to_string(),
            "strconv.ParseInt: parsing \"128\": value out of range"
        );
    }

    #[test]
    fn bool_truthiness_matches_go() {
        for s in ["1", "t", "T", "TRUE", "true", "True"] {
            assert!(parse_bool(s).unwrap(), "{s}");
            assert!(is_true(s), "{s}");
        }
        for s in ["0", "f", "F", "FALSE", "false", "False"] {
            assert!(!parse_bool(s).unwrap(), "{s}");
            assert!(!is_true(s), "{s}");
        }
        for s in ["", "yes", "no", "string"] {
            assert!(parse_bool(s).is_err(), "{s}");
            assert!(!is_true(s), "{s}");
        }
    }

    #[test]
    fn underscores_must_separate_digits() {
        assert!(parse_int("_10", 64).is_err());
        assert!(parse_int("10_", 64).is_err());
        assert!(parse_int("1__0", 64).is_err());
        assert!(parse_int("_0x10", 64).is_err());
        assert!(parse_int("0_x10", 64).is_err());
        assert!(parse_int("0x1__2", 64).is_err());
        assert!(parse_int("0xF_", 64).is_err());
        assert!(parse_int("0x_", 64).is_err());
        assert!(parse_int("0_", 64).is_err());
        assert!(parse_int("09_9", 64).is_err());
    }

    /// A separator may also sit between the base prefix and the first digit.
    /// Values captured from the Go implementation.
    #[test]
    fn underscores_may_follow_a_base_prefix() {
        assert_eq!(parse_int("0x_FF", 64).unwrap(), 255);
        assert_eq!(parse_int("0X_ff", 64).unwrap(), 255);
        assert_eq!(parse_int("0_755", 64).unwrap(), 493);
        assert_eq!(parse_int("0b_101", 64).unwrap(), 5);
        assert_eq!(parse_int("0o_17", 64).unwrap(), 15);
        assert_eq!(parse_int("0x1_F", 64).unwrap(), 31);
        assert_eq!(parse_int("0x_0", 64).unwrap(), 0);
        assert_eq!(parse_int("00_1", 64).unwrap(), 1);
        assert_eq!(parse_int("-0x_FF", 64).unwrap(), -255);
        assert_eq!(parse_int("+0x_FF", 64).unwrap(), 255);
        assert_eq!(parse_uint("0x_FF", 64).unwrap(), 255);
        assert_eq!(parse_uint("0_755", 64).unwrap(), 493);
        assert!(parse_uint("+0x_FF", 64).is_err());
    }

    #[test]
    fn float_narrowing_to_32_bits() {
        assert_eq!(parse_float("0.5", 32).unwrap(), 0.5);
        assert!(parse_float("1e40", 32).is_err());
        assert!(parse_float("1e40", 64).is_ok());
    }

    /// Go returns `ErrRange` when a finite literal overflows the target size,
    /// so an out-of-range value must not silently become an infinity.
    /// Values captured from the Go implementation.
    #[test]
    fn float_overflow_is_a_range_error() {
        for bits in [32, 64] {
            assert_eq!(
                parse_float("1e400", bits).unwrap_err().to_string(),
                "strconv.ParseFloat: parsing \"1e400\": value out of range"
            );
            assert!(parse_float("-1e400", bits).is_err());
            assert!(parse_float("1e99999999999999999999", bits).is_err());
        }
        assert!(parse_float("1e309", 64).is_err());
        assert!(parse_float("3.4028236e38", 32).is_err());
        assert!(parse_float("-3.5e38", 32).is_err());
        assert!(parse_float("1.7976931348623158e308", 32).is_err());

        // Within half an ULP of the largest value: rounded, not rejected.
        assert!(parse_float("3.4028235e38", 32).is_ok());
        assert_eq!(parse_float("1.7976931348623158e308", 64).unwrap(), f64::MAX);
    }

    /// Underflow is not an error in Go: the result is zero.
    #[test]
    fn float_underflow_is_not_an_error() {
        assert_eq!(parse_float("1e-400", 64).unwrap(), 0.0);
        assert_eq!(parse_float("1e-500", 32).unwrap(), 0.0);
        assert_eq!(parse_float("1e-46", 32).unwrap(), 0.0);
        assert_eq!(parse_float("1e-309", 32).unwrap(), 0.0);
        assert!(parse_float("1e-309", 64).unwrap() > 0.0);
        assert_eq!(parse_float("0e999999999", 64).unwrap(), 0.0);
    }

    /// An explicit infinity is not a range error; a signed `nan` is a syntax
    /// error, as in Go's `special`.
    #[test]
    fn float_special_forms_match_go() {
        for s in ["inf", "INF", "Infinity", "+Inf"] {
            assert!(parse_float(s, 64).unwrap().is_infinite(), "{s}");
            assert!(parse_float(s, 32).unwrap().is_infinite(), "{s}");
        }
        assert!(parse_float("-inf", 64).unwrap().is_sign_negative());
        assert!(parse_float("nan", 64).unwrap().is_nan());
        assert_eq!(
            parse_float("-nan", 64).unwrap_err().to_string(),
            "strconv.ParseFloat: parsing \"-nan\": invalid syntax"
        );
        assert!(parse_float("+nan", 32).is_err());
    }

    /// Float separators follow the same rule as integer ones.
    /// Values captured from the Go implementation.
    #[test]
    fn float_underscores_must_separate_digits() {
        assert_eq!(parse_float("1_0.5", 64).unwrap(), 10.5);
        assert_eq!(parse_float("0_1.5", 64).unwrap(), 1.5);
        assert_eq!(parse_float(".5_5", 64).unwrap(), 0.55);
        for s in ["1._5", "1_.5", "1.5_", "1e_3", "1_e3", "1__0"] {
            assert_eq!(
                parse_float(s, 64).unwrap_err().to_string(),
                format!("strconv.ParseFloat: parsing {s:?}: invalid syntax"),
            );
        }
    }
    #[test]
    fn empty_and_sign_only_inputs_are_syntax_errors() {
        assert!(parse_int("", 64).is_err());
        assert!(parse_int("-", 64).is_err());
        assert!(parse_int("+", 64).is_err());
        assert!(parse_uint("", 32).is_err());
        assert!(parse_uint("0x", 32).is_err());
        assert!(parse_uint("0b", 32).is_err());
    }

    #[test]
    fn digits_must_be_valid_for_the_detected_base() {
        assert!(parse_int("0b12", 64).is_err());
        assert!(parse_int("09", 64).is_err());
        assert!(parse_int("0xzz", 64).is_err());
    }

    #[test]
    fn very_large_magnitudes_are_range_errors() {
        assert!(parse_uint("340282366920938463463374607431768211456", 64).is_err());
        assert!(parse_int("-99999999999999999999999999999999999999999", 64).is_err());
    }

    #[test]
    fn floats_accept_go_special_forms() {
        assert!(parse_float("inf", 64).unwrap().is_infinite());
        assert!(parse_float("NaN", 64).unwrap().is_nan());
        assert_eq!(parse_float("1_0.5", 64).unwrap(), 10.5);
        assert!(parse_float("1__0", 64).is_err());
    }

    #[test]
    fn wide_bit_sizes_are_supported() {
        assert_eq!(parse_uint("255", 128).unwrap(), 255);
        assert_eq!(parse_int("127", 128).unwrap(), 127);
    }
}
