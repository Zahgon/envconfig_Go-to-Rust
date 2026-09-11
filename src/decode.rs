//! Built-in decoding, reached when a field implements none of the four
//! decoding traits — Go's `switch typ.Kind()`.

use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use crate::error::BoxError;
use crate::gostd::duration::{self, Duration};
use crate::gostd::strconv;
use crate::gostd::time::{self, Time};
use crate::gostd::url::{self, Url};
use crate::{BinaryUnmarshaler, FieldDecode, TextUnmarshaler};

impl FieldDecode for String {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        value.clone_into(self);
        Ok(())
    }
}

impl FieldDecode for bool {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        *self = strconv::parse_bool(value)?;
        Ok(())
    }
}

macro_rules! int_impl {
    ($($t:ty),*) => {$(
        impl FieldDecode for $t {
            fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
                *self = strconv::parse_int(value, <$t>::BITS)? as $t;
                Ok(())
            }
        }
    )*};
}

macro_rules! uint_impl {
    ($($t:ty),*) => {$(
        impl FieldDecode for $t {
            fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
                *self = strconv::parse_uint(value, <$t>::BITS)? as $t;
                Ok(())
            }
        }
    )*};
}

int_impl!(i8, i16, i32, i64, isize);
uint_impl!(u8, u16, u32, u64, usize);

impl FieldDecode for f32 {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        *self = strconv::parse_float(value, 32)? as f32;
        Ok(())
    }
}

impl FieldDecode for f64 {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        *self = strconv::parse_float(value, 64)?;
        Ok(())
    }
}

/// Go stores a duration as an `int64` but parses it with `time.ParseDuration`.
impl FieldDecode for Duration {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        *self = duration::parse_duration(value)?;
        Ok(())
    }
}

/// Go reaches `time.Time` through `encoding.TextUnmarshaler`, so this type
/// takes the same rung of the dispatch ladder.
impl TextUnmarshaler for Time {
    fn unmarshal_text(&mut self, data: &[u8]) -> Result<(), BoxError> {
        let text = std::str::from_utf8(data)?;
        *self = time::parse_rfc3339(text)?;
        Ok(())
    }
}

/// Go reaches `*url.URL` through `encoding.BinaryUnmarshaler`.
impl BinaryUnmarshaler for Url {
    fn unmarshal_binary(&mut self, data: &[u8]) -> Result<(), BoxError> {
        let text = std::str::from_utf8(data)?;
        *self = url::parse(text)?;
        Ok(())
    }
}

/// Comma-separated list. A blank or whitespace-only value yields an empty
/// collection rather than leaving the previous one in place, matching Go's
/// unconditional `field.Set(sl)`.
impl<T: FieldDecode + Default> FieldDecode for Vec<T> {
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        let mut out = Vec::new();
        if !value.trim().is_empty() {
            for part in value.split(',') {
                let mut item = T::default();
                item.decode_field(part)?;
                out.push(item);
            }
        }
        *self = out;
        Ok(())
    }
}

/// Decodes `key:value` pairs into a map-like collection.
fn decode_pairs<K, V, F>(value: &str, mut insert: F) -> Result<(), BoxError>
where
    K: FieldDecode + Default,
    V: FieldDecode + Default,
    F: FnMut(K, V),
{
    if value.trim().is_empty() {
        return Ok(());
    }
    for pair in value.split(',') {
        let mut parts = pair.split(':');
        let (Some(raw_key), Some(raw_value)) = (parts.next(), parts.next()) else {
            return Err(format!("invalid map item: {pair:?}").into());
        };
        if parts.next().is_some() {
            return Err(format!("invalid map item: {pair:?}").into());
        }
        let mut k = K::default();
        k.decode_field(raw_key)?;
        let mut v = V::default();
        v.decode_field(raw_value)?;
        insert(k, v);
    }
    Ok(())
}

impl<K, V> FieldDecode for HashMap<K, V>
where
    K: FieldDecode + Default + Eq + Hash,
    V: FieldDecode + Default,
{
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        let mut out = HashMap::new();
        decode_pairs(value, |k, v| {
            out.insert(k, v);
        })?;
        *self = out;
        Ok(())
    }
}

impl<K, V> FieldDecode for BTreeMap<K, V>
where
    K: FieldDecode + Default + Ord,
    V: FieldDecode + Default,
{
    fn decode_field(&mut self, value: &str) -> Result<(), BoxError> {
        let mut out = BTreeMap::new();
        decode_pairs(value, |k, v| {
            out.insert(k, v);
        })?;
        *self = out;
        Ok(())
    }
}

