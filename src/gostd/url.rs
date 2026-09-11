//! Go's `net/url`, limited to the parsing behaviour `envconfig` exposes.
//!
//! Reproduced rather than mapped onto a URL crate because two error surfaces
//! are observable through `ParseError`, and the test suite asserts both: the
//! operation name `parse` and the message
//! `first path segment in URL cannot contain colon`.

use std::error::Error;
use std::fmt;

/// Go's `url.Error`: an operation name, the offending URL, and the cause.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlError {
    /// Always `parse` here.
    pub op: String,
    /// The URL that could not be parsed.
    pub url: String,
    /// The underlying reason.
    pub err: String,
}

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:?}: {}", self.op, self.url, self.err)
    }
}

impl Error for UrlError {}

/// The credentials portion of an authority.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Userinfo {
    /// The decoded username.
    pub username: String,
    /// The decoded password, when one was present.
    pub password: Option<String>,
}

impl fmt::Display for Userinfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", escape(&self.username, Encoding::UserPassword))?;
        if let Some(p) = &self.password {
            write!(f, ":{}", escape(p, Encoding::UserPassword))?;
        }
        Ok(())
    }
}

/// A parsed URL, mirroring the fields of Go's `url.URL`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Url {
    /// Scheme, lower-cased.
    pub scheme: String,
    /// Set for rootless (opaque) URLs such as `mailto:a@b`.
    pub opaque: String,
    /// Credentials, when present.
    pub user: Option<Userinfo>,
    /// Host, with optional `:port`.
    pub host: String,
    /// Decoded path.
    pub path: String,
    /// Original encoded path, when it differs from the decoded form.
    pub raw_path: String,
    /// True when the URL had a scheme and an absolute path but no authority.
    pub omit_host: bool,
    /// True when the URL ended in a bare `?`.
    pub force_query: bool,
    /// Query string, without the leading `?`.
    pub raw_query: String,
    /// Decoded fragment.
    pub fragment: String,
    /// Original encoded fragment, when it differs from the decoded form.
    pub raw_fragment: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Path,
    Host,
    UserPassword,
    Fragment,
}

fn is_hex(c: u8) -> bool {
    c.is_ascii_hexdigit()
}

fn unhex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

fn should_escape(c: u8, mode: Encoding) -> bool {
    if c.is_ascii_alphanumeric() {
        return false;
    }
    if mode == Encoding::Host {
        if let b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=' | b':'
        | b'[' | b']' | b'<' | b'>' | b'"' = c
        {
            return false;
        }
    }
    match c {
        b'-' | b'_' | b'.' | b'~' => return false,
        b'$' | b'&' | b'+' | b',' | b'/' | b':' | b';' | b'=' | b'?' | b'@' => {
            return match mode {
                Encoding::Path => c == b'?',
                Encoding::UserPassword => matches!(c, b'@' | b'/' | b'?' | b':'),
                Encoding::Fragment => false,
                Encoding::Host => true,
            };
        }
        _ => {}
    }
    if mode == Encoding::Fragment {
        if let b'!' | b'(' | b')' | b'*' = c {
            return false;
        }
    }
    true
}

/// Go's `url.unescape`. The host mode carries the extra RFC 3986 rule that
/// percent-encoding in a host is only valid for non-ASCII bytes, which is what
/// makes `http://%41:8080/` fail.
fn unescape(s: &str, mode: Encoding) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                if i + 2 >= b.len() || !is_hex(b[i + 1]) || !is_hex(b[i + 2]) {
                    let end = (i + 3).min(b.len());
                    return Err(format!("invalid URL escape {:?}", &s[i..end]));
                }
                if mode == Encoding::Host && unhex(b[i + 1]) < 8 && &s[i..i + 3] != "%25" {
                    return Err(format!("invalid URL escape {:?}", &s[i..i + 3]));
                }
                out.push(unhex(b[i + 1]) << 4 | unhex(b[i + 2]));
                i += 3;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| "invalid UTF-8 in URL".to_owned())
}

