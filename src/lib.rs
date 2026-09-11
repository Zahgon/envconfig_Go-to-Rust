//! Decoding of environment variables based on a user-defined specification.
//!
//! A typical use is using environment variables for configuration settings.
//!
//! ```
//! use envconfig::EnvConfig;
//!
//! #[derive(Default, EnvConfig)]
//! struct Specification {
//!     debug: bool,
//!     port: i32,
//!     #[envconfig(default = "foobar")]
//!     default_var: String,
//! }
//!
//! let mut spec = Specification::default();
//! envconfig::process("myapp", &mut spec).unwrap();
//! ```
//!
//! # Specification attributes
//!
//! Go reads struct tags with `reflect` at run time. Rust has no run-time
//! reflection, so the same information is declared with `#[envconfig(…)]`
//! attributes and read by the derive macro at compile time.
//!
//! | Attribute | Go tag | Meaning |
//! |---|---|---|
//! | `name = "ALT"` | `envconfig:"ALT"` | alternate variable name, used without the prefix as a fallback |
//! | `default = "…"` | `default:"…"` | value used when the variable is unset |
//! | `required` | `required:"true"` | fail when unset and no default |
//! | `ignored` | `ignored:"true"` | skip the field entirely |
//! | `split_words` | `split_words:"true"` | split the field name into `_`-separated words |
//! | `desc = "…"` | `desc:"…"` | description shown in usage output |
//! | `embedded` | anonymous embedding | expand in place, keeping the parent prefix |
//! | `nested` | named struct field | expand in place, using this field's key as the prefix |
//! | `field_name = "TTL"` | — | the declared name, when the Rust identifier cannot spell it |
//!
//! A specification that is not a struct is rejected at compile time rather
//! than with [`Error::InvalidSpecification`]:
//!
//! ```compile_fail
//! use std::collections::HashMap;
//! let mut spec: HashMap<String, String> = HashMap::new();
//! envconfig::process("env_config", &mut spec).unwrap();
//! ```
//!
//! So is passing a specification by value instead of by unique reference:
//!
//! ```compile_fail
//! use envconfig::EnvConfig;
//! #[derive(Default, EnvConfig)]
//! struct Specification { debug: bool }
//! let spec = Specification::default();
//! envconfig::process("env_config", spec).unwrap();
//! ```

// Lets `#[derive(EnvConfig)]`, whose output names `::envconfig::…`, be used
// inside this crate's own tests and doctests.
extern crate self as envconfig;

pub mod decode;
pub mod dispatch;
pub mod error;
pub mod gostd;
pub mod usage;

pub use envconfig_derive::EnvConfig;
pub use error::{BoxError, Error, ParseError};
pub use gostd::duration::Duration;
pub use gostd::time::Time;
pub use gostd::url::Url;
pub use usage::{usage, usagef, usaget, DEFAULT_LIST_FORMAT, DEFAULT_TABLE_FORMAT};

/// Self-deserialisation from a string. Takes precedence over every other
/// decoding trait, matching Go's `Decoder` interface.
pub trait Decoder {
    /// Decodes `value` into `self`.
    fn decode(&mut self, value: &str) -> Result<(), BoxError>;
}

/// Self-deserialisation with the same semantics as [`Decoder`], but lower
/// precedence. Matches Go's `Setter` interface, which any `flag.Value` also
/// satisfies.
pub trait Setter {
    /// Sets `self` from `value`.
    fn set(&mut self, value: &str) -> Result<(), BoxError>;
}

/// Matches Go's `encoding.TextUnmarshaler`.
pub trait TextUnmarshaler {
    /// Decodes the UTF-8 `data` into `self`.
    fn unmarshal_text(&mut self, data: &[u8]) -> Result<(), BoxError>;
}

/// Matches Go's `encoding.BinaryUnmarshaler`.
pub trait BinaryUnmarshaler {
    /// Decodes the raw `data` into `self`.
    fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), BoxError>;
}

/// The built-in decoding rules, reached when a field implements none of the
/// four traits above. Corresponds to Go's `switch typ.Kind()`.
pub trait FieldDecode {
    /// Decodes `value` into `self`.
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError>;
}

/// Information about one configuration variable.
///
/// Corresponds to Go's unexported `varInfo`, but the type description is
/// resolved at compile time rather than from `reflect.Type`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarInfo {
    /// The declared field name, as it appears in error messages.
    pub name: &'static str,
    /// The alternate variable name, upper-cased, or empty.
    pub alt: &'static str,
    /// The fully derived variable name, including the prefix.
    pub key: String,
    /// The human-readable type, as shown in usage output.
    pub type_desc: &'static str,
    /// The declared default, or empty.
    pub default: &'static str,
    /// The declared `required` text, verbatim, or empty.
    pub required: &'static str,
    /// The declared description, or empty.
    pub desc: &'static str,
}

