//! `prosody-web` — the page, served on localhost.
//!
//! Everything the page asks for is answered by the library ([`prosody_web::Site`]); this binary
//! adds the IO: a socket on localhost, the recorded readings read from disk (`--readings DIR`,
//! `examples/readings` of this checkout by default), or, with `--site DIR`, the page written out as
//! static files for the WebAssembly build to sit beside (`scripts/build-site.sh`).
//!
//! ```text
//! prosody-web [PORT] [--readings DIR]
//! prosody-web --site DIR
//! ```

mod readings;
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
    let mut port = 4747;
    let mut recordings = std::path::PathBuf::from(readings::DEFAULT_DIR);
    let mut next = first;
    while let Some(arg) = next {
        if arg == "--readings" {
            recordings = args.next().context("--readings wants a directory")?.into();
        } else {
            port = arg.parse().context("the port must be a number")?;
        }
        next = args.next();
    }
    if !recordings.join("manifest.json").is_file() {
        println!(
            "prosody: no recorded readings in {} — the page reads with live engines only",
            recordings.display()
        );
    }

    let site = Site::new(Transport::Server)?;
    server::serve(port, move |request| {
        readings::answer(&recordings, &request.path).unwrap_or_else(|| site.respond(request).into())
    })
    .context("serving")?;
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
