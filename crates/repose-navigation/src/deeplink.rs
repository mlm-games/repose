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
            .map(|s| url_decode(s))
            .collect();

        let mut query = HashMap::new();
        if !query_str.is_empty() {
            for pair in query_str.split('&') {
                if let Some(eq) = pair.find('=') {
                    let key = url_decode(&pair[..eq]);
                    let val = url_decode(&pair[eq + 1..]);
                    query.insert(key, val);
                } else if !pair.is_empty() {
                    query.insert(url_decode(pair), String::new());
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

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hi = chars.next().and_then(|c| c.to_digit(16));
            let lo = chars.next().and_then(|c| c.to_digit(16));
            match (hi, lo) {
                (Some(h), Some(l)) => result.push(char::from((h << 4 | l) as u8)),
                _ => result.push('%'),
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}
