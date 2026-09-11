//! Port of `envconfig_test.go`.

mod common;

use std::collections::HashMap;

use common::*;
use envconfig::gostd::duration::{self, MINUTE};
use envconfig::gostd::time as gotime;
use envconfig::gostd::url;
use envconfig::{EnvConfig, Error, Time};

/// Returns the `ParseError` inside `err`, failing the test otherwise.
fn parse_error(err: Option<Error>) -> envconfig::ParseError {
    match err {
        Some(Error::Parse(p)) => p,
        other => panic!("expected ParseError, got {other:?}"),
    }
}

fn process(prefix: &str, spec: &mut Specification) -> Option<Error> {
    envconfig::process(prefix, spec).err()
}

#[test]
fn test_process() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("ENV_CONFIG_PORT", "8080");
    setenv("ENV_CONFIG_RATE", "0.5");
    setenv("ENV_CONFIG_USER", "Kelsey");
    setenv("ENV_CONFIG_TIMEOUT", "2m");
    setenv("ENV_CONFIG_ADMINUSERS", "John,Adam,Will");
    setenv("ENV_CONFIG_MAGICNUMBERS", "5,10,20");
    setenv("ENV_CONFIG_EMPTYNUMBERS", "");
    setenv("ENV_CONFIG_BYTESLICE", "this is a test value");
    setenv("ENV_CONFIG_COLORCODES", "red:1,green:2,blue:3");
    setenv("SERVICE_HOST", "127.0.0.1");
    setenv("ENV_CONFIG_TTL", "30");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_IGNORED", "was-not-ignored");
    setenv("ENV_CONFIG_OUTER_INNER", "iamnested");
    setenv("ENV_CONFIG_AFTERNESTED", "after");
    setenv("ENV_CONFIG_HONOR", "honor");
    setenv("ENV_CONFIG_DATETIME", "2016-08-16T18:57:05Z");
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_AUTO_SPLIT", "24");
    setenv("ENV_CONFIG_MULTI_WORD_ACR_WITH_AUTO_SPLIT", "25");
    setenv(
        "ENV_CONFIG_URLVALUE",
        "https://github.com/kelseyhightower/envconfig",
    );
    setenv(
        "ENV_CONFIG_URLPOINTER",
        "https://github.com/kelseyhightower/envconfig",
    );

    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }

    assert_eq!(s.no_prefix_with_alt, "127.0.0.1");
    assert!(s.debug, "expected true, got {}", s.debug);
    assert_eq!(s.port, 8080);
    assert_eq!(s.rate, 0.5);
    assert_eq!(s.ttl, 30);
    assert_eq!(s.user, "Kelsey");
    assert_eq!(s.timeout, MINUTE * 2);
    assert_eq!(s.required_var, "foo");
    assert_eq!(s.admin_users, vec!["John", "Adam", "Will"]);
    assert_eq!(s.magic_numbers, vec![5, 10, 20]);
    assert_eq!(
        s.empty_numbers.len(),
        0,
        "expected [], got {:?}",
        s.empty_numbers
    );
    assert_eq!(
        String::from_utf8(s.byte_slice.clone()).unwrap(),
        "this is a test value"
    );
    assert_eq!(s.ignored, "", "expected empty string, got {:?}", s.ignored);

    assert_eq!(s.color_codes.len(), 3);
    assert_eq!(s.color_codes["red"], 1);
    assert_eq!(s.color_codes["green"], 2);
    assert_eq!(s.color_codes["blue"], 3);

    assert_eq!(s.nested_specification.property, "iamnested");
    assert_eq!(
        s.nested_specification.property_with_default,
        "fuzzybydefault"
    );
    assert_eq!(s.after_nested, "after");
    assert_eq!(s.decode_struct.value, "decoded");

    let expected = Time::date(2016, 8, 16, 18, 57, 5, 0);
    assert!(
        s.datetime.equal(expected),
        "expected {}, got {}",
        expected.format_rfc3339(),
        s.datetime.format_rfc3339()
    );

    assert_eq!(s.multi_word_var_with_auto_split, 24);
    assert_eq!(s.multi_word_acr_with_auto_split, 25);

    let u = url::parse("https://github.com/kelseyhightower/envconfig").unwrap();
    assert_eq!(s.url_value.value.as_ref().unwrap(), &u);
    assert_eq!(s.url_pointer.as_ref().unwrap().value.as_ref().unwrap(), &u);
}

