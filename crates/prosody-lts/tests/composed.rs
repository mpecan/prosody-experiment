//! The phonemiser `SPEC.md` §7.2 actually asks for: dictionary first, guess second.
//!
//! Neither half is interesting alone. What matters is the seam — that a real word keeps its real
//! pronunciation, that an invented one still makes a sound, and that nothing downstream has to
//! guess which of those happened.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::{Fallback, Phoneme, Phonemiser, Provenance, SyllabifyError, Unpronounceable};
use prosody_lts::LetterToSound;

fn chain() -> Fallback<CmuDict, LetterToSound> {
    Fallback::new(CmuDict::new(), LetterToSound::new()).expect("both halves speak ARPAbet")
}

fn symbols(word: &str) -> Vec<&'static str> {
    chain()
        .pronounce(word)
        .unwrap()
        .phonemes()
        .map(Phoneme::symbol)
        .collect()
}

#[test]
fn a_known_word_keeps_its_dictionary_pronunciation() {
    let chain = chain();
    for word in ["burn", "burning", "burnt", "remember", "tide"] {
        let pronunciation = chain.pronounce(word).unwrap();
        assert_eq!(
            pronunciation.provenance(),
            Provenance::Dictionary,
            "{word} should come from the dictionary"
        );
        assert_eq!(pronunciation, CmuDict::new().pronounce(word).unwrap());
    }
}

#[test]
fn an_invented_word_still_makes_a_sound() {
    // SPEC.md §2: typing "krapplesnaffle" and having it become sound is the point of the
    // vocable-synthesis lineage this project sits in.
    let pronunciation = chain().pronounce("krapplesnaffle").unwrap();
    assert_eq!(pronunciation.provenance(), Provenance::LetterToSound);
    assert!(pronunciation.syllable_count() >= 3);
}

#[test]
fn a_non_english_word_degrades_to_a_visible_guess() {
    // SPEC.md §9: Slovenian is unsupported. It should be audibly approximate and plainly marked,
    // not silent and not silently wrong.
    let pronunciation = chain().pronounce("gorči").unwrap();
    assert_eq!(pronunciation.provenance(), Provenance::LetterToSound);
    assert!(pronunciation.syllable_count() >= 2);
}

#[test]
fn a_word_with_no_syllable_is_not_invented_one() {
    // "hm" is in the dictionary and genuinely has no nucleus. The fallback must not step in and
    // manufacture a vowel: that would add sound the poem does not contain.
    let error = chain().pronounce("hm").unwrap_err();
    assert_eq!(
        error,
        Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus)
    );
}

#[test]
fn a_word_neither_half_can_read_is_refused() {
    assert_eq!(chain().pronounce("漢字"), Err(Unpronounceable::NotKnown));
    assert_eq!(chain().pronounce(""), Err(Unpronounceable::NotKnown));
}

#[test]
fn the_dictionary_wins_where_spelling_would_mislead() {
    // Words whose spelling the rules get wrong, and which the dictionary therefore has to cover.
    let chain = chain();
    for word in ["one", "colonel", "though", "yacht", "island"] {
        let pronunciation = chain.pronounce(word).unwrap();
        assert_eq!(
            pronunciation.provenance(),
            Provenance::Dictionary,
            "{word} must not fall through to the guesser"
        );
        assert_ne!(
            symbols(word),
            LetterToSound::new()
                .pronounce(word)
                .unwrap()
                .phonemes()
                .map(Phoneme::symbol)
                .collect::<Vec<_>>(),
            "{word}: the guess and the dictionary should disagree, or this proves nothing"
        );
    }
}

#[test]
fn a_whole_couplet_is_pronounceable() {
    // The §8 acceptance test will need every token of a poem to produce sound. This checks the
    // composed phonemiser covers ordinary English plus one invented word.
    let chain = chain();
    let couplet = "the fire that burns the fire that burnt \
                   and krapplesnaffle turning in the tide";
    let mut guessed = 0_usize;
    for word in couplet.split_whitespace() {
        let pronunciation = chain
            .pronounce(word)
            .unwrap_or_else(|e| panic!("{word:?} produced no sound: {e}"));
        if pronunciation.provenance() == Provenance::LetterToSound {
            guessed += 1;
        }
    }
    assert_eq!(guessed, 1, "only the invented word should be guessed");
}

#[test]
fn the_chain_is_deterministic() {
    let chain = chain();
    for word in ["burn", "krapplesnaffle", "gorči", "'tis"] {
        assert_eq!(chain.pronounce(word), chain.pronounce(word), "{word}");
    }
}
