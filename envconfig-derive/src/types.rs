//! Compile-time type descriptions.
//!
//! Go builds the `[type]` column of the usage output from `reflect.Type` at
//! run time. The equivalent information is available here only as written
//! syntax, so the same descriptions are produced from the field's type tokens.

use syn::{GenericArgument, PathArguments, Type};

/// The identifier and generic arguments of a path type's final segment.
fn last_segment(ty: &Type) -> Option<(String, Vec<&Type>)> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let args = match &seg.arguments {
        PathArguments::AngleBracketed(a) => a
            .args
            .iter()
            .filter_map(|a| match a {
                GenericArgument::Type(t) => Some(t),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Some((seg.ident.to_string(), args))
}

/// Renders the human-readable type description used by `usage_type`.
pub fn describe(ty: &Type) -> String {
    // A slice or array of bytes is described as a string, matching Go's
    // special case for `[]byte`.
    if let Type::Reference(r) = ty {
        return describe(&r.elem);
    }
    if let Type::Slice(s) = ty {
        return sequence(&s.elem);
    }
    if let Type::Array(a) = ty {
        return sequence(&a.elem);
    }

    let Some((ident, args)) = last_segment(ty) else {
        return String::new();
    };

    match (ident.as_str(), args.len()) {
        ("String" | "str", _) => "String".to_owned(),
        ("bool", _) => "True or False".to_owned(),
        ("i8" | "i16" | "i32" | "i64" | "i128" | "isize", _) => "Integer".to_owned(),
        ("u8" | "u16" | "u32" | "u64" | "u128" | "usize", _) => "Unsigned Integer".to_owned(),
        ("f32" | "f64", _) => "Float".to_owned(),
        ("Vec" | "VecDeque", 1) => sequence(args[0]),
        ("HashMap" | "BTreeMap", 2) => format!(
            "Comma-separated list of {}:{} pairs",
            describe(args[0]),
            describe(args[1])
        ),
        // Go dereferences pointers before describing them.
        ("Option" | "Box" | "Cow", 1) => describe(args[0]),
        // Any other named type describes itself, as Go does for a named type
        // that decodes itself.
        _ => ident,
    }
}

/// `Comma-separated list of …`, with Go's `[]byte` special case.
fn sequence(elem: &Type) -> String {
    if let Some((ident, _)) = last_segment(elem) {
        if ident == "u8" {
            return "String".to_owned();
        }
    }
    format!("Comma-separated list of {}", describe(elem))
}

/// Renders the field's type as source text, for `ParseError::type_name`.
pub fn render(ty: &Type) -> String {
    let text = quote::quote!(#ty).to_string();
    // `quote` separates tokens with spaces; tighten the punctuation so the
    // rendered name reads like the written type.
    text.replace(" < ", "<")
        .replace(" > ", ">")
        .replace(" >", ">")
        .replace("< ", "<")
        .replace(" ,", ",")
        .replace(" ::", "::")
        .replace(":: ", "::")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(src: &str) -> String {
        describe(&syn::parse_str::<Type>(src).expect("type"))
    }

    /// Expected values read from `testdata/default_table.txt`.
    #[test]
    fn descriptions_match_the_golden_table() {
        assert_eq!(desc("String"), "String");
        assert_eq!(desc("bool"), "True or False");
        assert_eq!(desc("i32"), "Integer");
        assert_eq!(desc("u32"), "Unsigned Integer");
        assert_eq!(desc("f32"), "Float");
        assert_eq!(desc("Vec<u8>"), "String");
        assert_eq!(desc("Vec<String>"), "Comma-separated list of String");
        assert_eq!(desc("Vec<i32>"), "Comma-separated list of Integer");
        assert_eq!(
            desc("HashMap<String, i32>"),
            "Comma-separated list of String:Integer pairs"
        );
        assert_eq!(
            desc("HashMap<String, String>"),
            "Comma-separated list of String:String pairs"
        );
        assert_eq!(desc("Option<String>"), "String");
        assert_eq!(desc("Option<CustomURL>"), "CustomURL");
        assert_eq!(desc("Duration"), "Duration");
        assert_eq!(desc("Time"), "Time");
        assert_eq!(desc("HonorDecodeInStruct"), "HonorDecodeInStruct");
    }

    #[test]
    fn render_tightens_punctuation() {
        assert_eq!(
            render(&syn::parse_str::<Type>("Vec<String>").unwrap()),
            "Vec<String>"
        );
        assert_eq!(render(&syn::parse_str::<Type>("bool").unwrap()), "bool");
    }
}