#[test]
fn test_parse_error_bool() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "string");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "Debug");
    assert!(!s.debug);
}

#[test]
fn test_parse_error_float32() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_RATE", "string");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "Rate");
    assert_eq!(s.rate, 0.0);
}

#[test]
fn test_parse_error_int() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_PORT", "string");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "Port");
    assert_eq!(s.port, 0);
}

#[test]
fn test_parse_error_uint() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_TTL", "-30");
    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "TTL");
    assert_eq!(s.ttl, 0);
}

#[test]
fn test_parse_error_split_words() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_AUTO_SPLIT", "shakespeare");
    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "MultiWordVarWithAutoSplit");
    assert_eq!(s.multi_word_var_with_auto_split, 0);
}

/// Go rejects a non-struct specification at run time with
/// `ErrInvalidSpecification`. Rust rejects it at compile time — see the
/// `compile_fail` doctest on the crate root — so what remains testable here is
/// that the error variant still carries the original message.
#[test]
fn test_err_invalid_specification() {
    assert_eq!(
        Error::InvalidSpecification.to_string(),
        "specification must be a struct pointer"
    );
}

/// Go's counterpart passes the specification by value; in Rust that does not
/// compile, which the crate-root `compile_fail` doctest pins.
#[test]
fn test_non_pointer_fails_properly() {
    assert_eq!(
        Error::InvalidSpecification.to_string(),
        "specification must be a struct pointer"
    );
}

#[test]
fn test_unset_vars() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("USER", "foo");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    // If the var is not defined the non-prefixed version should not be used
    // unless the attribute says so.
    assert_eq!(s.user, "");
}

#[test]
fn test_alternate_var_names() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_MULTI_WORD_VAR", "foo");
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_ALT", "bar");
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_LOWER_CASE_ALT", "baz");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    // Setting the alt version has no effect without the attribute.
    assert_eq!(s.multi_word_var, "");
    // With the attribute, it does.
    assert_eq!(s.multi_word_var_with_alt, "bar");
    // The alt value is not case sensitive and is treated as all uppercase.
    assert_eq!(s.multi_word_var_with_lower_case_alt, "baz");
}

#[test]
fn test_required_var() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foobar");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.required_var, "foobar");
}

#[test]
fn test_required_missing() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    assert!(
        process("env_config", &mut s).is_some(),
        "no failure when missing required variable"
    );
}

#[test]
fn test_blank_default_var() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "requiredvalue");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.default_var, "foobar");
    assert_eq!(s.some_pointer_with_default.as_deref(), Some("foo2baz"));
}

#[test]
fn test_non_blank_default_var() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEFAULTVAR", "nondefaultval");
    setenv("ENV_CONFIG_REQUIREDVAR", "requiredvalue");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.default_var, "nondefaultval");
}

#[test]
fn test_explicit_blank_default_var() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEFAULTVAR", "");
    setenv("ENV_CONFIG_REQUIREDVAR", "");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.default_var, "");
}

#[test]
fn test_alternate_name_default_var() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("BROKER", "betterbroker");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.no_prefix_default, "betterbroker");

    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.no_prefix_default, "127.0.0.1");
}

#[test]
fn test_required_default() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.required_default, "foo2bar");
}

#[test]
fn test_pointer_field_blank() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert!(
        s.some_pointer.is_none(),
        "expected None, got {:?}",
        s.some_pointer
    );
}

#[test]
fn test_empty_map_field_override() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_MAPFIELD", "");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(
        s.map_field.len(),
        0,
        "expected empty map, got map of size {}",
        s.map_field.len()
    );
}

#[test]
fn test_must_process() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("ENV_CONFIG_PORT", "8080");
    setenv("ENV_CONFIG_RATE", "0.5");
    setenv("ENV_CONFIG_USER", "Kelsey");
    setenv("SERVICE_HOST", "127.0.0.1");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    envconfig::must_process("env_config", &mut s);
    assert!(s.debug);
    assert_eq!(s.port, 8080);
}

