//! A request and its response, without the network.
//!
//! The page asks for the same five things whether a local server answers it or the page's own
//! WebAssembly build does (`prosody-wasm`), so the request is a value both can build: the server
//! from a socket, the browser build from what the page hands it.

/// One parsed request.
pub struct Request {
    /// `GET` or `POST`.
    pub method: String,
    /// The path, without the query string.
    pub path: String,
    /// The decoded query parameters, in the order they appeared.
    pub query: Vec<(String, String)>,
    /// The request body.
    pub body: String,
}

impl Request {
    /// One query parameter, if it was given.
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What to send back.
pub struct Response {
    /// The HTTP status code.
    pub status: u16,
    /// The `Content-Type`.
    pub content_type: &'static str,
    /// The body.
    pub body: String,
}

impl Response {
    /// A successful plain-text response.
    #[must_use]
    pub fn text(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }

    /// A successful HTML response.
    #[must_use]
    pub fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
        }
    }

    /// A successful JavaScript module.
    #[must_use]
    pub fn javascript(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/javascript; charset=utf-8",
            body,
        }
    }

    /// A successful JSON document.
    #[must_use]
    pub fn json(body: String) -> Self {
        Self {
            status: 200,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    /// A refusal, with the reason as the body so the page can show it to the writer.
    ///
    /// A poem that will not compile is the normal case while someone is typing, so the reason
    /// matters more than the status — `CLAUDE.md` §6's rule that a fallback must be visible,
    /// applied to a text box.
    #[must_use]
    pub fn problem(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }
}

impl Request {
    /// A request for `target` — a path with an optional `?query` — the way a browser sends it.
    #[must_use]
    pub fn new(method: &str, target: &str, body: String) -> Self {
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path.to_owned(), parse_query(query)),
            None => (target.to_owned(), Vec::new()),
        };
        Self {
            method: method.to_owned(),
            path,
            query,
            body,
        }
    }
}

/// Splits and percent-decodes a query string.
pub(crate) fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decode(key), decode(value))
        })
        .collect()
}

/// Percent-decoding, with `+` for a space.
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes.get(index) {
            Some(b'+') => {
                out.push(b' ');
                index += 1;
            }
            Some(b'%') => {
                let digits = text
                    .get(index + 1..index + 3)
                    .and_then(|pair| u8::from_str_radix(pair, 16).ok());
                if let Some(byte) = digits {
                    out.push(byte);
                    index += 3;
                } else {
                    // A stray `%` is a literal one rather than a reason to fail.
                    out.push(b'%');
                    index += 1;
                }
            }
            Some(byte) => {
                out.push(*byte);
                index += 1;
            }
            None => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)] // test code; a panic here is a test failure. See CLAUDE.md §3.
mod tests {
    use super::{decode, parse_query};

    #[test]
    fn a_query_string_decodes() {
        let query = parse_query("scale=phrygian_dominant&root=F%23&swing=12");
        assert_eq!(
            query[0],
            ("scale".to_owned(), "phrygian_dominant".to_owned())
        );
        assert_eq!(
            query[1],
            ("root".to_owned(), "F#".to_owned()),
            "percent-decoded"
        );
        assert_eq!(query[2], ("swing".to_owned(), "12".to_owned()));
    }

    #[test]
    fn decoding_handles_the_awkward_cases() {
        assert_eq!(decode("a+b"), "a b", "plus is a space");
        assert_eq!(decode("%E2%80%94"), "—", "multi-byte utf-8");
        assert_eq!(
            decode("100%"),
            "100%",
            "a stray percent is literal, not an error"
        );
        assert_eq!(decode("%zz"), "%zz", "and so is an invalid escape");
        assert_eq!(decode(""), "");
    }

    #[test]
    fn a_target_splits_into_path_and_query() {
        let request = super::Request::new("POST", "/pattern?scale=lydian&off=bass", String::new());
        assert_eq!(request.path, "/pattern");
        assert_eq!(request.param("scale"), Some("lydian"));
        assert_eq!(request.param("off"), Some("bass"));
        let bare = super::Request::new("GET", "/", String::new());
        assert_eq!((bare.path.as_str(), bare.query.len()), ("/", 0));
    }

    #[test]
    fn an_empty_query_yields_nothing() {
        assert!(parse_query("").is_empty());
        assert_eq!(
            parse_query("bare").len(),
            1,
            "a key with no value is still a key"
        );
    }
}
