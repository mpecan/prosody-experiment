//! The arrangement rule, through the real passes: stanzas loop until a competing stanza transforms
//! them or a refrain stanza overrules them.
//!
//! The phonemiser is a toy: each `a` is an unstressed syllable and each `A` a stressed one, other
//! letters silent; `o` and `O` are the same with another vowel, for a second rhyme. Lines of equal length make a regular stanza (foundation, `SPEC.md` §4.1); a
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
                'o' => Some((ARPABET.phoneme("AO").unwrap(), Stress::Unstressed)),
                'O' => Some((ARPABET.phoneme("OW").unwrap(), Stress::Primary)),
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
    use Cause::{Arriving, Holding, Looping, Repeating, Stated, Yielding};
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
            // `na nA` rhymes with `la lA`, which is not heard until pass 2's second bar: it holds
            // until then, wherever it is not already the host.
            vec![(1, Yielding { pass: 1 }), (3, Looping), (4, Holding)],
            vec![(4, Arriving { pass: 2 }), (2, Looping)],
            vec![(5, Arriving { pass: 2 }), (3, Looping)],
            // The poem has ended and nothing stopped the loops: stanza 3 plays one more pass,
            // with stanza 2 still looping under it.
            vec![(4, Repeating), (2, Looping)],
            vec![(5, Repeating), (3, Looping)],
        ]
    );
}

#[test]
fn a_slot_takes_its_hosts_stanza_period() {
    // Stanza 2 is `ta` and `ma mA ma mA ma mA` — three iambs, three beats — so its period is
    // three, and both its bars last three beats: slot 3, hosting the long line, and slot 2,
    // hosting `ta`, which plays one beat and rests for two. Stanza 1's lines are one beat each.
    let score = compile(THREE);
    let beats: Vec<usize> = score
        .arrangement()
        .slots()
        .iter()
        .map(prosody_core::Slot::beats)
        .collect();
    assert_eq!(beats[..2], [1, 1]);
    assert_eq!(beats[2], 3);
    assert_eq!(beats[3], 3);
    // A one-beat loop fits a three-beat bar at its own pace; a longer one would be squeezed in.
    let slot = &score.arrangement().slots()[2];
    assert_eq!(slot.room_for(1), 1);
    assert_eq!(slot.room_for(5), 3);
    assert_eq!(
        score.beats_in(1),
        1,
        "the looped line keeps its own beat count"
    );
}

#[test]
fn a_refrain_stanza_overrules_every_loop() {
    use Cause::{Looping, Overruling, Refraining, Repeating, Stated};
    // Stanza 3 repeats stanza 1 verbatim, so both are made of refrain lines: both overrule, and
    // stanza 3 arrives alone — the flourish loop under it stops. In between, the refrain lane
    // plays `sa sA`, stanza 1's last refrain line, except where the foundation loop already does.
    let score = compile("da dA\nsa sA\n\nta\nma mA ma mA ma mA\n\nda dA\nsa sA\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Overruling)],
            vec![(1, Overruling)],
            vec![(2, Stated), (0, Looping), (1, Refraining)],
            vec![(3, Stated), (1, Looping)],
            vec![(4, Overruling)],
            vec![(5, Overruling)],
            // The outro: stanza 3 again, with its last refrain line in the lane — except under
            // itself.
            vec![(4, Repeating), (5, Refraining)],
            vec![(5, Repeating)],
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

#[test]
fn a_call_loops_from_the_bar_after_its_line() {
    use Cause::{Calling, Repeating, Stated};
    // `da dA, sa sA`: the caesura after `dA` makes `da dA` the line's call. It is heard in its line
    // in bar 0, then on its own under bar 1. In the outro it is silent under its own line, which
    // already plays it, and returns under bar 1.
    let score = compile("da dA, sa sA\nma mA ma mA\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Calling)],
            vec![(0, Repeating)],
            vec![(1, Repeating), (0, Calling)],
        ]
    );

    let call = score.arrangement().slots()[1].placements()[1];
    assert_eq!(call.lane(), prosody_core::Lane::Call);
    assert_eq!(
        call.part(),
        prosody_core::Part::Call(prosody_core::Span::new(0, 5))
    );
    // One iamb of the line's two: the call spans one beat, from the line's first.
    let placed = score.placed(call);
    assert_eq!((placed.first_beat(), placed.beats()), (0, 1));
    assert_eq!(placed.events().len(), 2, "`da` and `dA`, not `sa sA`");
}

#[test]
fn the_next_call_takes_the_lane_and_the_layers_loop_beside_it() {
    use Cause::{Calling, Looping, Repeating, Stated};
    // Stanza 1 (foundation) has a call in bar 0; stanza 2 (flourish: one syllable against six) has
    // one in bar 2, `ta`. In bar 2 stanza 1's call would sound — but its whole line is looping
    // there already, so it is left out. From bar 3 the lane is `ta`, silent again in the outro
    // under `ta`'s own line.
    let score = compile("da dA, sa sA\nma mA ma mA\n\nta, tA\nna nA na nA na nA\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Calling)],
            vec![(2, Stated), (0, Looping)],
            vec![(3, Stated), (1, Looping), (2, Calling)],
            vec![(2, Repeating), (0, Looping)],
            vec![(3, Repeating), (1, Looping), (2, Calling)],
        ]
    );
}