fn escape(s: &str, mode: Encoding) -> String {
    let mut out = String::with_capacity(s.len());
    for &c in s.as_bytes() {
        if should_escape(c, mode) {
            out.push('%');
            out.push(
                char::from_digit(u32::from(c >> 4), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
            out.push(
                char::from_digit(u32::from(c & 0xf), 16)
                    .unwrap_or('0')
                    .to_ascii_uppercase(),
            );
        } else {
            out.push(c as char);
        }
    }
    out
}

/// Splits `s` at the first `sep`, optionally dropping the separator.
fn split(s: &str, sep: char, cut: bool) -> (&str, &str) {
    match s.find(sep) {
        Some(i) if cut => (&s[..i], &s[i + sep.len_utf8()..]),
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

/// Go's `getScheme`.
fn get_scheme(raw: &str) -> Result<(&str, &str), String> {
    for (i, c) in raw.bytes().enumerate() {
        match c {
            b'a'..=b'z' | b'A'..=b'Z' => {}
            b'0'..=b'9' | b'+' | b'-' | b'.' => {
                if i == 0 {
                    return Ok(("", raw));
                }
            }
            b':' => {
                if i == 0 {
                    return Err("missing protocol scheme".to_owned());
                }
                return Ok((&raw[..i], &raw[i + 1..]));
            }
            // An invalid character means there is no valid scheme at all.
            _ => return Ok(("", raw)),
        }
    }
    Ok(("", raw))
}

fn valid_optional_port(port: &str) -> bool {
    if port.is_empty() {
        return true;
    }
    if !port.starts_with(':') {
        return false;
    }
    port[1..].bytes().all(|c| c.is_ascii_digit())
}

fn parse_host(host: &str) -> Result<String, String> {
    if let Some(rest) = host.strip_prefix('[') {
        // IPv6 literal, optionally followed by `:port`.
        let end = rest
            .find(']')
            .ok_or_else(|| format!("missing ']' in host {host:?}"))?;
        let colon_port = &rest[end + 1..];
        if !valid_optional_port(colon_port) {
            return Err(format!("invalid port {colon_port:?} after host"));
        }
        return Ok(host.to_owned());
    }
    if let Some(i) = host.rfind(':') {
        let colon_port = &host[i..];
        if !valid_optional_port(colon_port) {
            return Err(format!("invalid port {colon_port:?} after host"));
        }
    }
    unescape(host, Encoding::Host)
}

fn parse_authority(authority: &str) -> Result<(Option<Userinfo>, String), String> {
    let Some(at) = authority.rfind('@') else {
        return Ok((None, parse_host(authority)?));
    };
    let (userinfo, hostpart) = (&authority[..at], &authority[at + 1..]);
    let host = parse_host(hostpart)?;
    let user = match userinfo.find(':') {
        Some(i) => Userinfo {
            username: unescape(&userinfo[..i], Encoding::UserPassword)?,
            password: Some(unescape(&userinfo[i + 1..], Encoding::UserPassword)?),
        },
        None => Userinfo {
            username: unescape(userinfo, Encoding::UserPassword)?,
            password: None,
        },
    };
    Ok((Some(user), host))
}

fn parse_inner(raw: &str) -> Result<Url, String> {
    if raw.bytes().any(|c| c < 0x20 || c == 0x7f) {
        return Err("net/url: invalid control character in URL".to_owned());
    }
    let mut u = Url::default();
    if raw == "*" {
        u.path = "*".to_owned();
        return Ok(u);
    }

    let (scheme, mut rest) = get_scheme(raw)?;
    u.scheme = scheme.to_ascii_lowercase();

    if rest.ends_with('?') && !rest[..rest.len() - 1].contains('?') {
        u.force_query = true;
        rest = &rest[..rest.len() - 1];
    } else {
        let (r, q) = split(rest, '?', true);
        rest = r;
        u.raw_query = q.to_owned();
    }

    if !rest.starts_with('/') {
        if !u.scheme.is_empty() {
            // A rootless path is opaque, per RFC 3986.
            u.opaque = rest.to_owned();
            return Ok(u);
        }
        // Guards against malformed schemes such as `cache_object:foo/bar`.
        let (segment, _) = split(rest, '/', false);
        if segment.contains(':') {
            return Err("first path segment in URL cannot contain colon".to_owned());
        }
    }

    if (!u.scheme.is_empty() || !rest.starts_with("///")) && rest.starts_with("//") {
        let (authority, r) = split(&rest[2..], '/', false);
        rest = r;
        let (user, host) = parse_authority(authority)?;
        u.user = user;
        u.host = host;
    } else if !u.scheme.is_empty() && rest.starts_with('/') {
        u.omit_host = true;
    }

    set_path(&mut u, rest)?;
    Ok(u)
}

fn set_path(u: &mut Url, p: &str) -> Result<(), String> {
    let decoded = unescape(p, Encoding::Path)?;
    u.path = decoded;
    u.raw_path = if escape(&u.path, Encoding::Path) == p {
        String::new()
    } else {
        p.to_owned()
    };
    Ok(())
}

/// Go's `url.Parse`.
pub fn parse(raw: &str) -> Result<Url, UrlError> {
    let (without_frag, frag) = split(raw, '#', true);
    let mut u = parse_inner(without_frag).map_err(|e| UrlError {
        op: "parse".to_owned(),
        url: without_frag.to_owned(),
        err: e,
    })?;
    if frag.is_empty() {
        return Ok(u);
    }
    let decoded = unescape(frag, Encoding::Fragment).map_err(|e| UrlError {
        op: "parse".to_owned(),
        url: raw.to_owned(),
        err: e,
    })?;
    u.fragment = decoded;
    u.raw_fragment = if escape(&u.fragment, Encoding::Fragment) == frag {
        String::new()
    } else {
        frag.to_owned()
    };
    Ok(u)
}

impl Url {
    /// The encoded path, preferring the original spelling when it round-trips.
    pub fn escaped_path(&self) -> String {
        if !self.raw_path.is_empty() {
            if let Ok(decoded) = unescape(&self.raw_path, Encoding::Path) {
                if decoded == self.path {
                    return self.raw_path.clone();
                }
            }
        }
        escape(&self.path, Encoding::Path)
    }

    /// The encoded fragment, preferring the original spelling.
    pub fn escaped_fragment(&self) -> String {
        if !self.raw_fragment.is_empty() {
            if let Ok(decoded) = unescape(&self.raw_fragment, Encoding::Fragment) {
                if decoded == self.fragment {
                    return self.raw_fragment.clone();
                }
            }
        }
        escape(&self.fragment, Encoding::Fragment)
    }
}

impl fmt::Display for Url {
    /// Go's `URL.String`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::new();
        if !self.scheme.is_empty() {
            buf.push_str(&self.scheme);
            buf.push(':');
        }
        if !self.opaque.is_empty() {
            buf.push_str(&self.opaque);
        } else {
            if !self.scheme.is_empty() || !self.host.is_empty() || self.user.is_some() {
                let omit = self.omit_host && self.host.is_empty() && self.user.is_none();
                if !omit {
                    if !self.host.is_empty() || !self.path.is_empty() || self.user.is_some() {
                        buf.push_str("//");
                    }
                    if let Some(u) = &self.user {
                        buf.push_str(&u.to_string());
                        buf.push('@');
                    }
                    if !self.host.is_empty() {
                        buf.push_str(&escape(&self.host, Encoding::Host));
                    }
                }
            }
            let path = self.escaped_path();
            if !path.is_empty() && !path.starts_with('/') && !self.host.is_empty() {
                buf.push('/');
            }
            if buf.is_empty() {
                let (segment, _) = split(&path, '/', false);
                if segment.contains(':') {
                    buf.push_str("./");
                }
            }
            buf.push_str(&path);
        }
        if self.force_query || !self.raw_query.is_empty() {
            buf.push('?');
            buf.push_str(&self.raw_query);
        }
        if !self.fragment.is_empty() {
            buf.push('#');
            buf.push_str(&self.escaped_fragment());
        }
        f.write_str(&buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_url_used_by_the_test_suite() {
        let u = parse("https://github.com/kelseyhightower/envconfig").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "github.com");
        assert_eq!(u.path, "/kelseyhightower/envconfig");
        assert_eq!(u.raw_query, "");
        assert_eq!(
            u.to_string(),
            "https://github.com/kelseyhightower/envconfig"
        );
    }

    /// The exact message the Go suite compares against.
    #[test]
    fn first_path_segment_colon_matches_go() {
        let e = parse("http_://foo").unwrap_err();
        assert_eq!(e.op, "parse");
        assert_eq!(e.err, "first path segment in URL cannot contain colon");
        assert_eq!(
            e.to_string(),
            "parse \"http_://foo\": first path segment in URL cannot contain colon"
        );
    }

    /// Percent-encoding in a host is only valid for non-ASCII bytes.
    #[test]
    fn invalid_host_escape_matches_go() {
        let e = parse("http://%41:8080/").unwrap_err();
        assert_eq!(e.op, "parse");
        assert_eq!(e.err, "invalid URL escape \"%41\"");
        assert_eq!(
            e.to_string(),
            "parse \"http://%41:8080/\": invalid URL escape \"%41\""
        );
    }

    #[test]
    fn round_trips_common_forms() {
        for s in [
            "https://github.com/kelseyhightower/envconfig",
            "http://example.com",
            "http://example.com/a/b?c=d",
            "http://example.com/a#frag",
            "mailto:someone@example.com",
            "http://user:pass@example.com/x",
            "https://example.com:8443/p",
        ] {
            assert_eq!(parse(s).unwrap().to_string(), s, "{s}");
        }
    }

    #[test]
    fn equal_urls_compare_equal() {
        let a = parse("https://github.com/kelseyhightower/envconfig").unwrap();
        let b = parse("https://github.com/kelseyhightower/envconfig").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn rejects_bad_ports_and_control_characters() {
        assert!(parse("http://example.com:notaport/").is_err());
        assert!(parse("http://exa\u{7f}mple.com/").is_err());
    }

    #[test]
    fn percent_escapes_decode_in_paths() {
        let u = parse("http://example.com/a%20b%2Fc").unwrap();
        assert_eq!(u.path, "/a b/c");
        // The raw spelling is preserved because it does not round-trip.
        assert_eq!(u.raw_path, "/a%20b%2Fc");
        assert_eq!(u.escaped_path(), "/a%20b%2Fc");
        assert_eq!(u.to_string(), "http://example.com/a%20b%2Fc");
    }

    #[test]
    fn lower_case_hex_escapes_decode() {
        let u = parse("http://example.com/%2fa%ce%bb").unwrap();
        assert_eq!(u.path, "//a\u{3bb}");
    }

    #[test]
    fn truncated_escapes_are_rejected() {
        let e = parse("http://example.com/%A").unwrap_err();
        assert_eq!(e.err, "invalid URL escape \"%A\"");
        let e = parse("http://example.com/%ZZ").unwrap_err();
        assert_eq!(e.err, "invalid URL escape \"%ZZ\"");
    }

    #[test]
    fn escaped_host_bytes_above_seven_are_allowed() {
        // `%C3%A9` decodes to a non-ASCII byte, which RFC 3986 permits in a host.
        let u = parse("http://%C3%A9.example.com/").unwrap();
        assert_eq!(u.host, "\u{e9}.example.com");
    }

    #[test]
    fn fragments_are_decoded_and_re_encoded() {
        let u = parse("http://example.com/p#a%20b").unwrap();
        assert_eq!(u.fragment, "a b");
        // Go leaves RawFragment empty when the escaped form round-trips.
        assert_eq!(u.raw_fragment, "");
        assert_eq!(u.escaped_fragment(), "a%20b");
        assert_eq!(u.to_string(), "http://example.com/p#a%20b");

        // `!`, `(`, `)` and `*` are not escaped in a fragment.
        let u = parse("http://example.com/p#a!(b)*c").unwrap();
        assert_eq!(u.fragment, "a!(b)*c");
        assert_eq!(u.to_string(), "http://example.com/p#a!(b)*c");
    }

    #[test]
    fn userinfo_with_and_without_password() {
        let u = parse("http://bob@example.com/x").unwrap();
        let user = u.user.as_ref().unwrap();
        assert_eq!(user.username, "bob");
        assert_eq!(user.password, None);
        assert_eq!(u.to_string(), "http://bob@example.com/x");

        let u = parse("http://bob:s%3Acret@example.com/x").unwrap();
        let user = u.user.as_ref().unwrap();
        assert_eq!(user.username, "bob");
        assert_eq!(user.password.as_deref(), Some("s:cret"));
        assert_eq!(user.to_string(), "bob:s%3Acret");
    }

    #[test]
    fn ipv6_hosts_keep_their_brackets() {
        let u = parse("http://[::1]/x").unwrap();
        assert_eq!(u.host, "[::1]");
        let u = parse("http://[::1]:8080/x").unwrap();
        assert_eq!(u.host, "[::1]:8080");
        assert_eq!(
            parse("http://[::1:8080/x").unwrap_err().err,
            "missing ']' in host \"[::1:8080\""
        );
        assert_eq!(
            parse("http://[::1]:bad/x").unwrap_err().err,
            "invalid port \":bad\" after host"
        );
    }

    #[test]
    fn asterisk_is_a_path() {
        let u = parse("*").unwrap();
        assert_eq!(u.path, "*");
    }

    #[test]
    fn a_bare_trailing_question_mark_forces_the_query() {
        let u = parse("http://example.com/x?").unwrap();
        assert!(u.force_query);
        assert_eq!(u.raw_query, "");
        assert_eq!(u.to_string(), "http://example.com/x?");
    }

    #[test]
    fn a_scheme_with_no_authority_omits_the_host() {
        let u = parse("file:/etc/hosts").unwrap();
        assert!(u.omit_host);
        assert_eq!(u.scheme, "file");
        assert_eq!(u.path, "/etc/hosts");
        assert_eq!(u.to_string(), "file:/etc/hosts");
    }

    #[test]
    fn schemes_are_lower_cased_and_validated() {
        assert_eq!(parse("HTTP://example.com/").unwrap().scheme, "http");
        // A leading digit means there is no scheme at all.
        let u = parse("1http/x").unwrap();
        assert_eq!(u.scheme, "");
        assert_eq!(u.path, "1http/x");
        assert_eq!(parse(":foo").unwrap_err().err, "missing protocol scheme");
    }

    #[test]
    fn rootless_paths_are_opaque() {
        let u = parse("mailto:someone@example.com").unwrap();
        assert_eq!(u.scheme, "mailto");
        assert_eq!(u.opaque, "someone@example.com");
        assert_eq!(u.host, "");
    }

    #[test]
    fn a_relative_first_segment_with_a_colon_is_prefixed_on_output() {
        // No scheme, so the colon-bearing segment must not be mistaken for one.
        let u = Url {
            path: "a:b/c".to_owned(),
            ..Default::default()
        };
        assert_eq!(u.to_string(), "./a:b/c");
    }

    #[test]
    fn queries_survive_a_round_trip() {
        let u = parse("http://example.com/x?a=1&b=2").unwrap();
        assert_eq!(u.raw_query, "a=1&b=2");
        assert_eq!(u.to_string(), "http://example.com/x?a=1&b=2");
    }
}
