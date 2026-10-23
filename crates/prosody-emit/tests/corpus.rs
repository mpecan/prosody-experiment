//! The golden corpus: `CLAUDE.md` §7.2.
//!
//! Seven poems, each chosen for a way the pipeline can go wrong, compiled end to end through the
//! real dictionary and the real lexicon, with the whole output committed. See `corpus/SOURCES.md`
//! for what each one is for and where it came from.
//!
//! # What a snapshot is for here
//!
//! Not "the output is what it is" — that would be a test that can only ever be re-blessed. It is
//! for the class of regression the unit tests keep missing: a whole *column* going empty. Review
//! found that the palette block, the header counts, the layer labels and the beat position could
//! each be blanked or made constant with the entire suite still green. Each needed its own test
//! afterwards; one committed snapshot of the whole text would have caught all four at once,
//! because everything is in it.
//!
//! **`CLAUDE.md` §7.2: review every diff by reading it.** Accepting an `insta` diff without
//! reading it converts the corpus from a test into a record of whatever the code last did.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::annotate;
use prosody_core::lower::{self, Analyses};
use prosody_core::{Fallback, Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::{Backend as _, Strudel, Style};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

/// Every poem in the corpus, by name.
const CORPUS: &[(&str, &str)] = &[
    ("pentameter", include_str!("corpus/pentameter.poem")),
    ("free-verse", include_str!("corpus/free-verse.poem")),
    ("villanelle", include_str!("corpus/villanelle.poem")),
    ("refrain", include_str!("corpus/refrain.poem")),
    ("one-line", include_str!("corpus/one-line.poem")),
    ("nonsense", include_str!("corpus/nonsense.poem")),
    ("slovenian", include_str!("corpus/slovenian.poem")),
];

/// Compiles a poem the way the CLI does.
fn compile(source: &str) -> (String, String) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let lexicon = EnglishLexicon::new();

    let meter = meter::scan(&poem, &phonemiser);
    let rhyme = rhyme::analyse(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, &phonemiser);
    let surprisal = surprisal::analyse(&poem, &phonemiser, &lexicon);
    let roles = role::assign(&meter, &repetition).unwrap();
    let score = lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme,
        repetition: &repetition,
        sound: &sound,
        surprisal: &surprisal,
        roles: &roles,
    })
    .unwrap();

    let pattern = Strudel::new()
        .render(&score, &style)
        .unwrap_or_else(|error| {
            // A poem that makes no sound is a legitimate thing to write and a legitimate thing to
            // snapshot; refusing to render it is the behaviour under test.
            format!("<no pattern: {error}>\n")
        });
    let view = annotate::render(&poem, &annotate::run(&poem, &score, &roles));
    (pattern, view)
}

#[test]
fn the_corpus_compiles_to_committed_patterns() {
    for (name, source) in CORPUS {
        let (pattern, _) = compile(source);
        insta::assert_snapshot!(format!("pattern-{name}"), pattern);
    }
}

#[test]
fn the_corpus_annotates_to_committed_views() {
    for (name, source) in CORPUS {
        let (_, view) = compile(source);
        insta::assert_snapshot!(format!("view-{name}"), view);
    }
}

#[test]
fn every_poem_in_the_corpus_is_covered() {
    // The corpus is only as good as its coverage of `CLAUDE.md` §7.2's list, and a file added to
    // the directory without being added to `CORPUS` would be silently untested.
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut found: Vec<String> = std::fs::read_dir(&directory)
        .expect("the corpus directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|e| e == "poem")
        })
        .collect();
    found.sort();
    let mut listed: Vec<String> = CORPUS
        .iter()
        .map(|(name, _)| format!("{name}.poem"))
        .collect();
    listed.sort();
    assert_eq!(found, listed, "every .poem file must be in CORPUS");
}