#[test]
fn a_refrain_stanza_silences_the_call_lane_too() {
    use Cause::{Calling, Looping, Overruling, Refraining, Repeating, Stated};
    // Stanzas 1 and 3 are the same, so both are refrain stanzas: stated alone, no loops and no
    // call under them. Stanza 2 in between hears stanza 1's loop, and its call and its last
    // refrain line wherever the loop is not already playing the line they come from. The outro
    // repeats stanza 3 with its own call and refrain line, each silent under its own line.
    let score = compile(
        "da dA, sa sA\nma mA ma mA\n\nta\nna nA na nA na nA\n\nda dA, sa sA\nma mA ma mA\n",
    );
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Overruling)],
            vec![(1, Overruling)],
            vec![(2, Stated), (0, Looping), (1, Refraining)],
            vec![(3, Stated), (1, Looping), (0, Calling)],
            vec![(4, Overruling)],
            vec![(5, Overruling)],
            vec![(4, Repeating), (5, Refraining)],
            vec![(5, Repeating), (4, Calling)],
        ]
    );
}

#[test]
fn a_trailing_comma_is_not_a_call() {
    // A comma that ends a line divides nothing.
    let score = compile("da dA sa sA,\nma mA ma mA\n");
    assert!(
        score
            .arrangement()
            .slots()
            .iter()
            .flat_map(prosody_core::Slot::placements)
            .all(|placement| placement.cause() != Cause::Calling)
    );
}

#[test]
fn an_open_rhyme_holds_until_its_answer() {
    use Cause::{Holding, Repeating, Stated};
    // ABAB: the A line holds under the B line, and the B line under the second A. Each stops in
    // the bar its answer arrives. Every line has been answered by the end, so the outro holds
    // nothing.
    let score = compile("da dA\nda dO\nsa sA\nsa sO\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Holding)],
            vec![(2, Stated), (1, Holding)],
            vec![(3, Stated)],
            vec![(0, Repeating)],
            vec![(1, Repeating)],
            vec![(2, Repeating)],
            vec![(3, Repeating)],
        ]
    );
    let held = score.arrangement().slots()[1].placements()[1];
    assert_eq!(held.lane(), prosody_core::Lane::Held);
    assert_eq!(held.part(), prosody_core::Part::Line);
}

#[test]
fn an_enclosed_rhyme_holds_across_the_pair_inside_it() {
    use Cause::{Holding, Repeating, Stated};
    // ABBA: the B pair answers itself at once, so only the A line is ever held, under both Bs.
    let score = compile("da dA\nda dO\nsa sO\nsa sA\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Holding)],
            vec![(2, Stated), (0, Holding)],
            vec![(3, Stated)],
            vec![(0, Repeating)],
            vec![(1, Repeating)],
            vec![(2, Repeating)],
            vec![(3, Repeating)],
        ]
    );
}

#[test]
fn a_rhyme_answered_only_in_another_stanza_is_not_held() {
    let score = compile("da dA\nda dO\n\nsa sA\nsa sO\n");
    assert!(
        score
            .arrangement()
            .slots()
            .iter()
            .flat_map(prosody_core::Slot::placements)
            .all(|placement| placement.cause() != Cause::Holding)
    );
}

#[test]
fn a_refrain_line_loops_until_the_next_takes_over_and_nothing_doubles() {
    use Cause::{Holding, Refraining, Repeating, Stated};
    // Two refrains, `dO da` (bars 0, 4) and `sO so` (bars 2, 5), and three rhyme pairs: 0–4,
    // 1–3, 2–5. Bar 0 is both a refrain and held; it sounds once, in the refrain lane, until bar 2
    // takes that lane and bar 0 is left to the held lane. A refrain is silent under its own voice.
    let score = compile("dO da\nma mA\nsO so\nna nA\ndO da\nsO so\n");
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Refraining)],
            vec![(2, Stated), (0, Refraining), (1, Holding)],
            vec![(3, Stated), (2, Refraining), (0, Holding)],
            vec![(4, Stated), (2, Refraining)],
            vec![(5, Stated), (4, Refraining)],
            // The outro: the lane ends on `sO so` (bar 5), silent under bars 2 and 5, its voice.
            vec![(0, Repeating), (5, Refraining)],
            vec![(1, Repeating), (5, Refraining)],
            vec![(2, Repeating)],
            vec![(3, Repeating), (5, Refraining)],
            vec![(4, Repeating), (5, Refraining)],
            vec![(5, Repeating)],
        ]
    );
}
