//! Port of `envconfig_1.8_test.go`.
//!
//! The original is gated on the `go1.8` build tag, because `*url.URL` only
//! gained `UnmarshalBinary` in Go 1.8. The gate has no target counterpart, so
//! the tests carry over ungated.

mod common;

use common::{clearenv, guard, setenv};
use envconfig::gostd::url;
use envconfig::{EnvConfig, Error, Url};

#[derive(Default, EnvConfig)]
struct SpecWithURL {
    url_value: Url,
    url_pointer: Option<Url>,
}

#[test]
fn test_parse_url() {
    let _g = guard();
    let mut s = SpecWithURL::default();

    clearenv();
    setenv(
        "ENV_CONFIG_URLVALUE",
        "https://github.com/kelseyhightower/envconfig",
    );
    setenv(
        "ENV_CONFIG_URLPOINTER",
        "https://github.com/kelseyhightower/envconfig",
    );

    if let Err(err) = envconfig::process("env_config", &mut s) {
        panic!("unexpected error: {err}");
    }

    let u = url::parse("https://github.com/kelseyhightower/envconfig")
        .unwrap_or_else(|e| panic!("unexpected error: {e}"));

    assert_eq!(s.url_value, u, "expected {u:?}, got {:?}", s.url_value);
    assert_eq!(
        s.url_pointer.as_ref().unwrap(),
        &u,
        "expected {u:?}, got {:?}",
        s.url_pointer
    );
}

#[test]
fn test_parse_url_error() {
    let _g = guard();
    let mut s = SpecWithURL::default();

    clearenv();
    setenv("ENV_CONFIG_URLPOINTER", "http_://foo");

    let err = envconfig::process("env_config", &mut s).unwrap_err();
    let Error::Parse(v) = err else {
        panic!("expected ParseError, got {err:?}");
    };
    assert_eq!(v.field_name, "UrlPointer");

    let expected_underlying_error = url::UrlError {
        op: "parse".to_owned(),
        url: "http_://foo".to_owned(),
        err: "first path segment in URL cannot contain colon".to_owned(),
    };

    assert_eq!(v.err.to_string(), expected_underlying_error.to_string());
}
