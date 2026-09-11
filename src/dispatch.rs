//! Decoder selection.
//!
//! Go picks a field's decoder with a chain of run-time type assertions:
//! `Decoder`, then `Setter`, then `encoding.TextUnmarshaler`, then
//! `encoding.BinaryUnmarshaler`, and only then the built-in kinds. The order is
//! observable — a type implementing both `Decode` and `Set` must have `Decode`
//! called, and the test suite checks exactly that.
//!
//! Rust resolves this at compile time. The chain is expressed as a ladder of
//! wrapper types linked by [`Deref`]: method lookup starts at the top rung and
//! walks down until it finds a rung whose trait bound the field's type
//! satisfies. That yields the same precedence, decided statically, with no
//! run-time cost and no `unsafe`.
//!
//! The lookup only specialises when the field's type is concrete, which is why
//! the derive macro emits the call at each field rather than routing every
//! field through one generic function.

use std::ops::{Deref, DerefMut};

use crate::error::BoxError;
use crate::{BinaryUnmarshaler, Decoder, FieldDecode, Setter, TextUnmarshaler};

/// Rung 4: the built-in kinds.
pub struct Rung4<'a, T: ?Sized> {
    target: &'a mut T,
}
/// Rung 3: `BinaryUnmarshaler`.
pub struct Rung3<'a, T: ?Sized> {
    inner: Rung4<'a, T>,
}
/// Rung 2: `TextUnmarshaler`.
pub struct Rung2<'a, T: ?Sized> {
    inner: Rung3<'a, T>,
}
/// Rung 1: `Setter`.
pub struct Rung1<'a, T: ?Sized> {
    inner: Rung2<'a, T>,
}
/// Rung 0: `Decoder`, the highest precedence.
pub struct Rung0<'a, T: ?Sized> {
    inner: Rung1<'a, T>,
}

macro_rules! rung {
    ($outer:ident => $inner:ident) => {
        impl<'a, T: ?Sized> Deref for $outer<'a, T> {
            type Target = $inner<'a, T>;
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }
        impl<T: ?Sized> DerefMut for $outer<'_, T> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.inner
            }
        }
    };
}

rung!(Rung0 => Rung1);
rung!(Rung1 => Rung2);
rung!(Rung2 => Rung3);
rung!(Rung3 => Rung4);

/// Wraps a field so that [`Dispatch::dispatch`] selects its decoder.
pub fn probe<T: ?Sized>(target: &mut T) -> Rung0<'_, T> {
    Rung0 {
        inner: Rung1 {
            inner: Rung2 {
                inner: Rung3 {
                    inner: Rung4 { target },
                },
            },
        },
    }
}

/// Decodes `value` into the wrapped field.
///
/// Implemented once per rung; the compiler picks the highest rung whose bound
/// the field's type satisfies.
pub trait Dispatch {
    /// Decodes `value` into the wrapped field.
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError>;
}

impl<T: Decoder + ?Sized> Dispatch for Rung0<'_, T> {
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError> {
        self.inner.inner.inner.target.decode(value)
    }
}

impl<T: Setter + ?Sized> Dispatch for Rung1<'_, T> {
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError> {
        self.inner.inner.target.set(value)
    }
}

impl<T: TextUnmarshaler + ?Sized> Dispatch for Rung2<'_, T> {
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError> {
        self.inner.target.unmarshal_text(value.as_bytes())
    }
}

impl<T: BinaryUnmarshaler + ?Sized> Dispatch for Rung3<'_, T> {
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError> {
        self.target.unmarshal_binary(value.as_bytes())
    }
}

impl<T: FieldDecode + ?Sized> Dispatch for Rung4<'_, T> {
    fn dispatch(&mut self, value: &str) -> Result<(), BoxError> {
        self.target.decode_field(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Bracketed(String);
    impl Setter for Bracketed {
        fn set(&mut self, value: &str) -> Result<(), BoxError> {
            self.0 = format!("[{value}]");
            Ok(())
        }
    }

    /// Implements both `Decoder` and `Setter`; `Decoder` must win.
    #[derive(Default)]
    struct Quoted(Bracketed);
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

    #[derive(Default)]
    struct Stamp(String);
    impl TextUnmarshaler for Stamp {
        fn unmarshal_text(&mut self, data: &[u8]) -> Result<(), BoxError> {
            self.0 = format!("text:{}", String::from_utf8_lossy(data));
            Ok(())
        }
    }

    #[derive(Default)]
    struct Blob(String);
    impl BinaryUnmarshaler for Blob {
        fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), BoxError> {
            self.0 = format!("binary:{}", String::from_utf8_lossy(data));
            Ok(())
        }
    }

    /// Implements text *and* binary; text must win.
    #[derive(Default)]
    struct Both(String);
    impl TextUnmarshaler for Both {
        fn unmarshal_text(&mut self, data: &[u8]) -> Result<(), BoxError> {
            self.0 = format!("text:{}", String::from_utf8_lossy(data));
            Ok(())
        }
    }
    impl BinaryUnmarshaler for Both {
        fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), BoxError> {
            self.0 = format!("binary:{}", String::from_utf8_lossy(data));
            Ok(())
        }
    }

    #[test]
    fn precedence_follows_go() {
        let mut b = Bracketed::default();
        probe(&mut b).dispatch("bar").unwrap();
        assert_eq!(b.0, "[bar]");

        let mut q = Quoted::default();
        probe(&mut q).dispatch("baz").unwrap();
        assert_eq!(
            q.0 .0, "[\"baz\"]",
            "Decoder must take precedence over Setter"
        );

        let mut s = Stamp::default();
        probe(&mut s).dispatch("x").unwrap();
        assert_eq!(s.0, "text:x");

        let mut bl = Blob::default();
        probe(&mut bl).dispatch("y").unwrap();
        assert_eq!(bl.0, "binary:y");

        let mut both = Both::default();
        probe(&mut both).dispatch("z").unwrap();
        assert_eq!(
            both.0, "text:z",
            "TextUnmarshaler must take precedence over Binary"
        );
    }

    #[test]
    fn falls_through_to_builtin_kinds() {
        let mut s = String::new();
        probe(&mut s).dispatch("hello").unwrap();
        assert_eq!(s, "hello");

        let mut n: i32 = 0;
        probe(&mut n).dispatch("8080").unwrap();
        assert_eq!(n, 8080);
    }
}
