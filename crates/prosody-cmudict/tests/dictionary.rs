//! Properties that must hold across the whole vendored dictionary.
//!
//! The unit tests check hand-worked pronunciations for a handful of words. These check the
//! remaining hundred and thirty-five thousand, by re-reading the same data file independently of
//! the crate's own parser.
//!
//! # What a corpus test can and cannot prove
//!
//! An earlier version of this file claimed to catch "a syllabifier that is right about 'burning'
//! and wrong about one word in a thousand", and did not: three mutations — never forming an
//! onset, reversing the search to minimal onset, and deleting an entry from the phonotactics
//! table — all passed every test here. Conservation of phonemes and syllable counts is invariant
//! under any of those, and asserting that produced onsets are *legal* is circular, since the
//! splitter only ever returns a split the same table already approved.
//!
//! What actually bites is **maximality**: for every interior boundary, moving one more consonant
//! into the onset must be illegal. Together with legality that pins the boundary exactly, and it
//! fails immediately for a splitter that forms no onsets or the shortest one. It still cannot
//! catch a wrong *table* — for that see the hand-worked splits in `prosody-core`'s `phoneme`
//! module, which encode the expected division independently.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::{Phoneme, Phonemiser, SyllabifyError, Unpronounceable};

/// The same file the crate embeds, read here independently so the comparison is against the
/// source data rather than against the crate's own parse of it.
const RAW: &str = include_str!("../data/cmudict.dict");

/// Every entry, as `(word, phone symbols without stress digits)`, variants excluded.
fn entries() -> Vec<(&'static str, Vec<&'static str>)> {
    RAW.lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or(line).trim();
            let mut fields = line.split_whitespace();
            let word = fields.next()?;
            if word.ends_with(')') {
                return None;
            }
            let phones: Vec<&str> = fields
                .map(|f| f.trim_end_matches(['0', '1', '2']))
                .collect();
            (!phones.is_empty()).then_some((word, phones))
        })
        .collect()
}

fn is_vowel(symbol: &str) -> bool {
    matches!(
        symbol,
        "AA" | "AE"
            | "AH"
            | "AO"
            | "AW"
            | "AY"
            | "EH"
            | "ER"
            | "EY"
            | "IH"
            | "IY"
            | "OW"
            | "OY"
            | "UH"
            | "UW"
    )
}

#[test]
fn the_corpus_is_large_enough_to_be_worth_checking() {
    assert!(
        entries().len() > 100_000,
        "only {} entries",
        entries().len()
    );
}

#[test]
fn syllabification_loses_no_phoneme_and_reorders_none() {
    let dict = CmuDict::new();
    let mut checked = 0_usize;
    let mut unexpected = Vec::new();

    for (word, expected) in entries() {
        let pronunciation = match dict.pronounce(word) {
            Ok(pronunciation) => pronunciation,
            // A vowel-less entry legitimately has no syllable; anything else is a bug.
            Err(Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus)) => {
                if expected.iter().any(|p| is_vowel(p)) {
                    unexpected.push(word);
                }
                continue;
            }
            Err(other) => panic!("{word:?} failed unexpectedly: {other}"),
        };

        let got: Vec<&str> = pronunciation.phonemes().map(Phoneme::symbol).collect();
        assert_eq!(got, expected, "phonemes changed for {word:?}");

        let vowels = expected.iter().filter(|p| is_vowel(p)).count();
        assert_eq!(
            pronunciation.syllable_count(),
            vowels,
            "syllable count for {word:?}"
        );
        checked += 1;
    }

    assert!(
        unexpected.is_empty(),
        "{} words failed unexpectedly, e.g. {:?}",
        unexpected.len(),
        &unexpected[..unexpected.len().min(10)]
    );
    assert!(checked > 100_000, "only checked {checked}");
}

