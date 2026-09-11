//! Error types.

use std::error::Error as StdError;
use std::fmt;

/// The boxed cause carried by [`ParseError`], standing in for Go's `error`.
pub type BoxError = Box<dyn StdError + Send + Sync>;

/// Raised when an environment variable cannot be converted to the type
/// required by a field during assignment.
#[derive(Debug)]
pub struct ParseError {
    /// The environment variable that supplied the value.
    pub key_name: String,
    /// The declared name of the field being assigned.
    pub field_name: &'static str,
    /// The field's type, as written in the specification.
    pub type_name: &'static str,
    /// The value that could not be converted.
    pub value: String,
    /// The underlying conversion failure.
    pub err: BoxError,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "envconfig.Process: assigning {} to {}: converting '{}' to type {}. details: {}",
            self.key_name, self.field_name, self.value, self.type_name, self.err
        )
    }
}

impl StdError for ParseError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.err.as_ref())
    }
}

/// Everything `envconfig` can fail with.
#[derive(Debug)]
pub enum Error {
    /// The specification is not a struct that can be populated.
    ///
    /// Go returns this at run time when the value handed to `Process` is not a
    /// pointer to a struct. Rust rejects that at compile time instead, so this
    /// variant exists for API parity and to carry the original message text.
    InvalidSpecification,
    /// A field marked required had no value and no default.
    RequiredMissing(String),
    /// A value could not be converted to the field's type.
    Parse(ParseError),
    /// `check_disallowed` found a variable in the prefix that is not a known key.
    UnknownVariable(String),
    /// A usage template failed to parse or execute.
    Usage(crate::gostd::template::TemplateError),
    /// Writing usage output failed.
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSpecification => f.write_str("specification must be a struct pointer"),
            Self::RequiredMissing(key) => write!(f, "required key {key} missing value"),
            Self::Parse(e) => e.fmt(f),
            Self::UnknownVariable(name) => write!(f, "unknown environment variable {name}"),
            Self::Usage(e) => e.fmt(f),
            Self::Io(e) => f.write_str(e),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Usage(e) => Some(e),
            _ => None,
        }
    }
}

impl From<crate::gostd::template::TemplateError> for Error {
    fn from(e: crate::gostd::template::TemplateError) -> Self {
        Self::Usage(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact message formats the Go suite asserts on.
    #[test]
    fn messages_match_go() {
        assert_eq!(
            Error::InvalidSpecification.to_string(),
            "specification must be a struct pointer"
        );
        assert_eq!(
            Error::RequiredMissing("BAR".to_owned()).to_string(),
            "required key BAR missing value"
        );
        assert_eq!(
            Error::UnknownVariable("ENV_CONFIG_ZEBUG".to_owned()).to_string(),
            "unknown environment variable ENV_CONFIG_ZEBUG"
        );
    }

    #[test]
    fn parse_error_matches_go_format() {
        let e = ParseError {
            key_name: "ENV_CONFIG_DEBUG".to_owned(),
            field_name: "Debug",
            type_name: "bool",
            value: "string".to_owned(),
            err: "boom".into(),
        };
        assert_eq!(
            e.to_string(),
            "envconfig.Process: assigning ENV_CONFIG_DEBUG to Debug: converting 'string' to type bool. details: boom"
        );
    }
    #[test]
    fn all_variants_render_and_expose_their_source() {
        use std::error::Error as _;

        let parse = Error::Parse(ParseError {
            key_name: "K".to_owned(),
            field_name: "F",
            type_name: "bool",
            value: "v".to_owned(),
            err: "boom".into(),
        });
        assert!(parse.to_string().contains("details: boom"));
        assert!(parse.source().is_some());

        let usage = Error::Usage(
            crate::gostd::template::Template::parse(
                "envconfig",
                "{{nope .}}",
                crate::usage::default_functions(),
            )
            .unwrap_err(),
        );
        assert!(usage.to_string().contains("not defined"));
        assert!(usage.source().is_some());

        let io = Error::Io("disk gone".to_owned());
        assert_eq!(io.to_string(), "disk gone");
        assert!(io.source().is_none());
        assert!(Error::InvalidSpecification.source().is_none());
    }

    #[test]
    fn parse_error_exposes_its_cause() {
        use std::error::Error as _;
        let e = ParseError {
            key_name: "K".to_owned(),
            field_name: "F",
            type_name: "bool",
            value: "v".to_owned(),
            err: "boom".into(),
        };
        assert_eq!(e.source().unwrap().to_string(), "boom");
    }
}
