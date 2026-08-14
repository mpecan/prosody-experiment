//! A very small HTTP server, for one browser tab on localhost.
//!
//! # Why this is hand-written
//!
//! `CLAUDE.md` §10 asks what a dependency replaces and why hand-rolling is worse. Here it is not
//! worse: `axum` would bring `tokio`, `hyper`, `tower` and a dozen more crates to serve three
//! routes to a single local client, and the workspace's whole build is currently seven small
//! crates plus `blake3` and `toml`.
//!
//! The risk of a hand-written HTTP server is real but bounded here, and bounded in a way the
//! [`toml`](prosody_emit::style) case was not. A partial TOML parser silently mis-reads documents
//! it does not understand; a partial HTTP parser that meets a request it does not understand
//! returns 400 and the page says so. This speaks HTTP/1.1 with `Content-Length`, which is all a
//! `fetch` from our own page ever sends.
//!
//! # It binds to localhost only
//!
//! Deliberately, and not configurable. This compiles arbitrary text from its request body; that is
//! exactly what it is for on your own machine and exactly what it should never be on a network.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};

/// One parsed request.
pub(crate) struct Request {
    /// `GET` or `POST`.
    pub(crate) method: String,
    /// The path, without the query string.
    pub(crate) path: String,
    /// The decoded query parameters, in the order they appeared.
    pub(crate) query: Vec<(String, String)>,
    /// The request body.
    pub(crate) body: String,
}

impl Request {
    /// One query parameter, if it was given.
    pub(crate) fn param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What to send back.
pub(crate) struct Response {
    /// The HTTP status code.
    pub(crate) status: u16,
    /// The `Content-Type`.
    pub(crate) content_type: &'static str,
    /// The body.
    pub(crate) body: String,
}

impl Response {
    /// A successful plain-text response.
    #[must_use]
    pub(crate) fn text(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }

    /// A successful HTML response.
    #[must_use]
    pub(crate) fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
        }
    }

    /// A refusal, with the reason as the body so the page can show it to the writer.
    ///
    /// A poem that will not compile is the normal case while someone is typing, so the reason
    /// matters more than the status — `CLAUDE.md` §6's rule that a fallback must be visible,
    /// applied to a text box.
    #[must_use]
    pub(crate) fn problem(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body,
        }
    }
}

/// Serves until interrupted, calling `handle` for every request.
///
/// # Errors
///
/// If the port cannot be bound.
pub(crate) fn serve(
    port: u16,
    handle: impl Fn(&Request) -> Response + Send + Sync + 'static,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let bound = listener.local_addr()?;
    println!("prosody: http://{bound} — ^C to stop");

    let handle = std::sync::Arc::new(handle);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // A thread per connection, which is not an optimisation — it is required. Browsers open
        // connections speculatively and leave them idle, and the page issues its two fetches at
        // once; a single accept loop blocks on the first idle socket and the page never loads.
        // The read timeout is the other half: a connection that never sends a request must not
        // hold a thread for ever.
        let handle = std::sync::Arc::clone(&handle);
        std::thread::spawn(move || {
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));
            match read_request(&mut stream) {
                Ok(Some(request)) => {
                    let response = handle(&request);
                    let _ = write_response(&mut stream, &response);
                }
                // A speculative connection that closed without asking for anything.
                Ok(None) => {}
                Err(_) => {
                    let _ = write_response(
                        &mut stream,
                        &Response::problem(400, "could not read the request".to_owned()),
                    );
                }
            }
        });
    }
    Ok(())
}

/// The largest body accepted: far more poem than anyone will type, and small enough that a stray
/// request cannot ask this process to allocate without limit.
const LIMIT: usize = 1 << 20;

/// Reads one request. `Ok(None)` for a connection that closed without sending one.
fn read_request(stream: &mut TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);

    let mut start = String::new();
    if reader.read_line(&mut start)? == 0 {
        return Ok(None);
    }
    let mut parts = start.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "malformed request line",
        ));
    };
    let (method, target) = (method.to_owned(), target.to_owned());

    let mut length = 0_usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0_u8; length.min(LIMIT)];
    reader.read_exact(&mut body)?;

    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path.to_owned(), parse_query(query)),
        None => (target, Vec::new()),
    };

    Ok(Some(Request {
        method,
        path,
        query,
        body: String::from_utf8_lossy(&body).into_owned(),
    }))
}

/// Splits and percent-decodes a query string.
fn parse_query(query: &str) -> Vec<(String, String)> {
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

fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
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
    fn an_empty_query_yields_nothing() {
        assert!(parse_query("").is_empty());
        assert_eq!(
            parse_query("bare").len(),
            1,
            "a key with no value is still a key"
        );
    }
}
