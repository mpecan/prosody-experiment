//! The recorded readings, served from disk (`SPEC.md` §4.7, "Engines": `recorded`).
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
pub(crate) const DEFAULT_DIR: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/readings");

/// The answer to `path` if it asks for a recording: the file, or 404 if there is no such file.
/// `None` for any other path, which the page's [`prosody_web::Site`] answers.
pub(crate) fn answer(dir: &Path, path: &str) -> Option<Reply> {
    let wanted = path.strip_prefix("/readings/")?;
    let Some((file, content_type)) = file_of(dir, wanted) else {
        return Some(Reply::not_found());
    };
    Some(std::fs::read(&file).map_or_else(
        |_| Reply::not_found(),
        |bytes| Reply::ok(content_type, bytes),
    ))
}

/// The file a request names, and its type — only a manifest or an MP3, and only by a path of
/// plain names under `dir`: no `..`, no hidden files, nothing outside it.
fn file_of(dir: &Path, wanted: &str) -> Option<(PathBuf, &'static str)> {
    // Exactly these, lower case, as the tool writes them.
    let content_type = match Path::new(wanted).extension().and_then(|e| e.to_str()) {
        Some("json") => "application/json; charset=utf-8",
        Some("mp3") => "audio/mpeg",
        _ => return None,
    };
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

    use super::file_of;

    #[test]
    fn only_a_manifest_or_a_recording_under_the_directory_is_served() {
        let dir = Path::new("/r");
        assert_eq!(
            file_of(dir, "manifest.json"),
            Some((dir.join("manifest.json"), "application/json; charset=utf-8"))
        );
        assert_eq!(
            file_of(dir, "kokoro-af_heart/0123abcd.mp3"),
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
            assert_eq!(file_of(dir, refused), None, "{refused}");
        }
    }

    #[test]
    fn any_other_path_is_the_sites() {
        assert!(super::answer(Path::new("/r"), "/pattern").is_none());
        assert!(super::answer(Path::new("/r"), "/readings.json").is_none());
        let missing = super::answer(Path::new("/nowhere"), "/readings/manifest.json").unwrap();
        assert_eq!(missing.status, 404);
    }
}
