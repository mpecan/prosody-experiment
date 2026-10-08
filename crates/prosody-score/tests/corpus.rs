//! The scorer's baseline over every poem the project ships, with the whole report committed.
//!
//! Two sets: the golden corpus (`CLAUDE.md` §7.2), chosen for the ways the pipeline goes wrong —
//! a one-line poem, a nonsense word, a non-English text — and the runnable examples, which are
//! what a listener actually auditions and what a tuning loop will be judged on.
//!
//! The snapshot is the point. A change to any mapping moves these numbers, and the diff says which
//! way: whether the change made the piece more layered, more melodic, less local. **Review every
//! diff by reading it**, and say in the commit which numbers moved and why.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // test code; CLAUDE.md §3

use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::Score;
use prosody_core::{
    Fallback, Lexicon, Phonemiser, Poem, meter, repetition, rhyme, role, sound, surprisal,
};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;
use prosody_score::{DEFAULT_PROBES, Report, sensitivity};

const POEMS: &[(&str, &str)] = &[
    (
        "corpus-pentameter",
        include_str!("../../prosody-emit/tests/corpus/pentameter.poem"),
    ),
    (
        "corpus-free-verse",
        include_str!("../../prosody-emit/tests/corpus/free-verse.poem"),
    ),
    (
        "corpus-villanelle",
        include_str!("../../prosody-emit/tests/corpus/villanelle.poem"),
    ),
    (
        "corpus-refrain",
        include_str!("../../prosody-emit/tests/corpus/refrain.poem"),
    ),
    (
        "corpus-one-line",
        include_str!("../../prosody-emit/tests/corpus/one-line.poem"),
    ),
    (
        "corpus-nonsense",
        include_str!("../../prosody-emit/tests/corpus/nonsense.poem"),
    ),
    (
        "corpus-slovenian",
        include_str!("../../prosody-emit/tests/corpus/slovenian.poem"),
    ),
    (
        "example-blake-tyger",
        include_str!("../../../examples/blake-tyger.poem"),
    ),
    (
        "example-carroll-jabberwocky",
        include_str!("../../../examples/carroll-jabberwocky.poem"),
    ),
    (
        "example-dickinson-because",
        include_str!("../../../examples/dickinson-because.poem"),
    ),
    (
        "example-hopkins-pied-beauty",
        include_str!("../../../examples/hopkins-pied-beauty.poem"),
    ),
    (
        "example-poe-annabel-lee",
        include_str!("../../../examples/poe-annabel-lee.poem"),
    ),
    (
        "example-preseren-zdravljica",
        include_str!("../../../examples/preseren-zdravljica.poem"),
    ),
    (
        "example-shakespeare-sonnet-18",
        include_str!("../../../examples/shakespeare-sonnet-18.poem"),
    ),
    (
        "example-whitman-song-of-myself",
        include_str!("../../../examples/whitman-song-of-myself.poem"),
    ),
    (
        "example-wilde-theocritus",
        include_str!("../../../examples/wilde-theocritus.poem"),
    ),
];

/// Compiles a poem the way the CLI does.
fn compile(source: &str, phonemiser: &impl Phonemiser, lexicon: &impl Lexicon) -> Option<Score> {
    let poem = Poem::parse(source.to_owned()).ok()?;
    let meter = meter::scan(&poem, phonemiser);
    let rhyme = rhyme::analyse(&poem, phonemiser);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, phonemiser);
    let surprisal = surprisal::analyse(&poem, phonemiser, lexicon);
    let roles = role::assign(&meter, &repetition).ok()?;
    lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme,
        repetition: &repetition,
        sound: &sound,
        surprisal: &surprisal,
        roles: &roles,
    })
    .ok()
}

#[test]
fn every_shipped_poem_measures_as_committed() {
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let lexicon = EnglishLexicon::new();
    for (name, source) in POEMS {
        let score = compile(source, &phonemiser, &lexicon);
        assert!(score.is_some(), "{name} does not compile");
        let score = score.unwrap();
        let edits = sensitivity::measure(source, DEFAULT_PROBES, |edited| {
            compile(edited, &phonemiser, &lexicon)
        });
        insta::assert_snapshot!(*name, Report::new(&score, edits).to_text());
    }
}
