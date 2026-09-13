use std::collections::HashMap;

/// Parsed URI from a deeplink payload.
///
/// Extracts scheme, host, path segments, and query parameters from a URI string.
/// Use this in your `set_on_deeplink` callback to determine which navigation
/// destination to push.
///
/// # Example
/// ```
/// use repose_navigation::deeplink::DeeplinkUri;
///
/// let uri = DeeplinkUri::parse("https://example.com/user/42?id=foo");
/// assert_eq!(uri.scheme, "https");
/// assert_eq!(uri.host.as_deref(), Some("example.com"));
/// assert_eq!(uri.path, vec!["user", "42"]);
/// assert_eq!(uri.param("id"), Some("foo"));
/// ```
pub struct DeeplinkUri {
    pub scheme: String,
    pub host: Option<String>,
    pub path: Vec<String>,
    pub query: HashMap<String, String>,
}

impl DeeplinkUri {
    pub fn parse(uri: &str) -> Self {
        let uri = uri.trim();

        let has_scheme = uri.contains("://");
        let (scheme, rest) = if let Some(pos) = uri.find("://") {
            (uri[..pos].to_string(), &uri[pos + 3..])
        } else {
            (String::new(), uri)
        };

        let (host, path_and_query) = if has_scheme {
            if let Some(pos) = rest.find('/') {
                let h = &rest[..pos];
                if h.is_empty() {
                    (None, &rest[pos..])
                } else {
                    (Some(h.to_string()), &rest[pos..])
                }
            } else if let Some(qpos) = rest.find('?') {
                let h = &rest[..qpos];
                let host = if h.is_empty() {
                    None
                } else {
                    Some(h.to_string())
                };
                (host, &rest[qpos..])
            } else {
                (Some(rest.to_string()), "")
            }
        } else {
            (None, rest)
        };

        let (path_str, query_str) = if let Some(pos) = path_and_query.find('?') {
            (&path_and_query[..pos], &path_and_query[pos + 1..])
        } else {
            (path_and_query, "")
        };

        let path: Vec<String> = path_str
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|seg| url_decode(seg, false))
            .collect();

        let mut query = HashMap::new();
        if !query_str.is_empty() {
            for pair in query_str.split('&') {
                if let Some(eq) = pair.find('=') {
                    let key = url_decode(&pair[..eq], true);
                    let val = url_decode(&pair[eq + 1..], true);
                    query.insert(key, val);
                } else if !pair.is_empty() {
                    query.insert(url_decode(pair, true), String::new());
                }
            }
        }

        DeeplinkUri {
            scheme,
            host,
            path,
            query,
        }
    }

    /// Convenience: get a query parameter value by key.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(|s| s.as_str())
    }
}

/// Percent-decode `s`. `%XX` bytes are collected and decoded as UTF-8
/// (replacement char on invalid sequences) instead of one Latin-1 char per
/// byte. `+` maps to space only in query strings (`plus_as_space`), where it
/// is legal; in paths `+` stays literal per RFC 3986.
fn url_decode(s: &str, plus_as_space: bool) -> String {
    let mut buf: Vec<u8> = Vec::with_capacity(s.len());
    let bytes = s.as_bytes();
    let hex_at = |i: usize| -> Option<u8> {
        bytes
            .get(i)
            .copied()
            .and_then(|b| (b as char).to_digit(16).map(|d| d as u8))
    };
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'%' {
            match (hex_at(i + 1), hex_at(i + 2)) {
                (Some(h), Some(l)) => {
                    buf.push(h << 4 | l);
                    i += 3;
                }
                _ => {
                    buf.push(b'%');
                    i += 1;
                }
            }
        } else if c == b'+' && plus_as_space {
            buf.push(b' ');
            i += 1;
        } else {
            buf.push(c);
            i += 1;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_query_without_path() {
        let u = DeeplinkUri::parse("myapp://host?x=1");
        assert_eq!(u.host.as_deref(), Some("host"));
        assert!(u.path.is_empty());
        assert_eq!(u.param("x"), Some("1"));
    }

    #[test]
    fn percent_utf8_and_path_plus() {
        let u = DeeplinkUri::parse("myapp://h/p%E2%82%AC?q=%E2%82%AC");
        assert_eq!(u.path, vec!["p\u{20ac}"]);
        assert_eq!(u.param("q"), Some("\u{20ac}"));
        let u = DeeplinkUri::parse("myapp://h/a+b?q=a+b");
        assert_eq!(u.path, vec!["a+b"]);
        assert_eq!(u.param("q"), Some("a b"));
    }

    #[test]
    fn truncated_percent_stays_literal() {
        assert_eq!(url_decode("a%2", true), "a%2");
        assert_eq!(url_decode("a%", true), "a%");
        assert_eq!(url_decode("a%zz", true), "a%zz");
    }
}
