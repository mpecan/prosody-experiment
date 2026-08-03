//! How well word families match real English morphology, measured rather than asserted.
//!
//! `SPEC.md` §4.3 calls the morphological row the one that earns the project, and §1 names the
//! failure it has to avoid: output that sounds fine but where no audible feature traces to a
//! specific word. A family holding `for`, `fort`, `forest` and `ford` is exactly that — a
//! modulation landing on "forest" because of an accident in a table.
//!
//! The rule was measured at 45% precision when it checked only that a word ended in a plausible
//! suffix. Requiring the *right* suffix for the preceding sound — English picks `S` after a
//! voiceless consonant and `Z` after a voiced one — is what fixed it: **80.6% recall at a 5.0%
//! false-join rate**. These floors keep it fixed.
//!
//! Ground truth comes from spelling: a pair counts as real morphology when one word is the other
//! plus a written suffix. That is independent of the phonetic rule under test, so the numbers are
//! not circular.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::cast_precision_loss // counting word pairs; far under 2^52
)]

use std::collections::{BTreeMap, BTreeSet};

use prosody_cmudict::CmuDict;
use prosody_core::sound::{self, SoundRepetition};
use prosody_core::{Fallback, Poem};
use prosody_lts::LetterToSound;

const RAW: &str = include_str!("../../prosody-cmudict/data/cmudict.dict");

/// Dictionary headwords, lowercase and alphabetic only.
fn vocabulary() -> Vec<&'static str> {
    RAW.lines()
        .filter_map(|line| {
            let word = line.split_whitespace().next()?;
            word.chars().all(|c| c.is_ascii_lowercase()).then_some(word)
        })
        .collect()
}

/// Runs a pair through the real pass and reports whether they landed in one family.
fn joined(a: &str, b: &str) -> bool {
    let source = format!("{a} {b}\n");
    let poem = Poem::parse(source.clone()).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let rep: SoundRepetition = sound::analyse(&poem, &phonemiser);
    rep.forms().len() == 2 && rep.forms()[0].voice() == rep.forms()[1].voice()
}

/// Spelling suffixes, used as ground truth independently of the phonetic rule under test.
const WRITTEN: &[&str] = &["s", "es", "ed", "ing", "er", "est", "ly", "ness"];

/// Real morphological pairs: a root in the dictionary, plus a written inflection of it also in
/// the dictionary.
fn true_pairs(limit: usize) -> Vec<(&'static str, &'static str)> {
    let vocab: BTreeSet<&str> = vocabulary().into_iter().collect();
    let mut pairs = Vec::new();
    for root in vocabulary() {
        if root.len() < 4 {
            continue;
        }
        for suffix in WRITTEN {
            let inflected = format!("{root}{suffix}");
            if let Some(found) = vocab.get(inflected.as_str()) {
                pairs.push((root, *found));
                break;
            }
        }
        if pairs.len() >= limit {
            break;
        }
    }
    pairs
}

/// Pairs that are *not* morphology: a word and another word that merely starts the same way.
///
/// These are the traps — `for`/`fort`, `car`/`cart`, `sing`/`sink` — where the longer word is not
/// the shorter plus any written suffix.
fn false_pairs(limit: usize) -> Vec<(&'static str, &'static str)> {
    let vocab = vocabulary();
    let by_prefix: BTreeMap<&str, Vec<&str>> = {
        let mut map: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for word in &vocab {
            if word.len() >= 3 {
                map.entry(&word[..3]).or_default().push(word);
            }
        }
        map
    };

    let mut pairs = Vec::new();
    for words in by_prefix.values() {
        for shorter in words {
            for longer in words {
                if longer.len() <= shorter.len() || !longer.starts_with(shorter) {
                    continue;
                }
                let added = &longer[shorter.len()..];
                if WRITTEN.contains(&added) {
                    continue;
                }
                pairs.push((*shorter, *longer));
                if pairs.len() >= limit {
                    return pairs;
                }
            }
        }
    }
    pairs
}

#[test]
fn word_families_match_real_morphology() {
    let truths = true_pairs(1200);
    let falsehoods = false_pairs(1200);
    assert!(truths.len() > 800, "only {} true pairs", truths.len());
    assert!(
        falsehoods.len() > 800,
        "only {} false pairs",
        falsehoods.len()
    );

    let recalled = truths.iter().filter(|(a, b)| joined(a, b)).count();
    let admitted = falsehoods.iter().filter(|(a, b)| joined(a, b)).count();

    let recall = recalled as f64 * 100.0 / truths.len() as f64;
    let false_rate = admitted as f64 * 100.0 / falsehoods.len() as f64;

    println!("true pairs      {}", truths.len());
    println!("recall          {recall:.1}%");
    println!("false pairs     {}", falsehoods.len());
    println!("false join rate {false_rate:.1}%");

    // Floors, not targets. Raise them when the rule improves; see the module doc.
    assert!(recall >= 78.0, "recall fell to {recall:.1}%");
    assert!(
        false_rate <= 6.0,
        "false joins rose to {false_rate:.1}% — unrelated words are sharing a voice"
    );
}

#[test]
fn the_traps_that_motivated_the_rule_stay_apart() {
    // Named in the review that measured 45% precision. Each is one phoneme apart from a real
    // inflection and none of them is one.
    for (a, b) in [
        ("for", "fort"),
        ("car", "cart"),
        ("star", "start"),
        ("sing", "sink"),
        ("hard", "heart"),
        ("cold", "colt"),
    ] {
        assert!(!joined(a, b), "{a}/{b} were joined");
    }
}

#[test]
fn the_inflections_that_matter_still_join() {
    for (a, b) in [
        ("burn", "burnt"),
        ("burn", "burning"),
        ("walk", "walked"),
        ("walk", "walks"),
        ("sing", "singing"),
        ("love", "loved"),
        ("kiss", "kisses"),
        ("wait", "waited"),
        ("dark", "darkness"),
        ("slow", "slowly"),
    ] {
        assert!(joined(a, b), "{a}/{b} were not joined");
    }
}
