//! The arrangement rule, through the real passes: stanzas loop until a competing stanza transforms
//! them or a refrain stanza overrules them.
//!
//! The phonemiser is a toy: each `a` is an unstressed syllable and each `A` a stressed one, other
//! letters silent. Lines of equal length make a regular stanza (foundation, `SPEC.md` §4.1); a
//! one-syllable line beside a six-syllable one makes an irregular stanza (flourish).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use prosody_core::lower::{self, Analyses};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Cause, Layer, Poem, meter, repetition, rhyme, role, sound, surprisal};

struct Toy;

impl Phonemiser for Toy {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let phones: Vec<_> = word
            .chars()
            .filter_map(|letter| match letter {
                'a' => Some((ARPABET.phoneme("AH").unwrap(), Stress::Unstressed)),
                'A' => Some((ARPABET.phoneme("AA").unwrap(), Stress::Primary)),
                _ => None,
            })
            .collect();
        if phones.is_empty() {
            return Err(Unpronounceable::NotKnown);
        }
        Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
    }
}

struct Flat;

impl Lexicon for Flat {
    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }
}

fn compile(source: &str) -> Score {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Toy);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme::analyse(&poem, &Toy),
        repetition: &repetition,
        sound: &sound::analyse(&poem, &Toy),
        surprisal: &surprisal::analyse(&poem, &Toy, &Flat),
        roles: &roles,
    })
    .unwrap()
}

/// Each slot as `(bar, cause)` pairs, host first.
fn slots(score: &Score) -> Vec<Vec<(usize, Cause)>> {
    score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| {
            slot.placements()
                .iter()
                .map(|placement| (placement.bar().get(), placement.cause()))
                .collect()
        })
        .collect()
}

/// Regular (bars 0–1), irregular (bars 2–3), regular (bars 4–5).
const THREE: &str = "da dA\nsa sA\n\nta\nma mA ma mA ma mA\n\nna nA\nla lA\n";

#[test]
fn the_layers_are_what_the_rule_assumes() {
    // The rest of this file depends on these, so it checks them rather than trusting them.
    let score = compile(THREE);
    let layer = |bar: usize| {
        let event = score
            .events()
            .iter()
            .find(|e| e.bar().get() == bar)
            .unwrap();
        score.voice(event.voice()).unwrap().layer()
    };
    assert_eq!(layer(0), Layer::Foundation);
    assert_eq!(layer(2), Layer::Flourish);
    assert_eq!(layer(4), Layer::Foundation);
}

#[test]
fn a_stanza_loops_under_the_next_and_transforms_into_its_rival() {
    use Cause::{Arriving, Looping, Stated, Yielding};
    let score = compile(THREE);
    assert_eq!(
        slots(&score),
        vec![
            // Stanza 1 is stated into an empty foundation, then loops.
            vec![(0, Stated)],
            vec![(1, Stated)],
            // Stanza 2 is stated into an empty flourish, with stanza 1 looping under it.
            vec![(2, Stated), (0, Looping)],
            vec![(3, Stated), (1, Looping)],
            // Stanza 3 competes with stanza 1 for the foundation. Pass 1: its first line replaces
            // the loop's first bar. Pass 2: it is the whole loop. Stanza 2 loops underneath, bar
            // after bar, from its own start.
            vec![(4, Arriving { pass: 1 }), (2, Looping)],
            vec![(1, Yielding { pass: 1 }), (3, Looping)],
            vec![(4, Arriving { pass: 2 }), (2, Looping)],
            vec![(5, Arriving { pass: 2 }), (3, Looping)],
        ]
    );
}

#[test]
fn a_slot_takes_its_hosts_length() {
    // Slot 3 hosts `ma mA ma mA ma mA` — three iambs, three beats — with stanza 1's one-beat
    // `sa sA` fitted under it. Slot 2 hosts `ta`: one syllable, one beat.
    let score = compile(THREE);
    let beats: Vec<usize> = score
        .arrangement()
        .slots()
        .iter()
        .map(prosody_core::Slot::beats)
        .collect();
    assert_eq!(beats[2], 1);
    assert_eq!(beats[3], 3);
    assert_eq!(
        score.beats_in(1),
        1,
        "the looped line keeps its own beat count"
    );
}

#[test]
fn a_refrain_stanza_overrules_every_loop() {
    use Cause::{Looping, Overruling, Stated};
    // Stanza 3 repeats stanza 1 verbatim, so both are made of refrain lines: both overrule, and
    // stanza 3 arrives alone — the flourish loop under it stops.
    let score = compile("da dA\nsa sA\n\nta\nma mA ma mA ma mA\n\nda dA\nsa sA\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Overruling)],
            vec![(1, Overruling)],
            vec![(2, Stated), (0, Looping)],
            vec![(3, Stated), (1, Looping)],
            vec![(4, Overruling)],
            vec![(5, Overruling)],
        ]
    );
}

#[test]
fn every_placed_line_has_material() {
    // I4: a slot may only hold lines the lowering produced events for, or a silent line that is
    // listed as such — never a bar that traces to nothing.
    let score = compile(THREE);
    for slot in score.arrangement().slots() {
        for placement in slot.placements() {
            assert!(placement.bar().get() < score.bars());
        }
    }
}