#[test]
fn each_poem_exercises_the_case_it_was_chosen_for() {
    // A corpus entry that has stopped being the thing it is named for is worse than no entry: the
    // snapshot still passes, and the case it was meant to cover is silently gone.
    let facts = |source: &str| {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
        let meter = meter::scan(&poem, &phonemiser);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let rhyme = rhyme::analyse(&poem, &phonemiser);
        (poem, roles, rhyme, meter)
    };

    // Strict pentameter: every stanza regular on its own evidence.
    let (_, roles, ..) = facts(CORPUS[0].1);
    assert!(
        roles
            .stanzas()
            .iter()
            .all(|s| s.basis() == prosody_core::role::StanzaBasis::Regularity),
        "pentameter.poem must be regular: {:?}",
        roles
            .stanzas()
            .iter()
            .map(prosody_core::StanzaRole::basis)
            .collect::<Vec<_>>()
    );

    // Free verse: nothing regular, so §9's fallback is what gives the piece a pulse.
    let (_, roles, rhymes, _) = facts(CORPUS[1].1);
    assert!(
        roles
            .stanzas()
            .iter()
            .any(|s| s.basis() == prosody_core::role::StanzaBasis::MostRegular),
        "free-verse.poem must trigger the §9 fallback"
    );
    assert!(
        rhymes
            .stanzas()
            .iter()
            .flat_map(prosody_core::rhyme::StanzaRhyme::lines)
            .all(|line| line.score().is_none()),
        "free-verse.poem must not rhyme"
    );

    // Villanelle and refrain: lines that repeat verbatim, which is §4.3's refrain row.
    for index in [2, 3] {
        let (poem, ..) = facts(CORPUS[index].1);
        let repetition = repetition::analyse(&poem);
        let voices: Vec<_> = repetition
            .stanzas()
            .iter()
            .flat_map(prosody_core::repetition::StanzaRepetition::lines)
            .map(prosody_core::repetition::LineRepetition::voice)
            .collect();
        let distinct: std::collections::BTreeSet<_> = voices.iter().collect();
        assert!(
            distinct.len() < voices.len(),
            "{} must contain a refrain",
            CORPUS[index].0
        );
    }

    // One line: the minimum viable input §9 asks about.
    let (poem, roles, ..) = facts(CORPUS[4].1);
    assert_eq!(poem.stanzas().len(), 1);
    assert_eq!(poem.stanzas()[0].lines().len(), 1);
    assert_eq!(
        roles.stanzas()[0].basis(),
        prosody_core::role::StanzaBasis::OnlyCandidate,
        "a one-line poem is anchored for want of an alternative"
    );

    // Nonsense and Slovenian: words no dictionary has, so the fallback carries them — and says so.
    for index in [5, 6] {
        let (poem, ..) = facts(CORPUS[index].1);
        let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
        let meter = meter::scan(&poem, &phonemiser);
        assert!(
            meter
                .stanzas()
                .iter()
                .flat_map(prosody_core::meter::StanzaMeter::lines)
                .any(prosody_core::meter::LineMeter::rests_on_a_guess),
            "{} must exercise the letter-to-sound fallback",
            CORPUS[index].0
        );
    }
}

#[test]
fn every_corpus_pattern_is_valid_javascript() {
    // The emitted text is JavaScript. A bracket count let two syntax errors through once, and an
    // earlier emitter produced a program that could not run at all.
    let Ok(node) = std::process::Command::new("node").arg("--version").output() else {
        eprintln!("node not found; skipping");
        return;
    };
    if !node.status.success() {
        return;
    }
    for (name, source) in CORPUS {
        let (pattern, _) = compile(source);
        if pattern.starts_with("<no pattern") {
            continue;
        }
        let file = std::env::temp_dir().join(format!("prosody-corpus-{name}.js"));
        std::fs::write(&file, &pattern).unwrap();
        let checked = std::process::Command::new("node")
            .arg("--check")
            .arg(&file)
            .output()
            .unwrap();
        let _ = std::fs::remove_file(&file);
        assert!(
            checked.status.success(),
            "node rejected {name}:\n{}",
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}

/// The villanelle under the jazz palette — its two rhyme classes, §4.5's "two-note obsession", as
/// two seventh chords over the ride. Beside `pattern-villanelle` this is the palette's whole effect
/// on one poem: the same steps, chords where there were notes, other sounds, swing.
#[test]
fn the_jazz_palette_compiles_to_a_committed_pattern() {
    let source = CORPUS
        .iter()
        .find(|(name, _)| *name == "villanelle")
        .map(|(_, source)| *source)
        .unwrap();
    assert!(
        !source.starts_with("+++"),
        "the fixture has no header of its own"
    );
    let (pattern, _) = compile(&format!("+++\npreset = \"jazz\"\n+++\n{source}"));
    assert!(pattern.contains(".swingBy(0.330,"), "{pattern}");
    insta::assert_snapshot!("pattern-villanelle-jazz", pattern);
}

#[test]
fn compiling_the_corpus_is_reproducible() {
    // Invariant I1, over every shape of poem the corpus covers.
    for (name, source) in CORPUS {
        assert_eq!(compile(source), compile(source), "{name}");
    }
}
