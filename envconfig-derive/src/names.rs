//! Field-name handling.
//!
//! Go derives environment variable names from Go field names, which are
//! `PascalCase`. Rust field names are `snake_case`, so the declared name is
//! reconstructed before the Go word-splitting rules are applied. The
//! reconstruction is exact for the splitting algorithm: `multi_word_acr_with_auto_split`
//! becomes `MultiWordAcrWithAutoSplit`, which splits to the same
//! `MULTI_WORD_ACR_WITH_AUTO_SPLIT` that Go's `MultiWordACRWithAutoSplit`
//! produces.

/// Reconstructs the `PascalCase` declared name from a `snake_case` identifier.
pub fn pascal_case(snake: &str) -> String {
    // Raw identifiers such as `r#struct` name the field `struct`.
    let snake = snake.strip_prefix("r#").unwrap_or(snake);
    let mut out = String::with_capacity(snake.len());
    for part in snake.split('_').filter(|p| !p.is_empty()) {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// True when `c` is matched by the regexp character class `[A-Z]`.
fn is_upper(c: u8) -> bool {
    c.is_ascii_uppercase()
}

/// Splits a declared name into words, reproducing Go's
/// `gatherRegexp` = `([^A-Z]+|[A-Z]+[^A-Z]+|[A-Z]+)` followed by
/// `acronymRegexp` = `([A-Z]+)([A-Z][^A-Z]+)`, joined with `_`.
///
/// The gather pass is a direct transcription of the alternation: at each
/// position, a run of non-`[A-Z]` bytes; otherwise a run of `[A-Z]` bytes
/// followed by a run of non-`[A-Z]` bytes if any follow, else the `[A-Z]` run
/// alone.
pub fn split_words(name: &str) -> String {
    let b = name.as_bytes();
    let mut words: Vec<&str> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let start = i;
        if !is_upper(b[i]) {
            while i < b.len() && !is_upper(b[i]) {
                i += 1;
            }
        } else {
            while i < b.len() && is_upper(b[i]) {
                i += 1;
            }
            // `[A-Z]+[^A-Z]+` when a non-uppercase run follows, else `[A-Z]+`.
            while i < b.len() && !is_upper(b[i]) {
                i += 1;
            }
        }
        words.push(&name[start..i]);
    }

    if words.is_empty() {
        return name.to_owned();
    }

    // Acronym pass. A gathered word is either all non-uppercase, all
    // uppercase, or an uppercase run followed by a non-uppercase run; the
    // regexp only matches the third shape when the uppercase run is at least
    // two bytes long, and then splits it as (run minus last byte, last byte +
    // remainder).
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for word in words {
        let wb = word.as_bytes();
        let upper = wb.iter().take_while(|c| is_upper(**c)).count();
        if upper >= 2 && upper < wb.len() {
            out.push(word[..upper - 1].to_owned());
            out.push(word[upper - 1..].to_owned());
        } else {
            out.push(word.to_owned());
        }
    }
    out.join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pascal_case_rebuilds_declared_names() {
        assert_eq!(pascal_case("multi_word_var"), "MultiWordVar");
        assert_eq!(pascal_case("ttl"), "Ttl");
        assert_eq!(pascal_case("debug"), "Debug");
        assert_eq!(pascal_case("url_pointer"), "UrlPointer");
        assert_eq!(pascal_case("after_nested"), "AfterNested");
    }

    /// Expected values captured from the Go implementation.
    #[test]
    fn split_words_matches_go() {
        assert_eq!(split_words("MultiWordVar"), "Multi_Word_Var");
        assert_eq!(
            split_words("MultiWordVarWithAutoSplit"),
            "Multi_Word_Var_With_Auto_Split"
        );
        assert_eq!(
            split_words("MultiWordACRWithAutoSplit"),
            "Multi_Word_ACR_With_Auto_Split"
        );
        assert_eq!(
            split_words("MultiWordAcrWithAutoSplit"),
            "Multi_Word_Acr_With_Auto_Split"
        );
        assert_eq!(split_words("HTTPPort"), "HTTP_Port");
        assert_eq!(split_words("URLValue"), "URL_Value");
        assert_eq!(split_words("TTL"), "TTL");
        assert_eq!(split_words("ID"), "ID");
        assert_eq!(split_words("Debug"), "Debug");
        assert_eq!(split_words("AfterNested"), "After_Nested");
    }

    #[test]
    fn split_words_handles_digits_and_empty() {
        assert_eq!(split_words(""), "");
        assert_eq!(split_words("Port8080"), "Port8080");
        assert_eq!(split_words("lowercase"), "lowercase");
    }
}
