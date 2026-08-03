//! Scanning real verse with the real phonemiser.
//!
//! The unit tests in `prosody-core::meter` drive segmentation from a table, so they test the
//! algorithm without a dictionary. This tests the thing end to end — parse, pronounce, scan —
//! on lines whose meter a reader would agree about.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::meter::{FootKind, LineMeter};
use prosody_core::{Fallback, Poem, meter};
use prosody_lts::LetterToSound;

fn scan(source: &str) -> Vec<LineMeter> {
    let poem = Poem::parse(source).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    meter::scan(&poem, &phonemiser)
        .stanzas()
        .iter()
        .flat_map(|s| s.lines().to_vec())
        .collect()
}

#[test]
fn iambic_pentameter_scans_as_five_feet() {
    // Shakespeare, Sonnet 18. Ten syllables, five beats — SPEC.md §4.2's own example of why
    // nothing may round to 4/4.
    let lines = scan("shall i compare thee to a summers day\n");
    let line = &lines[0];
    assert_eq!(line.syllable_count().get(), 10, "{:?}", line.beats());
    assert_eq!(line.foot_count().get(), 5);
}

#[test]
fn a_tetrameter_line_scans_as_four_feet() {
    // Blake, "The Tyger" — trochaic tetrameter, seven syllables.
    let lines = scan("tyger tyger burning bright\n");
    let line = &lines[0];
    assert_eq!(line.syllable_count().get(), 7);
    assert_eq!(line.foot_count().get(), 4);
    assert_eq!(line.dominant_foot(), Some(FootKind::Trochee));
}

#[test]
fn an_anapestic_line_is_not_recoverable_from_lexical_stress() {
    // Byron's "The Assyrian came down like the wolf on the fold" is anapestic tetrameter to a
    // reader. `CMUdict` records *lexical* stress, and marks every monosyllable stressed — so
    // "came down like ... wolf ... fold" arrives as a run of stresses that no anapestic reading
    // survives. `SPEC.md` §9 names this exactly: "CMUdict gives lexical stress, not phrasal
    // stress. Accept the error rate initially."
    //
    // The test records the limitation rather than asserting a scansion the input cannot support.
    let lines = scan("the assyrian came down like the wolf on the fold\n");
    let line = &lines[0];
    assert!(line.syllable_count().get() >= 11);
    assert_ne!(
        line.dominant_foot(),
        Some(FootKind::Anapest),
        "if this ever passes, phrasal stress arrived and this test should become an assertion"
    );
    // What must hold regardless: the line is fully scanned.
    let covered: usize = line.feet().iter().map(|f| f.beats().len()).sum();
    assert_eq!(covered, line.syllable_count().get());
}

#[test]
fn an_anapestic_line_of_polysyllables_is_recognised() {
    // Where the words carry their own stress rather than relying on the phrase, anapests do come
    // through: each of these is w-w-S on its own.
    let lines = scan("understand intervene recommend comprehend\n");
    let line = &lines[0];
    assert!(line.syllable_count().get() >= 11);
    assert_eq!(line.dominant_foot(), Some(FootKind::Anapest));
}

#[test]
fn free_verse_scans_without_pretending_to_be_regular() {
    // Uneven lines must produce uneven counts — that is the signal SPEC.md §4.1 assigns roles by.
    let lines = scan("rain\nit came down all night and did not stop for anything\nthen nothing\n");
    let counts: Vec<usize> = lines.iter().map(|l| l.syllable_count().get()).collect();
    assert_eq!(counts.len(), 3);
    assert!(
        counts.iter().max() > counts.iter().min(),
        "free verse should not scan as uniform: {counts:?}"
    );
}

#[test]
fn a_regular_stanza_is_more_periodic_than_a_ragged_one() {
    // The comparison SPEC.md §4.1 turns into foundation-versus-flourish.
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let regular = Poem::parse("the fire that burns\nthe fire that burnt\n").unwrap();
    let ragged = Poem::parse("rain\nit came down all night and did not stop\n").unwrap();

    let regular = meter::scan(&regular, &phonemiser);
    let ragged = meter::scan(&ragged, &phonemiser);
    assert!(regular.stanzas()[0].syllable_variance() < ragged.stanzas()[0].syllable_variance());
}

#[test]
fn an_invented_word_still_contributes_syllables_but_says_it_guessed() {
    let lines = scan("the krapplesnaffle burns\n");
    let line = &lines[0];
    assert!(line.unpronounced().is_empty(), "the guesser covers it");
    assert!(line.rests_on_a_guess(), "and the line says so");
    assert!(line.syllable_count().get() >= 5);
}

#[test]
fn a_word_no_phonemiser_can_read_is_recorded() {
    let source = "the 漢字 burns\n";
    let lines = scan(source);
    let line = &lines[0];
    assert_eq!(line.unpronounced().len(), 1);
    assert_eq!(line.unpronounced()[0].text(source), Some("漢字"));
    // The syllable count is an undercount, and the line carries the evidence.
    assert_eq!(line.syllable_count().get(), 2);
}

#[test]
fn scanning_real_verse_is_deterministic() {
    let source = "shall i compare thee to a summers day\ntyger tyger burning bright\n";
    assert_eq!(scan(source), scan(source));
}