/// The second half of Go's `TestMustProcess` panics on a map specification.
/// A map cannot reach `must_process` in Rust, so the panic is exercised with
/// the failure that can occur: a missing required variable.
#[test]
#[should_panic(expected = "required key ENV_CONFIG_REQUIREDVAR missing value")]
fn test_must_process_panics() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    envconfig::must_process("env_config", &mut s);
}

#[test]
fn test_embedded_struct() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "required");
    setenv("ENV_CONFIG_ENABLED", "true");
    setenv("ENV_CONFIG_EMBEDDEDPORT", "1234");
    setenv("ENV_CONFIG_MULTIWORDVAR", "foo");
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_ALT", "bar");
    setenv("ENV_CONFIG_MULTI_WITH_DIFFERENT_ALT", "baz");
    setenv("ENV_CONFIG_EMBEDDED_WITH_ALT", "foobar");
    setenv("ENV_CONFIG_SOMEPOINTER", "foobaz");
    setenv("ENV_CONFIG_EMBEDDED_IGNORED", "was-not-ignored");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert!(s.embedded.enabled);
    assert_eq!(s.embedded.embedded_port, 1234);
    assert_eq!(s.multi_word_var, "foo");
    assert_eq!(s.embedded.multi_word_var, "foo");
    assert_eq!(s.multi_word_var_with_alt, "bar");
    assert_eq!(s.embedded.multi_word_var_with_alt, "baz");
    assert_eq!(s.embedded.embedded_alt, "foobar");
    assert_eq!(s.some_pointer.as_deref(), Some("foobaz"));
    assert_eq!(s.embedded.embedded_ignored, "");
}

#[test]
fn test_embedded_but_ignored_struct() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "required");
    setenv("ENV_CONFIG_FIRSTEMBEDDEDBUTIGNORED", "was-not-ignored");
    setenv("ENV_CONFIG_SECONDEMBEDDEDBUTIGNORED", "was-not-ignored");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.embedded_but_ignored.first_embedded_but_ignored, "");
    assert_eq!(s.embedded_but_ignored.second_embedded_but_ignored, "");
}

#[derive(Default, EnvConfig)]
struct CustomValueSpec {
    foo: String,
    bar: Bracketed,
    baz: Quoted,
    #[envconfig(field_name = "Struct")]
    r#struct: SetterStruct,
}

#[test]
fn test_custom_value_fields() {
    let _g = guard();
    let mut s = CustomValueSpec::default();
    clearenv();
    setenv("ENV_CONFIG_FOO", "foo");
    setenv("ENV_CONFIG_BAR", "bar");
    setenv("ENV_CONFIG_BAZ", "baz");
    setenv("ENV_CONFIG_STRUCT", "inner");

    if let Err(err) = envconfig::process("env_config", &mut s) {
        panic!("{err}");
    }

    assert_eq!(s.foo, "foo");
    assert_eq!(s.bar.to_string(), "[bar]");
    assert_eq!(s.baz.to_string(), r#"["baz"]"#);
    assert_eq!(s.r#struct.inner, r#"setterstruct{"inner"}"#);
}

#[derive(Default, EnvConfig)]
struct CustomPointerSpec {
    foo: String,
    bar: Option<Bracketed>,
    baz: Option<Quoted>,
    #[envconfig(field_name = "Struct")]
    r#struct: Option<SetterStruct>,
}

#[test]
fn test_custom_pointer_fields() {
    let _g = guard();
    let mut s = CustomPointerSpec::default();
    clearenv();
    setenv("ENV_CONFIG_FOO", "foo");
    setenv("ENV_CONFIG_BAR", "bar");
    setenv("ENV_CONFIG_BAZ", "baz");
    setenv("ENV_CONFIG_STRUCT", "inner");

    if let Err(err) = envconfig::process("env_config", &mut s) {
        panic!("{err}");
    }

    assert_eq!(s.foo, "foo");
    assert_eq!(s.bar.as_ref().unwrap().to_string(), "[bar]");
    assert_eq!(s.baz.as_ref().unwrap().to_string(), r#"["baz"]"#);
    assert_eq!(
        s.r#struct.as_ref().unwrap().inner,
        r#"setterstruct{"inner"}"#
    );
}

#[test]
fn test_empty_prefix_uses_field_names() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("REQUIREDVAR", "foo");
    if let Some(err) = process("", &mut s) {
        panic!("Process failed: {err}");
    }
    assert_eq!(s.required_var, "foo");
}

#[test]
fn test_nested_struct_var_name() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "required");
    let val = "found with only short name";
    setenv("INNER", val);
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.nested_specification.property, val);
}

