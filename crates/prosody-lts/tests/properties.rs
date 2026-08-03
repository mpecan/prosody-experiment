//! Properties of guessing, over arbitrary input.
//!
//! Every property here is paired with an anchor that fails if the guesser stops guessing. An
//! earlier version stated all three inside `if let Ok(..)` arms, so an implementation that
//! returned `Err` for everything satisfied them all by never entering the interesting branch —
//! the same vacuous shape this project has now caught three times.

// Property-test bodies unwrap and index freely, as test code may. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use proptest::prelude::{Strategy, prop_assert, prop_assert_eq, proptest};
use prosody_core::{Phonemiser, Provenance, Unpronounceable};
use prosody_lts::LetterToSound;

/// Ordinary-looking words containing at least one vowel letter.
///
/// The vowel is required, not incidental: a consonant-only string like "b" has no nucleus, and
/// declining it is correct behaviour rather than a failure to guess. `q` is excluded for the same
/// reason — `qu` swallows its own vowel, so "qu" alone has no nucleus either. Real `qu` words are
/// covered by the fixed anchor below.
fn plain_word() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-prs-z]{0,5}[aeiou][a-prs-z]{0,5}").unwrap()
}

/// Any text at all, including scripts and symbols the guesser has no rules for.
fn any_word() -> impl Strategy<Value = String> {
    proptest::string::string_regex(
        "[a-zA-Z'\u{2019}\u{2010}\u{00e0}-\u{017f}\u{4e00}-\u{4e10}\u{1f300}-\u{1f310} -]{0,16}",
    )
    .unwrap()
}

/// The anchor. Without it the properties below can all hold vacuously.
#[test]
fn the_guesser_actually_guesses() {
    let lts = LetterToSound::new();
    for word in [
        "cat",
        "burn",
        "krapplesnaffle",
        "table",
        "judge",
        "knight",
        "hoped",
        "queen",
        "though",
    ] {
        let pronunciation = lts
            .pronounce(word)
            .unwrap_or_else(|e| panic!("{word:?} should be guessable: {e}"));
        assert!(pronunciation.syllable_count() > 0, "{word}");
        assert_eq!(pronunciation.provenance(), Provenance::LetterToSound);
    }
}

proptest! {
    /// Guessing is total: for any input it either declines or returns a usable pronunciation.
    /// It never panics, never loops, and never returns something malformed.
    #[test]
    fn guessing_is_total(word in any_word()) {
        match LetterToSound::new().pronounce(&word) {
            Ok(pronunciation) => {
                prop_assert!(pronunciation.syllable_count() > 0);
                for syllable in pronunciation.syllables() {
                    prop_assert!(syllable.nucleus().is_vowel());
                }
            }
            Err(error) => prop_assert!(matches!(
                error,
                Unpronounceable::NotKnown | Unpronounceable::Unsyllabifiable(_)
            )),
        }
    }

    /// An ordinary lowercase word is always guessable. Paired with `guessing_is_total`, this is
    /// what stops an always-`Err` implementation from passing.
    #[test]
    fn a_plain_word_always_gets_a_pronunciation(word in plain_word()) {
        let pronunciation = LetterToSound::new().pronounce(&word);
        prop_assert!(pronunciation.is_ok(), "{word:?} produced {pronunciation:?}");
    }

    /// `CLAUDE.md` §6 — every guess is labelled a guess. Nothing here may claim to be a
    /// dictionary fact, or the "text-shaped seed" failure of `SPEC.md` §1 becomes invisible.
    #[test]
    fn every_guess_is_labelled_a_guess(word in plain_word()) {
        let pronunciation = LetterToSound::new().pronounce(&word).unwrap();
        prop_assert_eq!(pronunciation.provenance(), Provenance::LetterToSound);
        prop_assert!(pronunciation.provenance().is_guess());
    }

    /// I1 — guessing is a pure function of the spelling.
    #[test]
    fn i1_guessing_is_reproducible(word in any_word()) {
        let lts = LetterToSound::new();
        prop_assert_eq!(lts.pronounce(&word), lts.pronounce(&word));
    }

    /// Case and the punctuation that merely joins a word do not change how it is guessed.
    #[test]
    fn guessing_ignores_case(word in plain_word()) {
        let lts = LetterToSound::new();
        prop_assert_eq!(lts.pronounce(&word), lts.pronounce(&word.to_uppercase()));
    }

    /// Every phoneme produced belongs to the inventory the guesser reports, so nothing it makes
    /// can silently fail to compare against a dictionary pronunciation.
    #[test]
    fn phonemes_come_from_the_reported_inventory(word in plain_word()) {
        let lts = LetterToSound::new();
        let pronunciation = lts.pronounce(&word).unwrap();
        for phoneme in pronunciation.phonemes() {
            prop_assert_eq!(phoneme.inventory().name, lts.inventory().name);
        }
    }
}

#[test]
fn a_written_vowel_survives_as_a_spoken_one() {
    // "dge" spells one grapheme, `JH`, so the rules consume the word's only vowel letter and
    // leave nothing to build a syllable on. A fallback phonemiser that fails here goes silent on
    // exactly the odd input it exists to catch. Found by a property run, which is why the
    // fixture looks arbitrary.
    let lts = LetterToSound::new();
    for word in ["dge", "dges", "edge"] {
        let said = lts.pronounce(word);
        assert!(said.is_ok(), "{word:?} -> {said:?}");
    }
    // ...but a word with no written vowel at all still honestly has no syllable.
    assert!(lts.pronounce("brrr").is_err());
}
