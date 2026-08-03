//! Distinctiveness on real vocabulary, with the real phonemiser and lexicon.
//!
//! `SPEC.md` §4.4 warns that distinctiveness "needs a real metric or it degenerates into vibes".
//! These check the metric behaves the way a reader would expect on words they know.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::surprisal::{self, PoemSurprisal, Rarity, Surprisal};
use prosody_core::{Fallback, Lexicon, Poem};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

fn scored(source: &str) -> PoemSurprisal {
    let poem = Poem::parse(source).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    surprisal::analyse(&poem, &phonemiser, &EnglishLexicon::new())
}

fn score_of(source: &str, word: &str) -> Surprisal {
    scored(source)
        .words()
        .iter()
        .find(|w| w.span().text(source) == Some(word))
        .map_or_else(
            || panic!("{word:?} was not scored"),
            prosody_core::surprisal::WordSurprisal::surprisal,
        )
}

#[test]
fn a_striking_word_outscores_its_ordinary_neighbours() {
    let source = "the incandescent tide of the day\n";
    let rep = scored(source);
    assert_eq!(
        rep.sharpest().unwrap().span().text(source),
        Some("incandescent")
    );
}

#[test]
fn rarity_decides_between_words_of_equal_novelty() {
    // Without this, a metric that ignored rarity entirely passed every real-vocabulary test here,
    // because the chosen words were also the most phonetically novel in their lines. These two
    // share their sounds with the line equally; only rarity separates them.
    let source = "the gossamer and the summer\n";
    let lexicon = EnglishLexicon::new();
    assert!(
        lexicon.rarity("gossamer") > lexicon.rarity("summer"),
        "the fixture only works if these differ in rarity"
    );
    assert!(score_of(source, "gossamer") > score_of(source, "summer"));
}

#[test]
fn the_sharpest_word_is_the_one_a_reader_would_point_at() {
    // Measured cases from review: an earlier version picked "shorter" over "civility" here, and
    // "maybe" over "explode" below — ordinary words winning on a noisy novelty term.
    let dickinson = "for only gossamer my gown\nmy tippet only tulle\n";
    assert_eq!(
        scored(dickinson).sharpest().unwrap().span().text(dickinson),
        Some("gossamer")
    );

    let hughes = "maybe it just sags like a heavy load\nor does it explode\n";
    assert_eq!(
        scored(hughes).sharpest().unwrap().span().text(hughes),
        Some("explode")
    );
}

#[test]
fn function_words_are_the_flattest_things_in_a_line() {
    let source = "the fire of the night and the sea\n";
    let rep = scored(source);
    let the = score_of(source, "the");
    let fire = score_of(source, "fire");
    assert!(fire > the, "{} vs {}", fire.as_f64(), the.as_f64());
    assert_eq!(rep.words().len(), 8);
}

#[test]
fn context_moves_the_score_of_an_unchanged_word() {
    // §4.4's second term. Same word, same rarity, different neighbours.
    let among_kin = "burn turn spurn yearn\n";
    let among_strangers = "burn glass cheek psalm\n";
    let kin = score_of(among_kin, "burn");
    let strangers = score_of(among_strangers, "burn");
    assert!(
        strangers > kin,
        "{} among strangers vs {} among kin",
        strangers.as_f64(),
        kin.as_f64()
    );
}

#[test]
fn a_coinage_is_maximally_rare() {
    let lexicon = EnglishLexicon::new();
    assert_eq!(lexicon.rarity("krapplesnaffle"), Rarity::RAREST);

    let source = "the tide and the krapplesnaffle\n";
    let rep = scored(source);
    assert_eq!(
        rep.sharpest().unwrap().span().text(source),
        Some("krapplesnaffle")
    );
}

#[test]
fn a_guessed_pronunciation_is_marked() {
    let source = "the krapplesnaffle burns\n";
    let rep = scored(source);
    let coinage = rep
        .words()
        .iter()
        .find(|w| w.span().text(source) == Some("krapplesnaffle"))
        .unwrap();
    assert!(coinage.rests_on_a_guess());
    let known = rep
        .words()
        .iter()
        .find(|w| w.span().text(source) == Some("the"))
        .unwrap();
    assert!(!known.rests_on_a_guess());
}

#[test]
fn archaic_words_are_ordinary_in_a_poetry_corpus() {
    // The reason the table is built from verse rather than the web: these are common in poems.
    let lexicon = EnglishLexicon::new();
    for word in ["thee", "thy", "hath"] {
        assert!(lexicon.rarity(word) < Rarity::RAREST, "{word}");
    }
}

#[test]
fn scoring_real_verse_is_deterministic() {
    let source =
        "the curfew tolls the knell of parting day\nthe lowing herd wind slowly oer the lea\n";
    assert_eq!(scored(source), scored(source));
}
