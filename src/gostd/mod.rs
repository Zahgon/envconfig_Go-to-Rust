//! Narrow reimplementations of the Go standard-library behaviour that
//! `envconfig` exposes to its callers.
//!
//! The original has no third-party dependencies; everything it does comes from
//! Go's standard library. Several of those behaviours are observable — they
//! reach the caller as parsed values, as error text, or as rendered usage
//! bytes — so they are reproduced here rather than approximated with Rust
//! equivalents that have different semantics.
//!
//! Each module documents which observable behaviour it reproduces and why a
//! Rust counterpart was not used instead.

pub mod duration;
pub mod strconv;
pub mod tabwriter;
pub mod template;
pub mod time;
pub mod url;
