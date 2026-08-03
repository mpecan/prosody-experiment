//! How wrong the guesser is, measured against the dictionary rather than asserted.
//!
//! `SPEC.md` §1 names the failure mode this guards: output that sounds plausible but is not
//! traceable to anything real. A ruleset that produces confident nonsense passes every unit test
//! written for it, because those tests were written from the same rules. The only defence is to
//! grade it against pronunciations nobody involved chose — all 124,000 gradable `CMUdict`
//! entries.
//!
//! The floors below are deliberately a little under the measured figures, so ordinary noise does
//! not fail the build but a real regression does. **Raise them when the guesser improves**: a
//! floor that drifts far below the truth stops being a test.
//!
//! Syllable-count accuracy matters most. `SPEC.md` §4.2 maps syllables to subdivisions and feet,
//! so a wrong count changes the rhythm; a wrong vowel only changes the timbre.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::cast_precision_loss // counting words; the corpus is far under 2^52
)]

use prosody_cmudict::CmuDict;
use prosody_core::{Phoneme, Phonemiser, Stress};
use prosody_lts::LetterToSound;

const RAW: &str = include_str!("../../prosody-cmudict/data/cmudict.dict");

/// Every base entry, as `(word, true phone symbols)`, alternates excluded.
fn entries() -> Vec<(&'static str, Vec<&'static str>)> {
    RAW.lines()
        .filter_map(|line| {
            let line = line.split('#').next().unwrap_or(line).trim();
            let mut fields = line.split_whitespace();
            let word = fields.next()?;
            if word.ends_with(')') || !word.chars().all(|c| c.is_ascii_alphabetic()) {
                return None;
            }
            let phones: Vec<&str> = fields
                .map(|f| f.trim_end_matches(['0', '1', '2']))
                .collect();
            (!phones.is_empty()).then_some((word, phones))
        })
        .collect()
}

/// Levenshtein distance over phone symbols.
fn edit_distance(a: &[&str], b: &[&str]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0_usize; b.len() + 1];
    for (i, &left) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, &right) in b.iter().enumerate() {
            let cost = usize::from(left != right);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        previous.clone_from(&current);
    }
    previous[b.len()]
}

struct Score {
    graded: usize,
    exact: usize,
    syllables_right: usize,
    distance: usize,
    phones: usize,
    stress_right: usize,
    stress_graded: usize,
}

fn grade() -> Score {
    let lts = LetterToSound::new();
    let dict = CmuDict::new();
    let mut score = Score {
        graded: 0,
        exact: 0,
        syllables_right: 0,
        distance: 0,
        phones: 0,
        stress_right: 0,
        stress_graded: 0,
    };

    for (word, truth) in entries() {
        let Ok(guess) = lts.pronounce(word) else {
            continue;
        };
        let Ok(reference) = dict.pronounce(word) else {
            continue;
        };
        let guessed: Vec<&str> = guess.phonemes().map(Phoneme::symbol).collect();

        score.graded += 1;
        score.phones += truth.len();
        score.distance += edit_distance(&guessed, &truth);
        if guessed == truth {
            score.exact += 1;
        }
        if guess.syllable_count() == reference.syllable_count() {
            score.syllables_right += 1;
        }

        // Where the primary stress falls, among words that have more than one syllable.
        if reference.syllable_count() > 1 {
            score.stress_graded += 1;
            let position = |p: &prosody_core::Pronunciation| {
                p.stress_pattern()
                    .iter()
                    .position(|s| *s == Stress::Primary)
            };
            if position(&guess) == position(&reference) {
                score.stress_right += 1;
            }
        }
    }
    score
}

#[test]
fn the_guesser_is_graded_against_the_whole_dictionary() {
    let score = grade();
    let percent = |n: usize, of: usize| (n as f64) * 100.0 / (of as f64);

    let exact = percent(score.exact, score.graded);
    let syllables = percent(score.syllables_right, score.graded);
    let stress = percent(score.stress_right, score.stress_graded);
    let error_rate = (score.distance as f64) / (score.phones as f64);

    println!("graded          {}", score.graded);
    println!("exact match     {exact:.1}%");
    println!("syllable count  {syllables:.1}%");
    println!("stress position {stress:.1}%");
    println!("phone error     {error_rate:.3}");

    assert!(score.graded > 100_000, "only graded {}", score.graded);

    // Floors, not targets. See the module doc: raise them when the guesser improves.
    assert!(exact >= 24.0, "exact-match accuracy fell to {exact:.1}%");
    assert!(
        syllables >= 82.0,
        "syllable-count accuracy fell to {syllables:.1}%"
    );
    assert!(stress >= 66.0, "stress accuracy fell to {stress:.1}%");
    assert!(
        error_rate <= 0.25,
        "phone error rate rose to {error_rate:.3}"
    );
}

#[test]
fn the_guesser_covers_almost_every_word_it_is_shown() {
    // Coverage is separate from accuracy: a guesser that declined most words could score well on
    // the ones it attempted while leaving a poem full of silence.
    let lts = LetterToSound::new();
    let all = entries();
    let attempted = all
        .iter()
        .filter(|(word, _)| lts.pronounce(word).is_ok())
        .count();
    let coverage = (attempted as f64) * 100.0 / (all.len() as f64);
    println!("coverage        {coverage:.1}%");
    assert!(coverage >= 99.0, "coverage fell to {coverage:.1}%");
}
