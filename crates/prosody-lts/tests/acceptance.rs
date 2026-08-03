//! `SPEC.md` §8 — "First test to write — before any of the fun parts".
//!
//! > Take a couplet. Swap one word for a synonym with different phonetics and stress. Confirm you
//! > can **hear a specific, explicable change**, and that the change is confined to the expected
//! > voices.
//!
//! This is the project's reason to exist, and it is checkable at the IR without a backend: an
//! event list *is* what would be heard. `CLAUDE.md` §7.4 puts it before any backend polish for
//! that reason.
//!
//! What "specific and explicable" means here is that each assertion below names the mapping and
//! the `SPEC.md` section that predicts it, and the expected values are worked out from the two
//! words rather than read off the implementation.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::Score;
use prosody_core::{Fallback, Poem, Span, meter, repetition, rhyme, role, sound, surprisal};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

/// The couplet, with a second stanza that the swap must not touch.
const BEFORE: &str = "the fire will burn the night\n\
                      and turn the dark to light\n\
                      \n\
                      the river finds the sea\n\
                      and carries you and me\n";

/// The same poem with one word swapped: "burn" becomes "flicker".
///
/// Chosen to differ in exactly the ways §8 asks for. Phonetically: `B ER1 N` against
/// `F L IH1 K ER0`. Metrically: one stressed syllable against two, stressed-then-unstressed — an
/// iamb's worth of monosyllable replaced by a trochee.
///
/// Both words are in `CMUdict` *and* in the rarity table, which two earlier drafts were not.
/// "smoulder" is absent from the dictionary, so the pipeline guessed it by letter-to-sound and
/// the test graded a 68%-accurate ruleset instead of a mapping. "smolder" fixed that but sits in
/// the band reserved for *absence* from the rarity table, so the §4.4 assertion reduced to
/// "a word we have never seen outranks one we have" — true of any nonsense string, and no test
/// of the mapping at all. "flicker" is band 6 against "burn" at band 5: a real gradient, inside
/// the table, in the direction §4.4 predicts.
const AFTER: &str = "the fire will flicker the night\n\
                     and turn the dark to light\n\
                     \n\
                     the river finds the sea\n\
                     and carries you and me\n";

fn lower(source: &str) -> Score {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let lexicon = EnglishLexicon::new();
    let meter = meter::scan(&poem, &phonemiser);
    let rhyme = rhyme::analyse(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, &phonemiser);
    let surprisal = surprisal::analyse(&poem, &phonemiser, &lexicon);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme,
        repetition: &repetition,
        sound: &sound,
        surprisal: &surprisal,
        roles: &roles,
    })
    .unwrap()
}

/// What a bar plays, with spans made relative so that a byte shift earlier in the poem does not
/// register as a change. Two bars with the same shape sound the same.
fn shape(score: &Score, bar: usize) -> Vec<String> {
    score
        .events()
        .iter()
        .filter(|event| event.bar().get() == bar)
        .map(|event| {
            // Every field that decides what is heard. An earlier version omitted the voice and
            // the family, and review proved both: an event list where every event was reassigned
            // to the first line's voice, and one where each word's family leaked to the next
            // word, both passed this test unchanged. §8's wording is "confined to the expected
            // *voices*", so the voice has to be in here.
            format!(
                "{:?} {} {}/{} {:?} {:.3} {:.3} {:?} {:?} {:?}",
                event.voice(),
                event.beat().get(),
                event.subdivision().index(),
                event.subdivision().of(),
                event.foot(),
                event.velocity().as_f64(),
                event.sharpness().as_f64(),
                event
                    .pitch()
                    .map(|p| (p.degree().get(), p.detune().as_f64().to_bits())),
                event.family().map(|f| (f.voice(), f.depth())),
                event.anchor(),
            )
        })
        .collect()
}

#[test]
fn both_words_are_dictionary_facts() {
    // The swap must exercise the mapping, not the letter-to-sound guesser. CLAUDE.md §6 requires
    // a fallback to be visible; here it must not be taken at all.
    let before = lower(BEFORE);
    let after = lower(AFTER);
    assert!(
        before.events().iter().all(|e| !e.rests_on_a_guess()),
        "{:?}",
        before
            .events()
            .iter()
            .filter(|e| e.rests_on_a_guess())
            .filter_map(|e| e.span().text(BEFORE))
            .collect::<Vec<_>>()
    );
    assert!(
        after.events().iter().all(|e| !e.rests_on_a_guess()),
        "{:?}",
        after
            .events()
            .iter()
            .filter(|e| e.rests_on_a_guess())
            .filter_map(|e| e.span().text(AFTER))
            .collect::<Vec<_>>()
    );
    assert!(before.silent().is_empty(), "every word sounds");
    assert!(after.silent().is_empty());
}

