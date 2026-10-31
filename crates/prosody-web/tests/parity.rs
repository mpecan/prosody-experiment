//! The parity fixtures: the Strudel text and the Performance of the same piece, side by side, for
//! the page's parity test (`web/test/parity.test.ts`) to play the text with Strudel itself and
//! compare what it plays with the Performance (`docs/player.md` §3, §6).
//!
//! The comparison needs Strudel, so it runs in the page's test suite; the pieces need the
//! compiler, so they are made here, through the very requests the page sends (`/pattern`,
//! `/performance`), and committed under `web/test/parity/`. This test fails when a committed
//! fixture is not what the compiler now makes, so neither side can change without the parity
//! test seeing it. After a deliberate change, rewrite them and read the diff:
//!
//! ```sh
//! PROSODY_BLESS=1 cargo test -p prosody-web --test parity
//! ```

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use std::fmt::Write as _;
use std::path::PathBuf;

use prosody_web::{Request, Site, Transport};

/// The corpus poems (`crates/prosody-emit/tests/corpus`), by name.
const POEMS: &[(&str, &str)] = &[
    (
        "pentameter",
        include_str!("../../prosody-emit/tests/corpus/pentameter.poem"),
    ),
    (
        "free-verse",
        include_str!("../../prosody-emit/tests/corpus/free-verse.poem"),
    ),
    (
        "villanelle",
        include_str!("../../prosody-emit/tests/corpus/villanelle.poem"),
    ),
    (
        "refrain",
        include_str!("../../prosody-emit/tests/corpus/refrain.poem"),
    ),
    (
        "one-line",
        include_str!("../../prosody-emit/tests/corpus/one-line.poem"),
    ),
    (
        "nonsense",
        include_str!("../../prosody-emit/tests/corpus/nonsense.poem"),
    ),
    (
        "slovenian",
        include_str!("../../prosody-emit/tests/corpus/slovenian.poem"),
    ),
];

/// Every case: a fixture name, a corpus poem, and the page's query — the palette and the
/// mappings. Every poem under the defaults, and then settings chosen to reach every parameter and
/// every way a note is placed: swing under both pulses (the elastic one reaching Strudel's
/// `swingBy` departures, which the parity test names), humanise and density, chords and their
/// turnarounds, walking basses, the straight stack, anchors, articulated drums, slant rhymes,
/// assonance, oscillators, no compression and no echo.
const CASES: &[(&str, &str, &str)] = &[
    ("pentameter", "pentameter", ""),
    ("free-verse", "free-verse", ""),
    ("villanelle", "villanelle", ""),
    ("refrain", "refrain", ""),
    ("one-line", "one-line", ""),
    ("nonsense", "nonsense", ""),
    ("slovenian", "slovenian", ""),
    (
        "refrain-jazz",
        "refrain",
        "swing=0.33&voicing=seventh&kit=ride&bank=virtuosity&melody=gm_epiano1\
         &bass=gm_acoustic_bass&humanise=0.5&density_level=0.5&drive=0.3",
    ),
    (
        "refrain-harmony",
        "refrain",
        "set=harmony:both,bass:feet,reading:stanza&voicing=seventh&chord_sound=gm_vibraphone",
    ),
    (
        "pentameter-elastic-swing",
        "pentameter",
        "set=pulse:elastic&swing=0.33",
    ),
    // Swing far past any palette's: where Strudel's `swingBy` doubles and drops notes, so the
    // parity test's account of those departures is checked against Strudel itself.
    ("free-verse-hard-swing", "free-verse", "swing=0.9"),
    (
        "free-verse-candidates",
        "free-verse",
        "set=anchor:on,drums:articulated,melody:assonance,rhyme:slant&delay=0&compress=0\
         &melody=sine&bass=square&ride_fallback=crash",
    ),
    (
        "one-line-straight",
        "one-line",
        "set=stack:straight,bass:syllables,function-words:light,harmony:cadence,pulse:elastic\
         &swing=0.5&scale=lydian&root=F%23&humanise=1",
    ),
];

/// Where the fixtures live: beside the page's tests.
fn directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/test/parity")
}

/// A JSON string literal.
fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other if (other as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", other as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// One fixture: the case, its pattern, and its Performance.
fn fixture(site: &Site, name: &str, poem: &str, query: &str) -> String {
    let ask = |view: &str| {
        let target = if query.is_empty() {
            view.to_owned()
        } else {
            format!("{view}?{query}")
        };
        let response = site.respond(&Request::new("POST", &target, poem.to_owned()));
        assert_eq!(response.status, 200, "{name} {target}: {}", response.body);
        response.body
    };
    format!(
        "{{\"name\":{},\"query\":{},\"pattern\":{},\"performance\":{}}}\n",
        quoted(name),
        quoted(query),
        quoted(&ask("/pattern")),
        ask("/performance").trim_end()
    )
}

#[test]
fn the_parity_fixtures_are_what_the_compiler_makes() {
    let site = Site::new(Transport::Server).unwrap();
    let bless = std::env::var_os("PROSODY_BLESS").is_some();
    let mut stale = Vec::new();
    for (name, poem, query) in CASES {
        let text = POEMS.iter().find(|(p, _)| p == poem).unwrap().1;
        let made = fixture(&site, name, text, query);
        let path = directory().join(format!("{name}.json"));
        if bless {
            std::fs::create_dir_all(directory()).unwrap();
            std::fs::write(&path, &made).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(made.as_str()) {
            stale.push(path.display().to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "parity fixtures out of date (rewrite with PROSODY_BLESS=1 and read the diff): {stale:?}"
    );
    // No fixture lingers for a case that is gone: the page's test would still compare it.
    let names: Vec<String> = std::fs::read_dir(directory())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|file| {
            std::path::Path::new(file)
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    for file in names {
        assert!(
            CASES
                .iter()
                .any(|(name, _, _)| format!("{name}.json") == file),
            "{file} is no case's fixture"
        );
    }
}
