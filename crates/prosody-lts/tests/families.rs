//! Word families on real vocabulary, with the real phonemiser.
//!
//! `SPEC.md` §4.3 says the morphological row is what earns the project, so it is worth checking
//! against real pronunciations rather than a table chosen to make it work.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::sound::{self, SoundFigureKind, SoundRepetition};
use prosody_core::{Fallback, Poem};
use prosody_lts::LetterToSound;

fn analyse(source: &str) -> SoundRepetition {
    let poem = Poem::parse(source).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    sound::analyse(&poem, &phonemiser)
}

/// Which family each word landed in, by its text.
fn families(source: &str) -> Vec<(&str, usize, usize)> {
    analyse(source)
        .forms()
        .iter()
        .filter_map(|form| {
            Some((
                form.span().text(source)?,
                form.voice().get(),
                form.distance().get(),
            ))
        })
        .collect()
}

#[test]
fn burn_burning_burnt_are_one_voice() {
    // The example SPEC.md §4.3 gives by name.
    let source = "burn burning burnt\n";
    let found = families(source);
    assert_eq!(found.len(), 3);
    assert_eq!(found[0].1, found[1].1, "{found:?}");
    assert_eq!(found[0].1, found[2].1, "{found:?}");
    assert_eq!(found[0].2, 0, "burn is the root");
    assert!(found[1].2 > 0, "burning is varied");
    assert!(found[2].2 > 0, "burnt is varied");
}

#[test]
fn a_rhyme_does_not_join_the_family() {
    // "turn" rhymes with "burn". §4.5 maps that to pitch; §4.3's voice maps to timbre. Merging
    // them would play a rhyme as a variation of the same instrument.
    let found = families("burn turn\n");
    assert_eq!(found.len(), 2);
    assert_ne!(found[0].1, found[1].1, "{found:?}");
}

#[test]
fn real_inflections_group() {
    for (root, inflected) in [
        ("walk", "walking"),
        ("walk", "walked"),
        ("sing", "singing"),
        ("love", "loved"),
        ("dream", "dreaming"),
        ("turn", "turning"),
    ] {
        let source = format!("{root} {inflected}\n");
        let found = families(&source);
        assert_eq!(found.len(), 2, "{source:?} -> {found:?}");
        assert_eq!(found[0].1, found[1].1, "{source:?} -> {found:?}");
        assert!(found[1].2 > 0, "{source:?} -> {found:?}");
    }
}

#[test]
fn unrelated_words_stay_apart() {
    for (a, b) in [
        ("burn", "bird"),
        ("fire", "fear"),
        ("dark", "dart"),
        ("light", "like"),
        ("sing", "sink"),
    ] {
        let source = format!("{a} {b}\n");
        let found = families(&source);
        assert_eq!(found.len(), 2, "{source:?}");
        assert_ne!(found[0].1, found[1].1, "{source:?} -> {found:?}");
    }
}

#[test]
fn distance_grows_with_the_size_of_the_change() {
    // "burnt" adds one phoneme to "burn"; "burning" adds two. §4.3 turns that into modulation
    // depth, so the ordering has to be right, not just the grouping.
    let found = families("burn burnt burning\n");
    assert_eq!(found.len(), 3);
    assert!(
        found[1].2 < found[2].2,
        "burnt should be nearer the root than burning: {found:?}"
    );
}

#[test]
fn a_family_survives_stanza_breaks() {
    let source = "the fire will burn\n\nand the fire is burning\n";
    let rep = analyse(source);
    let burn = rep
        .forms()
        .iter()
        .find(|f| f.span().text(source) == Some("burn"))
        .unwrap();
    let burning = rep
        .forms()
        .iter()
        .find(|f| f.span().text(source) == Some("burning"))
        .unwrap();
    assert_eq!(burn.voice(), burning.voice());
    assert!(burning.distance().get() > 0);

    // ...and "fire" repeats exactly, so it is one voice at distance zero.
    let fires: Vec<_> = rep
        .forms()
        .iter()
        .filter(|f| f.span().text(source) == Some("fire"))
        .collect();
    assert_eq!(fires.len(), 2);
    assert_eq!(fires[0].voice(), fires[1].voice());
    assert!(fires[1].distance().is_root());
}

#[test]
fn alliteration_is_found_in_real_verse() {
    // Hopkins, who is mostly alliteration.
    let source = "i caught this morning mornings minion kingdom of daylights dauphin\n";
    let rep = analyse(source);
    let figures: Vec<_> = rep.figures_of(SoundFigureKind::Alliteration).collect();
    assert!(!figures.is_empty(), "expected alliteration");
    let has_m = figures.iter().any(|f| f.phoneme().symbol() == "M");
    let has_d = figures.iter().any(|f| f.phoneme().symbol() == "D");
    assert!(
        has_m && has_d,
        "{:?}",
        figures
            .iter()
            .map(|f| f.phoneme().symbol())
            .collect::<Vec<_>>()
    );
}

#[test]
fn assonance_is_found_in_real_verse() {
    let source = "the rain in spain stays mainly\n";
    let rep = analyse(source);
    let figures: Vec<_> = rep.figures_of(SoundFigureKind::Assonance).collect();
    assert!(
        figures.iter().any(|f| f.phoneme().symbol() == "EY"),
        "{:?}",
        figures
            .iter()
            .map(|f| f.phoneme().symbol())
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_invented_word_still_gets_a_family_and_says_it_guessed() {
    let source = "krapplesnaffle krapplesnaffles\n";
    let rep = analyse(source);
    assert_eq!(rep.forms().len(), 2);
    assert_eq!(rep.forms()[0].voice(), rep.forms()[1].voice());
    assert!(
        rep.forms()
            .iter()
            .all(prosody_core::sound::WordForm::rests_on_a_guess)
    );
}

#[test]
fn analysis_of_real_verse_is_deterministic() {
    let source = "the fire will burn\nthe fire is burning\nthe fire has burnt\n";
    assert_eq!(analyse(source), analyse(source));
}
