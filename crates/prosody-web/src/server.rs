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

use prosody_web::{Request, Response};

/// What is sent back: the page's [`Response`], or a file's bytes (a recorded reading).
pub(crate) struct Reply {
    /// The HTTP status code.
    pub(crate) status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Reply {
    /// A file, found.
    pub(crate) const fn ok(content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type,
            body,
        }
    }

    /// No such file.
    pub(crate) fn not_found() -> Self {
        Response::problem(404, "no such thing here\n".to_owned()).into()
    }
}

impl From<Response> for Reply {
    fn from(response: Response) -> Self {
        Self {
            status: response.status,
            content_type: response.content_type,
            body: response.body.into_bytes(),
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
    handle: impl Fn(&Request) -> Reply + Send + Sync + 'static,
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
                        &Response::problem(400, "could not read the request".to_owned()).into(),
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

    Ok(Some(Request::new(
        method,
        target,
        String::from_utf8_lossy(&body).into_owned(),
    )))
}

fn write_response(stream: &mut TcpStream, response: &Reply) -> std::io::Result<()> {
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
    stream.write_all(&response.body)?;
    stream.flush()
}