#[test]
fn swapping_one_word_changes_that_voice_and_only_that_voice() {
    let before = lower(BEFORE);
    let after = lower(AFTER);

    // Both poems have four lines, so four bars. The swap changes what a bar plays, never how many
    // there are — SPEC.md §4.2 maps line to bar, and the edit did not add a line.
    assert_eq!(before.bars(), 4, "four lines, four bars");
    assert_eq!(after.bars(), 4);

    // (a) A specific, explicable change in the edited voice.
    //
    // "burn" is one syllable and "flicker" is two, so bar 0 gains exactly one event (§4.2,
    // syllable → subdivision). Everything else in the poem is untouched, so the poem gains
    // exactly one event in total.
    assert_eq!(
        after.events().len(),
        before.events().len() + 1,
        "flicker adds one syllable to the poem"
    );
    assert_ne!(shape(&before, 0), shape(&after, 0), "bar 0 must change");

    // (b) Confined to the expected voices: every other bar plays exactly what it did, in every
    // field that reaches a listener.
    for bar in 1..4 {
        assert_eq!(
            shape(&before, bar),
            shape(&after, bar),
            "bar {bar} must be untouched"
        );
    }
}

#[test]
fn the_change_is_attributable_to_the_word_that_moved() {
    // Invariant I4, at the grain that matters: the events that differ are the ones belonging to
    // the swapped word, and no others.
    let before = lower(BEFORE);
    let after = lower(AFTER);

    let burn = Span::new(
        BEFORE.find("burn").unwrap(),
        BEFORE.find("burn").unwrap() + "burn".len(),
    );
    let flicker = Span::new(
        AFTER.find("flicker").unwrap(),
        AFTER.find("flicker").unwrap() + "flicker".len(),
    );

    assert_eq!(
        before.events().iter().filter(|e| e.span() == burn).count(),
        1,
        "burn is one syllable"
    );
    assert_eq!(
        after
            .events()
            .iter()
            .filter(|e| e.span() == flicker)
            .count(),
        2,
        "flicker is two"
    );

    // Every word other than the swapped one plays the same number of events either side.
    let words_of = |score: &Score, source: &str| {
        let mut counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        for event in score.events() {
            if let Some(text) = event.span().text(source) {
                *counts.entry(text.to_owned()).or_default() += 1;
            }
        }
        counts
    };
    let mut a = words_of(&before, BEFORE);
    let mut b = words_of(&after, AFTER);
    assert_eq!(a.remove("burn"), Some(1));
    assert_eq!(b.remove("flicker"), Some(2));
    assert_eq!(a, b, "no other word changed how much it plays");
}

#[test]
fn the_rhyme_that_did_not_move_keeps_its_pitch() {
    // "night" and "light" rhyme, and the swap touches neither. SPEC.md §4.5 maps a rhyme class to
    // a scale degree, so a pitch moving here would mean editing one word had transposed a line it
    // has nothing to do with.
    let before = lower(BEFORE);
    let after = lower(AFTER);

    let pitches = |score: &Score, source: &str| {
        let mut found: Vec<(String, usize, bool)> = score
            .events()
            .iter()
            .filter_map(|event| {
                let pitch = event.pitch()?;
                Some((
                    event.span().text(source)?.to_owned(),
                    pitch.degree().get(),
                    pitch.detune().is_exact(),
                ))
            })
            .collect();
        found.sort();
        found.dedup();
        found
    };

    let before_pitches = pitches(&before, BEFORE);
    assert!(
        before_pitches.iter().any(|(word, ..)| word == "night"),
        "expected the rhyme to be pitched: {before_pitches:?}"
    );
    assert_eq!(
        before_pitches,
        pitches(&after, AFTER),
        "the melody is made of rhyme words, and none of them moved"
    );
}

#[test]
fn the_swap_is_audible_in_the_word_itself() {
    // The point of §4.4: a rarer, longer word is a sharper, brighter event. This asserts the
    // direction rather than a value — the lexicon's exact band is palette-adjacent tuning, but
    // "flicker is more distinctive than burn" is a fact about English.
    let before = lower(BEFORE);
    let after = lower(AFTER);

    let burn = before
        .events()
        .iter()
        .find(|e| e.span().text(BEFORE) == Some("burn"))
        .expect("burn");
    let flicker = after
        .events()
        .iter()
        .find(|e| e.span().text(AFTER) == Some("flicker"))
        .expect("flicker");

    assert!(
        flicker.sharpness() > burn.sharpness(),
        "flicker {:?} should be sharper than burn {:?}",
        flicker.sharpness().as_f64(),
        burn.sharpness().as_f64()
    );
    // ...and both are known words, so neither is riding the absent-from-the-table ceiling.
    assert!(burn.sharpness() > prosody_core::surprisal::Surprisal::NONE);
    assert!(flicker.sharpness().as_f64() < 1.0, "not the ceiling");
}

#[test]
fn the_first_syllable_of_the_new_word_carries_the_stress() {
    // "flicker" is a trochee: SMOUL-der. §4.2 maps stress to velocity, so the two events it
    // contributes must be loud-then-soft — the opposite of the iamb "burn" sat in.
    let after = lower(AFTER);
    let mut its: Vec<_> = after
        .events()
        .iter()
        .filter(|e| e.span().text(AFTER) == Some("flicker"))
        .collect();
    its.sort_by_key(|e| (e.bar().get(), e.beat().get(), e.subdivision().index()));
    assert_eq!(its.len(), 2);
    assert!(
        its[0].velocity() > its[1].velocity(),
        "SMOUL-der, not smoul-DER: {:?} then {:?}",
        its[0].velocity().as_f64(),
        its[1].velocity().as_f64()
    );
}
