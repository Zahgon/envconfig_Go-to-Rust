//! The shared specification fixture and environment helpers.
//!
//! Go's test binary runs the tests in a package sequentially, which is what
//! makes it safe for every test to clear and repopulate the process
//! environment. Rust's harness runs tests in parallel threads inside one
//! process, so that ordering has to be imposed explicitly — see [`guard`].

#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, MutexGuard, OnceLock};

use envconfig::gostd::url;
use envconfig::{BinaryUnmarshaler, BoxError, Decoder, Duration, EnvConfig, Setter, Time, Url};

/// Serialises the tests that touch the process environment.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Acquires exclusive use of the process environment for one test.
///
/// A failing test poisons the mutex; the lock is still handed out so that one
/// failure does not cascade into spurious failures in every other test.
pub fn guard() -> MutexGuard<'static, ()> {
    match env_lock().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Go's `os.Clearenv`.
pub fn clearenv() {
    let keys: Vec<_> = std::env::vars_os().map(|(k, _)| k).collect();
    for k in keys {
        std::env::remove_var(k);
    }
}

/// Go's `os.Setenv`.
pub fn setenv(key: &str, value: &str) {
    std::env::set_var(key, value);
}

// --- fixture types -------------------------------------------------------

/// Sets a fixed value regardless of the input, to prove `Decode` is reached.
#[derive(Default, Debug)]
pub struct HonorDecodeInStruct {
    pub value: String,
}

impl Decoder for HonorDecodeInStruct {
    fn decode(&mut self, _env: &str) -> Result<(), BoxError> {
        self.value = "decoded".to_owned();
        Ok(())
    }
}

/// Named to match the original so that `testdata/default_table.txt` keeps
/// rendering `CustomURL` in the type column.
#[allow(clippy::upper_case_acronyms)]
#[derive(Default, Debug, PartialEq, Eq)]
pub struct CustomURL {
    pub value: Option<Url>,
}

impl BinaryUnmarshaler for CustomURL {
    fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), BoxError> {
        let text = std::str::from_utf8(data)?;
        match url::parse(text) {
            Ok(u) => {
                self.value = Some(u);
                Ok(())
            }
            Err(e) => {
                // Go assigns the (nil) result before returning the error.
                self.value = None;
                Err(Box::new(e))
            }
        }
    }
}

/// Wraps its value in brackets. Implements `Setter` only.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Bracketed(pub String);

impl Setter for Bracketed {
    fn set(&mut self, value: &str) -> Result<(), BoxError> {
        self.0 = format!("[{value}]");
        Ok(())
    }
}

impl fmt::Display for Bracketed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Used to test the precedence of `Decode` over `Set`. It implements both, so
/// a correct implementation must call `Decode`.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Quoted(pub Bracketed);

impl Setter for Quoted {
    fn set(&mut self, value: &str) -> Result<(), BoxError> {
        self.0.set(value)
    }
}

impl Decoder for Quoted {
    fn decode(&mut self, value: &str) -> Result<(), BoxError> {
        self.set(&format!("\"{value}\""))
    }
}

impl fmt::Display for Quoted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// A struct that decodes itself, so it is a leaf rather than a nested spec.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct SetterStruct {
    pub inner: String,
}

impl Setter for SetterStruct {
    fn set(&mut self, value: &str) -> Result<(), BoxError> {
        self.inner = format!("setterstruct{{{value:?}}}");
        Ok(())
    }
}

// --- the specification ---------------------------------------------------

#[derive(Default, EnvConfig)]
pub struct Embedded {
    #[envconfig(desc = "some embedded value")]
    pub enabled: bool,
    pub embedded_port: i32,
    pub multi_word_var: String,
    #[envconfig(name = "MULTI_WITH_DIFFERENT_ALT")]
    pub multi_word_var_with_alt: String,
    #[envconfig(name = "EMBEDDED_WITH_ALT")]
    pub embedded_alt: String,
    #[envconfig(ignored)]
    pub embedded_ignored: String,
}

#[derive(Default, EnvConfig)]
pub struct EmbeddedButIgnored {
    pub first_embedded_but_ignored: String,
    pub second_embedded_but_ignored: String,
}

#[derive(Default, EnvConfig)]
pub struct NestedSpecification {
    #[envconfig(name = "inner")]
    pub property: String,
    #[envconfig(default = "fuzzybydefault")]
    pub property_with_default: String,
}

#[derive(Default, EnvConfig)]
pub struct Specification {
    #[envconfig(embedded, desc = "can we document a struct")]
    pub embedded: Embedded,
    #[envconfig(embedded, ignored)]
    pub embedded_but_ignored: EmbeddedButIgnored,
    pub debug: bool,
    pub port: i32,
    pub rate: f32,
    pub user: String,
    #[envconfig(field_name = "TTL")]
    pub ttl: u32,
    pub timeout: Duration,
    pub admin_users: Vec<String>,
    pub magic_numbers: Vec<i32>,
    pub empty_numbers: Vec<i32>,
    pub byte_slice: Vec<u8>,
    pub color_codes: HashMap<String, i32>,
    pub multi_word_var: String,
    #[envconfig(split_words)]
    pub multi_word_var_with_auto_split: u32,
    #[envconfig(split_words, field_name = "MultiWordACRWithAutoSplit")]
    pub multi_word_acr_with_auto_split: u32,
    pub some_pointer: Option<String>,
    #[envconfig(default = "foo2baz", desc = "foorbar is the word")]
    pub some_pointer_with_default: Option<String>,
    #[envconfig(name = "MULTI_WORD_VAR_WITH_ALT", desc = "what alt")]
    pub multi_word_var_with_alt: String,
    #[envconfig(name = "multi_word_var_with_lower_case_alt")]
    pub multi_word_var_with_lower_case_alt: String,
    #[envconfig(name = "SERVICE_HOST")]
    pub no_prefix_with_alt: String,
    #[envconfig(default = "foobar")]
    pub default_var: String,
    #[envconfig(required = "True")]
    pub required_var: String,
    #[envconfig(name = "BROKER", default = "127.0.0.1")]
    pub no_prefix_default: String,
    #[envconfig(required = "true", default = "foo2bar")]
    pub required_default: String,
    #[envconfig(ignored)]
    pub ignored: String,
    #[envconfig(nested, name = "outer")]
    pub nested_specification: NestedSpecification,
    pub after_nested: String,
    #[envconfig(name = "honor")]
    pub decode_struct: HonorDecodeInStruct,
    pub datetime: Time,
    #[envconfig(default = "one:two,three:four")]
    pub map_field: HashMap<String, String>,
    pub url_value: CustomURL,
    pub url_pointer: Option<CustomURL>,
}