/// A specification that can be gathered and populated.
///
/// Implemented by `#[derive(EnvConfig)]`.
pub trait EnvConfig {
    /// Appends this specification's variables to `out`, in declaration order.
    fn gather(&self, prefix: &str, out: &mut Vec<VarInfo>);

    /// Populates this specification from the environment.
    fn process_env(&mut self, prefix: &str) -> Result<(), Error>;
}

/// Joins the prefix and the field's key, then upper-cases the result.
///
/// An alternate name replaces the field-derived base entirely, as in Go.
#[doc(hidden)]
pub fn make_key(prefix: &str, base: &str, alt: &str) -> String {
    let key = if alt.is_empty() { base } else { alt };
    if prefix.is_empty() {
        key.to_uppercase()
    } else {
        format!("{prefix}_{key}").to_uppercase()
    }
}

/// Looks a variable up, distinguishing set-but-empty from unset.
fn lookup_env(key: &str) -> Option<String> {
    std::env::var_os(key).map(|v| v.to_string_lossy().into_owned())
}

/// Resolves the value for one variable.
///
/// Returns `Ok(None)` when the field should be left untouched, which is Go's
/// `continue`, and an error when a required variable is missing.
#[doc(hidden)]
pub fn resolve(
    key: &str,
    alt: &str,
    default: &str,
    required: &str,
) -> Result<Option<String>, Error> {
    let mut found = lookup_env(key);
    if found.is_none() && !alt.is_empty() {
        found = lookup_env(alt);
    }
    let ok = found.is_some();
    let mut value = found.unwrap_or_default();

    if !default.is_empty() && !ok {
        value = default.to_owned();
    }

    if !ok && default.is_empty() {
        if gostd::strconv::is_true(required) {
            let reported = if alt.is_empty() { key } else { alt };
            return Err(Error::RequiredMissing(reported.to_owned()));
        }
        return Ok(None);
    }

    Ok(Some(value))
}

/// Populates `spec` from environment variables.
pub fn process<T: EnvConfig + ?Sized>(prefix: &str, spec: &mut T) -> Result<(), Error> {
    spec.process_env(prefix)
}

/// Same as [`process`], but panics if an error occurs.
///
/// # Panics
///
/// Panics when [`process`] returns an error.
pub fn must_process<T: EnvConfig + ?Sized>(prefix: &str, spec: &mut T) {
    if let Err(e) = process(prefix, spec) {
        panic!("{e}");
    }
}

/// Gathers the variables `spec` would read, in declaration order.
pub fn gather_info<T: EnvConfig + ?Sized>(prefix: &str, spec: &T) -> Vec<VarInfo> {
    let mut out = Vec::new();
    spec.gather(prefix, &mut out);
    out
}

/// Checks that no environment variable carrying the prefix is set that the
/// specification does not know how to parse.
///
/// This is likely only meaningful with a non-empty prefix. Ignored fields are
/// not known keys, so setting one is reported as unknown.
pub fn check_disallowed<T: EnvConfig + ?Sized>(prefix: &str, spec: &T) -> Result<(), Error> {
    let known: std::collections::HashSet<String> = gather_info(prefix, spec)
        .into_iter()
        .map(|info| info.key)
        .collect();

    let scoped = if prefix.is_empty() {
        String::new()
    } else {
        format!("{}_", prefix.to_uppercase())
    };

    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if !name.starts_with(&scoped) {
            continue;
        }
        if !known.contains(name.as_ref()) {
            return Err(Error::UnknownVariable(name.into_owned()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_key_joins_and_upper_cases() {
        assert_eq!(make_key("env_config", "Debug", ""), "ENV_CONFIG_DEBUG");
        assert_eq!(make_key("", "RequiredVar", ""), "REQUIREDVAR");
        assert_eq!(
            make_key("env_config", "NoPrefixWithAlt", "SERVICE_HOST"),
            "ENV_CONFIG_SERVICE_HOST"
        );
        assert_eq!(
            make_key("ENV_CONFIG_OUTER", "Property", "INNER"),
            "ENV_CONFIG_OUTER_INNER"
        );
    }

    #[test]
    fn make_key_upper_cases_a_lower_case_alternate() {
        assert_eq!(
            make_key("env_config", "X", "MULTI_WORD_VAR_WITH_LOWER_CASE_ALT"),
            "ENV_CONFIG_MULTI_WORD_VAR_WITH_LOWER_CASE_ALT"
        );
    }
}
