//! Files served from disk: the built page (`web/dist`), and the recorded readings (`SPEC.md` §4.7,
//! "Engines": `recorded`).
//!
//! The page is a separate program, built by Vite into `web/dist`: its HTML, its scripts and styles,
//! the player's render worker and worklet, the speech worker, and what `web/public` puts beside them
//! — the drums and soundfont zones (FLAC) and, once `scripts/build-synth.sh` has written it, the
//! player's synth (`synth.js`, `synth_bg.wasm`). The server answers it from there, beside the
//! compiler's answers, so served and static (`scripts/build-site.sh` copies the same files) are one
//! page, and the served one plays (`docs/player.md` §5).
//!
//! `scripts/readings/` renders every line of every example poem with a set of voices into
//! `examples/readings/` — a manifest and one small audio file per line and voice. The page asks
//! for them at `/readings/…`, beside itself, so the static site (`scripts/build-site.sh` copies the
//! directory in) and this server answer the same paths. They are files, not answers of the
//! compiler, so they are read here, in the binary, where all the IO is: the library stays free of
//! the file system, and the WebAssembly build never carries ten megabytes of speech.

use std::path::{Path, PathBuf};

use crate::server::Reply;

/// Where the recordings are when no `--readings DIR` is given: the checkout this binary was built
/// from. A binary moved elsewhere finds none there, answers 404 for each, and the page says so and
/// reads with a live engine.
pub(crate) const READINGS_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/readings");

/// Where the built page is when no `--page DIR` is given: `web/dist` of this checkout, which
/// `npm run build` in `web/` writes.
pub(crate) const PAGE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/dist");

/// The answer to `path` if it asks for a recording: the file, or 404 if there is no such file.
/// `None` for any other path, which the page's [`prosody_web::Site`] answers.
pub(crate) fn reading(dir: &Path, path: &str) -> Option<Reply> {
    let wanted = path.strip_prefix("/readings/")?;
    Some(read(file_of(dir, wanted, recording_type)))
}

/// The answer to `path` from the built page: `/` is its `index.html`; anything else a file of it,
/// or 404.
pub(crate) fn page(dir: &Path, path: &str) -> Reply {
    let wanted = path.strip_prefix('/').unwrap_or(path);
    let wanted = if wanted.is_empty() {
        "index.html"
    } else {
        wanted
    };
    read(file_of(dir, wanted, page_type))
}

fn read(file: Option<(PathBuf, &'static str)>) -> Reply {
    let Some((file, content_type)) = file else {
        return Reply::not_found();
    };
    std::fs::read(&file).map_or_else(
        |_| Reply::not_found(),
        |bytes| Reply::ok(content_type, bytes),
    )
}

/// A recording's type: only a manifest or an MP3, exactly these, lower case, as the tool writes
/// them.
fn recording_type(extension: &str) -> Option<&'static str> {
    match extension {
        "json" => Some("application/json; charset=utf-8"),
        "mp3" => Some("audio/mpeg"),
        _ => None,
    }
}

/// A type of file the built page has.
fn page_type(extension: &str) -> Option<&'static str> {
    match extension {
        "html" => Some("text/html; charset=utf-8"),
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "css" => Some("text/css; charset=utf-8"),
        "json" => Some("application/json; charset=utf-8"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "woff2" => Some("font/woff2"),
        "woff" => Some("font/woff"),
        // The player's synth: `WebAssembly.instantiateStreaming` refuses any other type.
        "wasm" => Some("application/wasm"),
        // The drums and the soundfonts' zones, which the player decodes itself.
        "flac" => Some("audio/flac"),
        _ => None,
    }
}

/// The file a request names, and its type — only of a type `type_of` knows, and only by a path of
/// plain names under `dir`: no `..`, no hidden files, nothing outside it.
fn file_of(
    dir: &Path,
    wanted: &str,
    type_of: fn(&str) -> Option<&'static str>,
) -> Option<(PathBuf, &'static str)> {
    let content_type = type_of(Path::new(wanted).extension().and_then(|e| e.to_str())?)?;
    let mut file = dir.to_path_buf();
    for segment in wanted.split('/') {
        let plain = !segment.is_empty()
            && !segment.starts_with('.')
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !plain {
            return None;
        }
        file.push(segment);
    }
    Some((file, content_type))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)] // test code; a panic here is a test failure. See CLAUDE.md §3.
