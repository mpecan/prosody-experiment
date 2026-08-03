//! Rhyme on real verse, with the real phonemiser.
//!
//! The unit tests in `prosody-core::rhyme` drive scoring from a table. These check the schemes
//! that `SPEC.md` §4.5 names — ABAB, AABB, the villanelle's obsession — come out of poems a
//! reader would agree about.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::rhyme::{self, PoemRhyme, RhymeScore};
use prosody_core::{Fallback, Phoneme, Phonemiser, Poem};
use prosody_lts::LetterToSound;

fn phonemiser() -> Fallback<CmuDict, LetterToSound> {
    Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap()
}

fn analyse(source: &str) -> PoemRhyme {
    let poem = Poem::parse(source).unwrap();
    rhyme::analyse(&poem, &phonemiser())
}

fn schemes(source: &str) -> Vec<String> {
    analyse(source)
        .stanzas()
        .iter()
        .map(prosody_core::rhyme::StanzaRhyme::scheme)
        .collect()
}

fn pair(a: &str, b: &str) -> RhymeScore {
    let p = phonemiser();
    rhyme::score(&p.pronounce(a).unwrap(), &p.pronounce(b).unwrap())
}

#[test]
fn real_perfect_rhymes_score_one() {
    for (a, b) in [
        ("burning", "turning"),
        ("fire", "desire"),
        ("december", "remember"),
        ("day", "may"),
        ("light", "night"),
        ("brigade", "cavalcade"),
    ] {
        assert!(
            pair(a, b).is_perfect(),
            "{a}/{b} scored {}",
            pair(a, b).as_f64()
        );
    }
}

#[test]
fn words_that_do_not_rhyme_score_low() {
    for (a, b) in [
        ("burning", "silver"),
        ("orange", "purple"),
        ("tide", "song"),
    ] {
        assert!(
            pair(a, b) < rhyme::RHYME_THRESHOLD,
            "{a}/{b} scored {}",
            pair(a, b).as_f64()
        );
    }
}

#[test]
fn a_slant_rhyme_lands_between_perfect_and_nothing() {
    // Dickinson's characteristic near-rhyme.
    let slant = pair("soul", "all");
    assert!(!slant.is_perfect(), "should not be a perfect rhyme");
    let none = pair("soul", "tide");
    assert!(slant > none, "but closer than an unrelated pair");
}

#[test]
fn an_abab_quatrain_reads_as_abab() {
    // Gray's Elegy, opening quatrain.
    let source = "the curfew tolls the knell of parting day\n\
                  the lowing herd wind slowly oer the lea\n\
                  the ploughman homeward plods his weary way\n\
                  and leaves the world to darkness and to me\n";
    assert_eq!(schemes(source), ["ABAB"]);
}

#[test]
fn a_couplet_reads_as_aa() {
    // Pope, closing an argument on a rhyme.
    let source = "true wit is nature to advantage dressed\n\
                  what oft was thought but neer so well expressed\n";
    assert_eq!(schemes(source), ["AA"]);
}

#[test]
fn a_shakespearean_sonnet_ending_reads_as_a_couplet() {
    let source = "so long as men can breathe or eyes can see\n\
                  so long lives this and this gives life to thee\n";
    assert_eq!(schemes(source), ["AA"]);
}

#[test]
fn a_villanelles_refrain_returns_to_its_class() {
    // Thomas. The refrain recurs stanzas later and must land in the same class, or SPEC.md §4.5's
    // "two-note obsession" never emerges.
    let source = "do not go gentle into that good night\n\
                  old age should burn and rave at close of day\n\
                  rage rage against the dying of the light\n\
                  \n\
                  though wise men at their end know dark is right\n\
                  because their words had forked no lightning they\n\
                  do not go gentle into that good night\n";
    let rhymes = analyse(source);
    let class = |s: usize, l: usize| rhymes.stanzas()[s].lines()[l].class().unwrap();

    // The refrain, three lines apart across a stanza break.
    assert_eq!(class(0, 0), class(1, 2), "the refrain keeps its class");
    // "night" / "light" / "right" are one class; "day" / "they" another.
    assert_eq!(class(0, 0), class(0, 2));
    assert_eq!(class(0, 1), class(1, 1));
    assert_ne!(class(0, 0), class(0, 1));
    assert_eq!(rhymes.class_count(), 2, "a villanelle really is two notes");
}

#[test]
fn free_verse_gives_mostly_distinct_classes() {
    let source = "rain\n\
                  it came down all night and did not stop for anything\n\
                  not for the dogs\n\
                  then nothing\n";
    let rhymes = analyse(source);
    assert!(
        rhymes.class_count() >= 3,
        "unrhymed lines should not collapse together: {} classes",
        rhymes.class_count()
    );
}

#[test]
fn the_rhyme_word_is_traceable_to_the_source() {
    let source = "the curfew tolls the knell of parting day\n";
    let rhymes = analyse(source);
    let line = &rhymes.stanzas()[0].lines()[0];
    assert_eq!(line.word().and_then(|s| s.text(source)), Some("day"));
    let tail: Vec<&str> = line.tail().iter().copied().map(Phoneme::symbol).collect();
    assert_eq!(tail, ["EY"]);
}

#[test]
fn an_invented_word_rhymes_on_a_guess_and_says_so() {
    let source = "the burning tide\n\
                  the krapplesnaffle\n";
    let rhymes = analyse(source);
    let lines = rhymes.stanzas()[0].lines();
    assert!(!lines[0].rests_on_a_guess());
    assert!(lines[1].rests_on_a_guess(), "the coinage was guessed at");
}

#[test]
fn analysing_real_verse_is_deterministic() {
    let source = "the curfew tolls the knell of parting day\n\
                  the lowing herd wind slowly oer the lea\n";
    assert_eq!(analyse(source), analyse(source));
}