#[test]
fn test_text_unmarshaler_error() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_DATETIME", "I'M NOT A DATE");

    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "Datetime");

    let expected = gotime::ParseError {
        layout: gotime::RFC3339.to_owned(),
        value: "I'M NOT A DATE".to_owned(),
        layout_elem: "2006".to_owned(),
        value_elem: "I'M NOT A DATE".to_owned(),
        message: String::new(),
    };
    assert_eq!(v.err.to_string(), expected.to_string());
}

#[test]
fn test_binary_unmarshaler_error() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_URLPOINTER", "http://%41:8080/");

    let v = parse_error(process("env_config", &mut s));
    assert_eq!(v.field_name, "UrlPointer");

    let ue = v
        .err
        .downcast_ref::<url::UrlError>()
        .expect("expected error type to be url::UrlError");
    assert_eq!(ue.op, "parse");
}

#[test]
fn test_check_disallowed_only_allowed() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("UNRELATED_ENV_VAR", "true");
    if let Err(err) = envconfig::check_disallowed("env_config", &s) {
        panic!("expected no error, got {err}");
    }
}

#[test]
fn test_check_disallowed_mispelled() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("ENV_CONFIG_ZEBUG", "false");
    let err = envconfig::check_disallowed("env_config", &s).unwrap_err();
    assert_eq!(
        err.to_string(),
        "unknown environment variable ENV_CONFIG_ZEBUG"
    );
}

#[test]
fn test_check_disallowed_ignored() {
    let _g = guard();
    let s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("ENV_CONFIG_IGNORED", "false");
    let err = envconfig::check_disallowed("env_config", &s).unwrap_err();
    assert_eq!(
        err.to_string(),
        "unknown environment variable ENV_CONFIG_IGNORED"
    );
}

#[derive(Default, EnvConfig)]
struct RequiredAltSpec {
    #[envconfig(name = "BAR", required = "true")]
    foo: String,
}

#[test]
fn test_error_message_for_required_alt_var() {
    let _g = guard();
    let mut s = RequiredAltSpec::default();
    clearenv();
    let err = envconfig::process("env_config", &mut s)
        .expect_err("no failure when missing required variable");
    assert!(
        err.to_string().contains(" BAR "),
        "expected error message to contain BAR, got \"{err}\""
    );
}

/// Go's `BenchmarkGatherInfo`, kept as a correctness test since Rust
/// benchmarks are not part of the stable test harness.
#[test]
fn bench_gather_info() {
    let _g = guard();
    clearenv();
    setenv("ENV_CONFIG_DEBUG", "true");
    setenv("ENV_CONFIG_PORT", "8080");
    setenv("ENV_CONFIG_RATE", "0.5");
    setenv("ENV_CONFIG_USER", "Kelsey");
    setenv("ENV_CONFIG_TIMEOUT", "2m");
    setenv("ENV_CONFIG_ADMINUSERS", "John,Adam,Will");
    setenv("ENV_CONFIG_MAGICNUMBERS", "5,10,20");
    setenv("ENV_CONFIG_COLORCODES", "red:1,green:2,blue:3");
    setenv("SERVICE_HOST", "127.0.0.1");
    setenv("ENV_CONFIG_TTL", "30");
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_IGNORED", "was-not-ignored");
    setenv("ENV_CONFIG_OUTER_INNER", "iamnested");
    setenv("ENV_CONFIG_AFTERNESTED", "after");
    setenv("ENV_CONFIG_HONOR", "honor");
    setenv("ENV_CONFIG_DATETIME", "2016-08-16T18:57:05Z");
    setenv("ENV_CONFIG_MULTI_WORD_VAR_WITH_AUTO_SPLIT", "24");
    for _ in 0..1000 {
        let s = Specification::default();
        let infos = envconfig::gather_info("env_config", &s);
        assert_eq!(infos.len(), 36);
    }
}