mod tests {
    use std::path::Path;

    use super::{file_of, page_type, recording_type};

    #[test]
    fn only_a_manifest_or_a_recording_under_the_directory_is_served() {
        let dir = Path::new("/r");
        let file_of = |wanted| file_of(dir, wanted, recording_type);
        assert_eq!(
            file_of("manifest.json"),
            Some((dir.join("manifest.json"), "application/json; charset=utf-8"))
        );
        assert_eq!(
            file_of("kokoro-af_heart/0123abcd.mp3"),
            Some((
                dir.join("kokoro-af_heart").join("0123abcd.mp3"),
                "audio/mpeg"
            ))
        );
        for refused in [
            "../Cargo.toml",
            "../x.mp3",
            "a/../../x.mp3",
            ".hidden.mp3",
            "a//b.mp3",
            "/etc/x.mp3",
            "a%2F..%2Fb.mp3",
            "x.wav",
            "x.mp3/",
            "",
        ] {
            assert_eq!(file_of(refused), None, "{refused}");
        }
    }

    #[test]
    fn any_other_path_is_the_sites() {
        assert!(super::reading(Path::new("/r"), "/pattern").is_none());
        assert!(super::reading(Path::new("/r"), "/readings.json").is_none());
        let missing = super::reading(Path::new("/nowhere"), "/readings/manifest.json").unwrap();
        assert_eq!(missing.status, 404);
    }

    #[test]
    fn the_page_is_served_by_its_files_types_and_nothing_outside_it() {
        let dir = Path::new("/p");
        let file_of = |wanted| file_of(dir, wanted, page_type);
        assert_eq!(
            file_of("index.html"),
            Some((dir.join("index.html"), "text/html; charset=utf-8"))
        );
        // A module worker must be JavaScript, or the browser refuses it.
        for module in [
            "assets/index-3f2a.js",
            "assets/render-worker-9c1d.js",
            "speech-worker.mjs",
            "synth.js",
        ] {
            assert_eq!(
                file_of(module).map(|(_, kind)| kind),
                Some("text/javascript; charset=utf-8"),
                "{module}"
            );
        }
        assert_eq!(
            file_of("assets/index-3f2a.css").map(|(_, kind)| kind),
            Some("text/css; charset=utf-8")
        );
        for refused in [
            "../Cargo.toml",
            "../index.html",
            ".env.js",
            "x.exe",
            "a/../b.js",
            "",
        ] {
            assert_eq!(file_of(refused), None, "{refused}");
        }
        // `/` is the page.
        assert_eq!(super::page(Path::new("/nowhere"), "/").status, 404);
    }

    #[test]
    fn the_player_and_its_samples_are_served() {
        let dir = Path::new("/p");
        let kind = |wanted| file_of(dir, wanted, page_type).map(|(_, kind)| kind);
        assert_eq!(kind("synth_bg.wasm"), Some("application/wasm"));
        assert_eq!(kind("drums/tr808/bd/0.flac"), Some("audio/flac"));
        assert_eq!(kind("soundfonts/zones/0a1b.flac"), Some("audio/flac"));
    }

    /// Every file the page ships beside itself (`web/public`, copied into `web/dist` by Vite) is one
    /// the server answers: refused, a drum or a soundfont zone is silence on the served page alone.
    /// Only the notes on where the samples came from are not the page's.
    #[test]
    fn every_file_beside_the_page_is_of_a_type_the_server_answers() {
        fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    files(&path, out);
                } else {
                    out.push(path);
                }
            }
        }
        let public = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public"));
        let mut found = Vec::new();
        files(public, &mut found);
        assert!(found.len() > 300, "{} files in web/public", found.len());
        let refused: Vec<_> = found
            .iter()
            .filter(|path| path.extension().and_then(|e| e.to_str()) != Some("md"))
            .filter(|path| {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                page_type(extension).is_none()
            })
            .collect();
        assert!(refused.is_empty(), "{refused:?}");
    }
}
