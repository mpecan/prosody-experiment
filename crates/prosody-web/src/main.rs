//! `prosody-web` — the page, served on localhost.
//!
//! Everything the page asks of the compiler is answered by the library ([`prosody_web::Site`]);
//! this binary adds the IO: a socket on localhost, the built page read from disk (`--page DIR`,
//! `web/dist` of this checkout by default, which `npm run build` in `web/` writes), the recorded
//! readings (`--readings DIR`, `examples/readings` by default), or, with `--site DIR`, the page's
//! data written out for the static build (`scripts/build-site.sh`).
//!
//! ```text
//! prosody-web [PORT] [--page DIR] [--readings DIR]
//! prosody-web --site DIR
//! ```

mod files;
mod server;

use anyhow::{Context as _, Result};
use prosody_web::{Site, Transport};

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if first.as_deref() == Some("--site") {
        let dir = args.next().context("--site wants a directory")?;
        return write_site(std::path::Path::new(&dir));
    }
    let mut port = 4747;
    let mut recordings = std::path::PathBuf::from(files::READINGS_DIR);
    let mut page = std::path::PathBuf::from(files::PAGE_DIR);
    let mut next = first;
    while let Some(arg) = next {
        if arg == "--readings" {
            recordings = args.next().context("--readings wants a directory")?.into();
        } else if arg == "--page" {
            page = args.next().context("--page wants a directory")?.into();
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
    // The compiler still answers without it — `npm run dev` in `web/` serves the page itself and
    // sends its questions here — so this is said, not refused.
    if !page.join("index.html").is_file() {
        println!(
            "prosody: no built page in {} — run `npm run build` in web/, or `npm run dev` there",
            page.display()
        );
    }

    let site = Site::new(Transport::Server)?;
    server::serve(port, move |request| {
        if let Some(reply) = files::reading(&recordings, &request.path) {
            return reply;
        }
        let answer = site.respond(request);
        if answer.status == 404 && request.method == "GET" {
            return files::page(&page, &request.path);
        }
        answer.into()
    })
    .context("serving")?;
    Ok(())
}

/// Writes the page's data, `site.json`, into `dir`, for the static build: the page beside it comes
/// from `web/dist` and the compiler (`prosody.js` and `prosody_bg.wasm`) from `wasm-bindgen`, both
/// copied there by `scripts/build-site.sh`.
fn write_site(dir: &std::path::Path) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let site = Site::new(Transport::Wasm)?;
    let path = dir.join("site.json");
    std::fs::write(&path, site.data()).with_context(|| format!("writing {}", path.display()))?;
    println!("prosody: wrote the page's data to {}", dir.display());
    Ok(())
}
