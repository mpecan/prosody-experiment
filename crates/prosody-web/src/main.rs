//! `prosody-web` — the page, served on localhost.
//!
//! Everything the page asks for is answered by the library ([`prosody_web::Site`]); this binary
//! adds the IO: a socket on localhost, or, with `--site DIR`, the page written out as static files
//! for the WebAssembly build to sit beside (`scripts/build-site.sh`).

mod server;

use anyhow::{Context as _, Result};
use prosody_web::{SPEECH_WORKER, STRUDEL_GLOBALS, Site, Transport};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--site") {
        let dir = args.next().context("--site wants a directory")?;
        return write_site(std::path::Path::new(&dir));
    }
    let port = first
        .map_or(Ok(4747), |given| given.parse())
        .context("the port must be a number")?;

    let site = Site::new(Transport::Server)?;
    server::serve(port, move |request| site.respond(request)).context("serving")?;
    Ok(())
}

/// Writes the static page and its helper modules into `dir`. The page then expects the compiler
/// beside it (`prosody.js` and `prosody_bg.wasm`, which `scripts/build-site.sh` generates).
fn write_site(dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let site = Site::new(Transport::Wasm)?;
    for (name, text) in [
        ("index.html", site.page()),
        ("strudel-globals.mjs", STRUDEL_GLOBALS),
        ("speech-worker.mjs", SPEECH_WORKER),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }
    println!("prosody: wrote the page to {}", dir.display());
    Ok(())
}