#[test]
fn every_interior_onset_is_maximal() {
    // The property that actually pins the boundary. For each syllable after the first, taking one
    // more consonant from the end of the previous syllable's coda must produce an illegal onset —
    // otherwise the splitter should have taken it. A splitter that forms no onset at all, or the
    // shortest legal one, fails here on the first word with an interior cluster.
    let dict = CmuDict::new();
    let inventory = dict.inventory();
    let mut with_interior_cluster = 0_usize;

    for (word, _) in entries() {
        let Ok(pronunciation) = dict.pronounce(word) else {
            continue;
        };
        let syllables = pronunciation.syllables();
        for index in 1..syllables.len() {
            let previous = &syllables[index - 1];
            let onset = syllables[index].onset();

            assert!(
                (inventory.legal_onset)(onset),
                "{word:?} produced illegal interior onset {:?}",
                onset
                    .iter()
                    .copied()
                    .map(Phoneme::symbol)
                    .collect::<Vec<_>>()
            );

            if let Some(&borrowed) = previous.coda().last() {
                with_interior_cluster += 1;
                let mut longer = Vec::with_capacity(onset.len() + 1);
                longer.push(borrowed);
                longer.extend_from_slice(onset);
                assert!(
                    !(inventory.legal_onset)(&longer),
                    "{word:?}: onset {:?} is not maximal, {:?} would also be legal",
                    onset
                        .iter()
                        .copied()
                        .map(Phoneme::symbol)
                        .collect::<Vec<_>>(),
                    longer
                        .iter()
                        .copied()
                        .map(Phoneme::symbol)
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    assert!(
        with_interior_cluster > 10_000,
        "only {with_interior_cluster} boundaries had a coda to test maximality against"
    );
}

#[test]
fn every_syllable_is_well_formed() {
    let dict = CmuDict::new();
    for (word, _) in entries() {
        let Ok(pronunciation) = dict.pronounce(word) else {
            continue;
        };
        for syllable in pronunciation.syllables() {
            assert!(syllable.nucleus().is_vowel(), "{word:?} nucleus");
            assert!(
                syllable.onset().iter().all(|p| !p.is_vowel()),
                "{word:?} onset"
            );
            assert!(
                syllable.coda().iter().all(|p| !p.is_vowel()),
                "{word:?} coda"
            );
            let rime: Vec<&str> = syllable
                .rime()
                .iter()
                .copied()
                .map(Phoneme::symbol)
                .collect();
            assert_eq!(rime[0], syllable.nucleus().symbol(), "{word:?} rime");
            assert_eq!(
                rime.len(),
                1 + syllable.coda().len(),
                "{word:?} rime length"
            );
        }
    }
}

#[test]
fn the_rhyme_tail_is_always_a_suffix_of_the_word() {
    // Rhyme is scored on this tail (SPEC.md §4.5), so it must be real phonemes from the end of
    // the word — never a reordering, never a fragment from the middle.
    let dict = CmuDict::new();
    for (word, _) in entries().iter().step_by(11) {
        let Ok(pronunciation) = dict.pronounce(word) else {
            continue;
        };
        let all: Vec<&str> = pronunciation.phonemes().map(Phoneme::symbol).collect();
        let tail: Vec<&str> = pronunciation
            .rhyme_tail()
            .iter()
            .copied()
            .map(Phoneme::symbol)
            .collect();
        assert!(!tail.is_empty(), "{word:?} has an empty rhyme tail");
        assert!(
            tail.len() <= all.len(),
            "{word:?} tail longer than the word"
        );
        assert_eq!(&all[all.len() - tail.len()..], &tail[..], "{word:?}");
        assert!(is_vowel(tail[0]), "{word:?} tail must start at a vowel");
    }
}

#[test]
fn lookups_are_reproducible() {
    // Invariant I1 at the dictionary boundary: two handles, same answers.
    let first = CmuDict::new();
    let second = CmuDict::new();
    for (word, _) in entries().iter().step_by(101) {
        assert_eq!(first.pronounce(word), second.pronounce(word), "{word:?}");
    }
}
