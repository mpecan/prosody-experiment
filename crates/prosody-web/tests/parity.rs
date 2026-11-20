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

/// A poem the corpus does not have: a stanza longer than `arrange::LONG_STANZA` (eight lines) whose
/// loop the next stanza arrives over, so the arrival erodes it (`SPEC.md` §4.6, erosion) — the
/// default for long stanzas, and a way a note is placed that no corpus poem reaches (their stanzas
/// are at most four lines). Every line two iambs, so both stanzas are one layer and one loop.
const LONG_STANZA: &str = "\
the river runs
the willow bends
the morning comes
the meadow wakes
the swallow flies
the shadow falls
the water turns
the evening ends
the lantern burns

the summer goes
the winter stays
";

/// A poem the corpus does not have: one whose piece ends on a beat of three syllables
/// ("shim-mer-ing"), so its last note begins two thirds into its last beat, where a swing past two
/// thirds carries it beyond the piece's end. Strudel's pattern is cyclic, so it plays that note at
/// the start of the next time round, the first included (`docs/player.md` §9); the parity test
/// plays two rounds to see it on both sides of the seam.
const SWUNG_ROUND: &str = "\
the morning comes
the meadow wakes, and the river is shimmering
";

/// Every case: a fixture name, a poem, and the page's query — the palette and the mappings. Every
/// corpus poem under the defaults, and then settings chosen to reach the parameters and the ways a
/// note is placed that the defaults do not: swing under both pulses (the elastic one reaching
/// notes Strudel's `swingBy` would double or drop, which the text's own swing must not), humanise and density, chords and
/// their turnarounds, chords at a density level below one, walking basses, the straight stack,
/// anchors, articulated drums, slant rhymes, assonance, oscillators, no compression and no echo, a
/// long stanza eroding under an arrival and the same stanza handed over line by line with no outro,
/// and a note swung past the piece's end, round to the start of the next time round.
///
/// Not every combination: a placement or parameter the cases do not reach is compared only through
/// the Rust properties (`prosody-emit/tests/perform.rs`), not against Strudel.
/// `the_cases_reach_what_they_claim` checks that the cases named for a placement do reach it.
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
    // Swing far past any palette's: where Strudel's `swingBy` would double and drop notes, so the
    // text's own swing is checked against Strudel exactly where `swingBy` went wrong.
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
    // Chords in bars that hold several lines, at a density level: the chord lane's gain below 1.
    (
        "villanelle-harmony-density",
        "villanelle",
        "set=harmony:both&density_level=0.7",
    ),
    // A long stanza eroding under the next (the default), and handed over line by line instead,
    // with the outro switched off.
    ("long-stanza", "long-stanza", ""),
    (
        "long-stanza-gradual",
        "long-stanza",
        "set=long-stanzas:gradual&off=outro",
    ),
    // The last note swung past the piece's end, to the start of the next time round, nudged too.
    ("swung-round", "swung-round", "swing=0.9&humanise=1"),
];

/// The poem a case names: a corpus poem, or [`LONG_STANZA`].
fn poem_named(name: &str) -> &'static str {
    match name {
        "long-stanza" => return LONG_STANZA,
        "swung-round" => return SWUNG_ROUND,
        _ => {}
    }
    POEMS.iter().find(|(poem, _)| *poem == name).unwrap().1
}

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
        let made = fixture(&site, name, poem_named(poem), query);
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

/// The text and the Performance of a case, made now.
fn made(site: &Site, case: &str) -> (String, String) {
    let (_, poem, query) = CASES.iter().find(|(name, ..)| *name == case).unwrap();
    let ask = |view: &str| {
        let target = if query.is_empty() {
            view.to_owned()
        } else {
            format!("{view}?{query}")
        };
        let response = site.respond(&Request::new("POST", &target, poem_named(poem).to_owned()));
        assert_eq!(response.status, 200, "{case} {target}: {}", response.body);
        response.body
    };
    (ask("/pattern"), ask("/performance"))
}

#[test]
fn the_cases_reach_what_they_claim() {
    // A case added for a placement or a parameter is only a guard while it still reaches it: a
    // change to the arrangement or to a palette default could quietly move it off, and the parity
    // test would keep passing over notes that no longer exercise anything.
    let site = Site::new(Transport::Server).unwrap();

    // Chords whose gain is a density level below one. Each note's JSON starts at its `from`.
    let (_, performance) = made(&site, "villanelle-harmony-density");
    let quiet_chords = performance
        .split("{\"from\":")
        .filter(|note| note.contains("\"channel\":\"chords\""))
        .filter(|note| !note.contains("\"gain\":1.000000,"))
        .count();
    assert!(quiet_chords > 0, "no chord below gain 1: {performance}");

    // The long stanza erodes under the default, and not under `gradual`.
    let (eroding, _) = made(&site, "long-stanza");
    assert!(
        eroding.contains("long-stanzas = erode: 6 bar(s) over an eroding loop"),
        "{eroding}"
    );
    let (gradual, _) = made(&site, "long-stanza-gradual");
    assert!(!gradual.contains("over an eroding loop"), "{gradual}");

    // A note swing carries past the piece's end sounds at the start of the next round: its onset
    // is earlier than where its place in the pattern (`cycle`) begins, which no other note's is.
    let (_, performance) = made(&site, "swung-round");
    let number = |json: &str, key: &str| -> f64 {
        json.split(&format!("\"{key}\":"))
            .nth(1)
            .and_then(|rest| rest.split([',', '}']).next())
            .unwrap()
            .parse()
            .unwrap()
    };
    let per_cycle = 60.0 / number(&performance, "cpm");
    let seconds = number(&performance, "seconds");
    let round = performance
        .split("{\"from\":")
        .filter(|note| note.contains("\"channel\":"))
        .filter(|note| number(note, "onset") + 1e-6 < number(note, "cycle") * per_cycle)
        .collect::<Vec<_>>();
    assert!(!round.is_empty(), "no note comes round: {performance}");
    for note in round {
        // At the start of the round: within swing's delay (0.45 of a beat) and the nudge.
        assert!(number(note, "onset") < 0.45 * per_cycle + 0.03, "{note}");
        assert!(
            number(note, "cycle") * per_cycle > seconds - per_cycle,
            "{note}"
        );
    }
}