/// Go's `[]byte` special case: the raw bytes of the value, never comma-split.
///
/// Called directly by the derive macro for `Vec<u8>` fields, because Rust's
/// coherence rules do not allow it as a second `impl` alongside the generic
/// `Vec<T>` one.
pub fn decode_byte_slice(target: &mut Vec<u8>, value: &str) -> Result<(), BoxError> {
    *target = value.as_bytes().to_vec();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode<T: FieldDecode + Default>(value: &str) -> T {
        let mut t = T::default();
        t.decode_field(value).expect("decode");
        t
    }

    #[test]
    fn integers_use_go_base_detection() {
        assert_eq!(decode::<i32>("8080"), 8080);
        assert_eq!(decode::<i32>("0x10"), 16);
        assert_eq!(decode::<i32>("010"), 8);
        assert_eq!(decode::<u32>("30"), 30);
    }

    #[test]
    fn out_of_range_values_fail_for_the_declared_width() {
        let mut v: i8 = 0;
        assert!(v.decode_field("300").is_err());
        let mut u: u32 = 0;
        assert!(u.decode_field("-30").is_err());
    }

    #[test]
    fn slices_split_on_commas() {
        assert_eq!(
            decode::<Vec<String>>("John,Adam,Will"),
            vec!["John", "Adam", "Will"]
        );
        assert_eq!(decode::<Vec<i32>>("5,10,20"), vec![5, 10, 20]);
    }

    #[test]
    fn blank_slices_and_maps_become_empty_not_absent() {
        assert!(decode::<Vec<i32>>("").is_empty());
        assert!(decode::<Vec<i32>>("   ").is_empty());
        assert!(decode::<HashMap<String, i32>>("").is_empty());
    }

    #[test]
    fn a_populated_collection_is_replaced_not_extended() {
        let mut v: Vec<i32> = vec![1, 2, 3];
        v.decode_field("7").unwrap();
        assert_eq!(v, vec![7]);
        let mut v: Vec<i32> = vec![1, 2, 3];
        v.decode_field("").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn maps_split_on_commas_then_colons() {
        let m = decode::<HashMap<String, i32>>("red:1,green:2,blue:3");
        assert_eq!(m.len(), 3);
        assert_eq!(m["red"], 1);
        assert_eq!(m["green"], 2);
        assert_eq!(m["blue"], 3);
    }

    #[test]
    fn malformed_map_items_report_go_message() {
        let mut m: HashMap<String, i32> = HashMap::new();
        let err = m.decode_field("red").unwrap_err();
        assert_eq!(err.to_string(), "invalid map item: \"red\"");
        let err = m.decode_field("a:b:c").unwrap_err();
        assert_eq!(err.to_string(), "invalid map item: \"a:b:c\"");
    }

    #[test]
    fn byte_slices_take_the_raw_value() {
        let mut b: Vec<u8> = Vec::new();
        decode_byte_slice(&mut b, "this is a test value").unwrap();
        assert_eq!(String::from_utf8(b).unwrap(), "this is a test value");
    }

    #[test]
    fn durations_use_go_syntax() {
        assert_eq!(decode::<Duration>("2m"), duration::MINUTE * 2);
        assert_eq!(decode::<Duration>("1h30m").nanoseconds(), 5_400_000_000_000);
    }

    #[test]
    fn time_and_url_go_through_their_unmarshalers() {
        let mut t = Time::default();
        t.unmarshal_text(b"2016-08-16T18:57:05Z").unwrap();
        assert!(t.equal(Time::date(2016, 8, 16, 18, 57, 5, 0)));

        let mut u = Url::default();
        u.unmarshal_binary(b"https://github.com/kelseyhightower/envconfig")
            .unwrap();
        assert_eq!(u.host, "github.com");
    }
    #[test]
    fn floats_and_wide_integers_decode() {
        assert_eq!(decode::<f64>("0.5"), 0.5);
        assert_eq!(decode::<f32>("0.5"), 0.5);
        assert_eq!(decode::<i64>("-9223372036854775808"), i64::MIN);
        assert_eq!(decode::<u64>("18446744073709551615"), u64::MAX);
        assert_eq!(decode::<isize>("-7"), -7);
        assert_eq!(decode::<usize>("7"), 7);
        assert_eq!(decode::<i16>("-300"), -300);
        assert_eq!(decode::<u16>("300"), 300);
        assert_eq!(decode::<i8>("-8"), -8);
        assert_eq!(decode::<u8>("8"), 8);
    }

    #[test]
    fn btree_maps_decode_like_hash_maps() {
        let m = decode::<BTreeMap<String, i32>>("red:1,green:2");
        assert_eq!(m.len(), 2);
        assert_eq!(m["red"], 1);
        assert!(decode::<BTreeMap<String, i32>>("  ").is_empty());
        let mut m: BTreeMap<String, i32> = BTreeMap::new();
        assert!(m.decode_field("red").is_err());
    }

    #[test]
    fn bools_decode_with_go_truthiness() {
        assert!(decode::<bool>("True"));
        assert!(!decode::<bool>("F"));
        let mut b = false;
        assert!(b.decode_field("banana").is_err());
    }

    #[test]
    fn nested_collections_decode_elementwise() {
        assert_eq!(decode::<Vec<bool>>("true,false"), vec![true, false]);
        let mut v: Vec<i32> = Vec::new();
        assert!(v.decode_field("1,notanumber").is_err());
    }

    #[test]
    fn unmarshaler_errors_propagate() {
        let mut t = Time::default();
        assert!(t.unmarshal_text(b"nope").is_err());
        let mut u = Url::default();
        assert!(u.unmarshal_binary(b"http_://foo").is_err());
    }
}