/// Duration decoding is now this crate's own code rather than Go's, so it
/// carries its own assertions.
#[test]
fn duration_field_uses_go_syntax() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_TIMEOUT", "1h30m");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.timeout, duration::parse_duration("1h30m").unwrap());
    assert_eq!(s.timeout.nanoseconds(), 5_400_000_000_000);
}

/// Base-detecting integer parsing likewise.
#[test]
fn integer_fields_detect_the_base() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    setenv("ENV_CONFIG_PORT", "0x1f90");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    assert_eq!(s.port, 8080);
}

/// The keys the specification derives, in gather order. This is the contract
/// that `testdata/default_table.txt` renders.
#[test]
fn gathered_keys_match_the_original() {
    let _g = guard();
    clearenv();
    let s = Specification::default();
    let keys: Vec<String> = envconfig::gather_info("env_config", &s)
        .into_iter()
        .map(|i| i.key)
        .collect();
    let expected: Vec<&str> = vec![
        "ENV_CONFIG_ENABLED",
        "ENV_CONFIG_EMBEDDEDPORT",
        "ENV_CONFIG_MULTIWORDVAR",
        "ENV_CONFIG_MULTI_WITH_DIFFERENT_ALT",
        "ENV_CONFIG_EMBEDDED_WITH_ALT",
        "ENV_CONFIG_DEBUG",
        "ENV_CONFIG_PORT",
        "ENV_CONFIG_RATE",
        "ENV_CONFIG_USER",
        "ENV_CONFIG_TTL",
        "ENV_CONFIG_TIMEOUT",
        "ENV_CONFIG_ADMINUSERS",
        "ENV_CONFIG_MAGICNUMBERS",
        "ENV_CONFIG_EMPTYNUMBERS",
        "ENV_CONFIG_BYTESLICE",
        "ENV_CONFIG_COLORCODES",
        "ENV_CONFIG_MULTIWORDVAR",
        "ENV_CONFIG_MULTI_WORD_VAR_WITH_AUTO_SPLIT",
        "ENV_CONFIG_MULTI_WORD_ACR_WITH_AUTO_SPLIT",
        "ENV_CONFIG_SOMEPOINTER",
        "ENV_CONFIG_SOMEPOINTERWITHDEFAULT",
        "ENV_CONFIG_MULTI_WORD_VAR_WITH_ALT",
        "ENV_CONFIG_MULTI_WORD_VAR_WITH_LOWER_CASE_ALT",
        "ENV_CONFIG_SERVICE_HOST",
        "ENV_CONFIG_DEFAULTVAR",
        "ENV_CONFIG_REQUIREDVAR",
        "ENV_CONFIG_BROKER",
        "ENV_CONFIG_REQUIREDDEFAULT",
        "ENV_CONFIG_OUTER_INNER",
        "ENV_CONFIG_OUTER_PROPERTYWITHDEFAULT",
        "ENV_CONFIG_AFTERNESTED",
        "ENV_CONFIG_HONOR",
        "ENV_CONFIG_DATETIME",
        "ENV_CONFIG_MAPFIELD",
        "ENV_CONFIG_URLVALUE",
        "ENV_CONFIG_URLPOINTER",
    ];
    assert_eq!(keys, expected);
}

/// `HashMap` is the map type here; confirm the decoded contents rather than
/// any particular iteration order.
#[test]
fn map_field_default_is_parsed() {
    let _g = guard();
    let mut s = Specification::default();
    clearenv();
    setenv("ENV_CONFIG_REQUIREDVAR", "foo");
    if let Some(err) = process("env_config", &mut s) {
        panic!("{err}");
    }
    let expected: HashMap<String, String> = [
        ("one".to_owned(), "two".to_owned()),
        ("three".to_owned(), "four".to_owned()),
    ]
    .into_iter()
    .collect();
    assert_eq!(s.map_field, expected);
}
