//! The arrangement rule, through the real passes: stanzas loop until a competing stanza transforms
//! them or a refrain stanza overrules them.
//!
//! The phonemiser is a toy: each `a` is an unstressed syllable and each `A` a stressed one, other
//! letters silent; `o` and `O` are the same with another vowel, for a second rhyme. Lines of equal length make a regular stanza (foundation, `SPEC.md` §4.1); a
//! one-syllable line beside a six-syllable one makes an irregular stanza (flourish).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings};
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
    // No closed class: every word keeps its dictionary stress under either variant.
    fn is_function_word(&self, _: &str) -> bool {
        false
    }

    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }
}

fn compile(source: &str) -> Score {
    compile_with(source, &Mappings::all())
}

fn compile_with(source: &str, mappings: &Mappings) -> Score {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Toy);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &Toy),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &Toy),
            surprisal: &surprisal::analyse(&poem, &Toy, &Flat),
            roles: &roles,
        },
        mappings,
    )
    .unwrap()
}

fn without(mapping: Mapping) -> Mappings {
    Mappings::all().without(mapping)
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

// Each candidate mapping switched off on its own (`prosody_core::mapping`): the piece falls back to
// what it was before that mapping was added, and nothing else moves.

#[test]
fn without_the_held_lane_an_open_rhyme_is_not_held() {
    use Cause::{Repeating, Stated};
    // The ABAB of `an_open_rhyme_holds_until_its_answer`, with the lane gone: each line alone.
    let score = compile_with("da dA\nda dO\nsa sA\nsa sO\n", &without(Mapping::HeldLane));
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated)],
            vec![(2, Stated)],
            vec![(3, Stated)],
            vec![(0, Repeating)],
            vec![(1, Repeating)],
            vec![(2, Repeating)],
            vec![(3, Repeating)],
        ]
    );
}

#[test]
fn without_the_refrain_lane_a_refrain_is_only_held() {
    use Cause::{Holding, Repeating, Stated};
    // The fixture of `a_refrain_line_loops_until_the_next_takes_over_and_nothing_doubles`. Rhyme
    // pairs 0–4, 1–3 and 2–5 still hold, now with nothing in the refrain lane to stand in for
    // bar 0: it holds from bar 1, joined by bar 1 under bar 2 and bar 2 under bar 3, where bar 1
    // is answered. Bar 4 answers bar 0 and bar 5 answers bar 2, so the outro holds nothing.
    let score = compile_with(
        "dO da\nma mA\nsO so\nna nA\ndO da\nsO so\n",
        &without(Mapping::RefrainLane),
    );
    assert_eq!(
        slots(&score),
        vec![
            vec![(0, Stated)],
            vec![(1, Stated), (0, Holding)],
            vec![(2, Stated), (0, Holding), (1, Holding)],
            vec![(3, Stated), (0, Holding), (2, Holding)],
            vec![(4, Stated), (2, Holding)],
            vec![(5, Stated)],
            vec![(0, Repeating)],
            vec![(1, Repeating)],
            vec![(2, Repeating)],
            vec![(3, Repeating)],
            vec![(4, Repeating)],
            vec![(5, Repeating)],
        ]
    );
}

#[test]
fn without_the_bass_only_rhyme_words_are_pitched() {
    // `da dA` alone rhymes with nothing, so its only note is the bass on the stressed `dA`.
    let pitched = |score: &Score| {
        score
            .events()
            .iter()
            .filter(|e| e.pitch().is_some() || e.bass().is_some())
            .count()
    };
    assert_eq!(pitched(&compile("da dA\n")), 1);
    assert_eq!(
        pitched(&compile_with("da dA\n", &without(Mapping::Bass))),
        0
    );
    // A rhyme keeps its note: `dA` and `sA` rhyme, so each line's last word is pitched.
    let rhymed = compile_with("da dA\nsa sA\n", &without(Mapping::Bass));
    assert_eq!(pitched(&rhymed), 2);
}

#[test]
fn without_the_stanza_period_a_bar_lasts_its_own_line() {
    // `THREE`'s stanza 2 is `ta` (one beat) and `ma mA ma mA ma mA` (three). Each bar now takes
    // its host's length, so `ta`'s bar is one beat — and stanza 1's loop under it, a one-beat
    // line, fills it. Under the three-beat bar the same one-beat loop is stretched across all
    // three, rather than playing one and resting for two.
    let score = compile_with(THREE, &without(Mapping::StanzaPeriod));
    let slots = score.arrangement().slots();
    let beats: Vec<usize> = slots.iter().map(prosody_core::Slot::beats).collect();
    assert_eq!(beats[..4], [1, 1, 1, 3]);
    assert_eq!(slots[3].room_for(1), 3);
    assert_eq!(slots[3].room_for(5), 3);
}

#[test]
fn without_the_outro_the_piece_ends_with_the_poem() {
    let full = compile(THREE);
    let score = compile_with(THREE, &without(Mapping::Outro));
    let ended = slots(&score);
    // `THREE`'s last stanza has two lines, so the outro was its last two slots.
    assert_eq!(ended, slots(&full)[..slots(&full).len() - 2]);
    assert!(
        ended
            .iter()
            .flatten()
            .all(|(_, cause)| *cause != Cause::Repeating)
    );
}

/// Stanza 1: two regular lines. Stanza 2: nine — a long stanza (more than
/// [`prosody_core::arrange::LONG_STANZA`] lines). Every line is four syllables; line 1 (bar 1)
/// ends in the two-syllable word `gAga`, so its rhyme word outlasts a stressed syllable before it.
const SHORT_THEN_LONG: &str = "ba bA ba bA\nga gA gAga\n\n\
    ca cA ca cA\nda dA da dA\nfa fA fa fA\nha hA ha hA\nja jA ja jA\n\
    ka kA ka kA\nla lA la lA\nma mA ma mA\nna nA na nA\n";

/// Nine regular lines, then two.
const LONG_THEN_SHORT: &str = "ba bA ba bA\nca cA ca cA\nda dA da dA\nfa fA fa fA\n\
    ga gA gAga\nha hA ha hA\nja jA ja jA\nka kA ka kA\nla lA la lA\n\nma mA ma mA\nna nA na nA\n";

/// The lanes and the outro left out, so a slot is only its layer's lines: the erosion alone.
fn layers_only(long_stanzas: &str) -> Mappings {
    Mappings::all()
        .without(Mapping::HeldLane)
        .without(Mapping::RefrainLane)
        .without(Mapping::Outro)
        .with(Mapping::LongStanzas.choose_named(long_stanzas).unwrap())
}

/// Which of its line's syllables a placement sounds, counted from 0 in playing order.
fn kept(score: &Score, slot: usize, bar: usize) -> Vec<usize> {
    let slot = &score.arrangement().slots()[slot];
    let placement = slot
        .placements()
        .iter()
        .find(|placement| placement.bar().get() == bar)
        .unwrap();
    let line: Vec<_> = score
        .events()
        .iter()
        .filter(|event| event.bar().get() == bar)
        .collect();
    score
        .placed_in(slot, *placement)
        .events()
        .iter()
        .map(|event| line.iter().position(|own| own == event).unwrap())
        .collect()
}

#[test]
fn the_erosion_fixtures_are_one_layer() {
    // Both stanzas of each fixture must compete for one layer, or nothing arrives into a loop.
    for source in [SHORT_THEN_LONG, LONG_THEN_SHORT] {
        let score = compile(source);
        for event in score.events() {
            assert_eq!(
                score.voice(event.voice()).unwrap().layer(),
                Layer::Foundation
            );
        }
    }
}

#[test]
fn a_long_loop_erodes_under_a_short_arrival() {
    use Cause::{Arriving, Eroding, Stated};
    // The nine-line loop is long, so stanza 2 does not replace it a line a pass: it is stated
    // whole three times, and the old loop runs on under it from its first line, one line a bar,
    // pass 1 under the first statement, 2 under the second, 3 under the third — then it is gone.
    let score = compile_with(LONG_THEN_SHORT, &layers_only("erode"));
    let mut expected: Vec<Vec<(usize, Cause)>> = (0..9).map(|bar| vec![(bar, Stated)]).collect();
    expected.extend([
        vec![(9, Arriving { pass: 1 }), (0, Eroding { pass: 1 })],
        vec![(10, Arriving { pass: 1 }), (1, Eroding { pass: 1 })],
        vec![(9, Arriving { pass: 2 }), (2, Eroding { pass: 2 })],
        vec![(10, Arriving { pass: 2 }), (3, Eroding { pass: 2 })],
        vec![(9, Arriving { pass: 3 }), (4, Eroding { pass: 3 })],
        vec![(10, Arriving { pass: 3 }), (5, Eroding { pass: 3 })],
    ]);
    assert_eq!(slots(&score), expected);
}

#[test]
fn gradual_replaces_even_a_long_loop_a_line_a_pass() {
    use Cause::{Arriving, Stated, Yielding};
    // Today's rule: pass 1 is stanza 2's first line and the old loop's lines 2–9; pass 2 is
    // stanza 2 alone. 9 + 9 + 2 = 20 bars where erosion takes 9 + 6 = 15.
    let score = compile_with(LONG_THEN_SHORT, &layers_only("gradual"));
    let mut expected: Vec<Vec<(usize, Cause)>> = (0..9).map(|bar| vec![(bar, Stated)]).collect();
    expected.push(vec![(9, Arriving { pass: 1 })]);
    expected.extend((1..9).map(|bar| vec![(bar, Yielding { pass: 1 })]));
    expected.push(vec![(9, Arriving { pass: 2 })]);
    expected.push(vec![(10, Arriving { pass: 2 })]);
    assert_eq!(slots(&score), expected);
}

#[test]
fn a_short_loop_erodes_under_a_long_arrival() {
    use Cause::{Arriving, Eroding, Stated};
    // Stanza 2 is long, so the two-line loop erodes under it: 3 × 9 = 27 bars, the old loop
    // alternating its two lines all the way through, line 0 under the first bar of each odd
    // statement, line 1 under the first bar of the second.
    let score = compile_with(SHORT_THEN_LONG, &layers_only("erode"));
    let got = slots(&score);
    assert_eq!(got.len(), 2 + 27);
    assert_eq!(got[0], vec![(0, Stated)]);
    assert_eq!(got[1], vec![(1, Stated)]);
    for (index, slot) in got[2..].iter().enumerate() {
        let pass = index / 9 + 1;
        assert_eq!(
            slot,
            &vec![
                (2 + index % 9, Arriving { pass }),
                (index % 2, Eroding { pass })
            ],
            "slot {}",
            index + 2
        );
    }
}

#[test]
fn an_eroding_line_loses_unstressed_syllables_first_and_its_rhyme_word_last() {
    // Worked by hand, with k syllables kept to ⌈k · (4 − p) / 4⌉ on pass p.
    //
    // `ba bA ba bA` — unstressed, stressed, unstressed, stressed rhyme word. Four syllables:
    // pass 1 keeps 3 (drops the first unstressed), pass 2 keeps 2 (both unstressed gone),
    // pass 3 keeps 1 — the rhyme word `bA`.
    //
    // `ga gA gAga` — unstressed, stressed, then the rhyme word's stressed and unstressed
    // syllables. Pass 1 keeps 3 (drops `ga`); pass 2 keeps 2 — the rhyme word whole, which
    // outlasts the stressed `gA` before it; pass 3 keeps 1, the rhyme word's stressed syllable.
    let score = compile_with(SHORT_THEN_LONG, &layers_only("erode"));
    // Line 0 erodes in slots 2 (pass 1), 12 (pass 2) and 20 (pass 3): it is under every
    // other bar, and the statements are nine bars long.
    assert_eq!(kept(&score, 2, 0), vec![1, 2, 3]);
    assert_eq!(kept(&score, 12, 0), vec![1, 3]);
    assert_eq!(kept(&score, 20, 0), vec![3]);
    // Line 1: slots 3, 11 and 21.
    assert_eq!(kept(&score, 3, 1), vec![1, 2, 3]);
    assert_eq!(kept(&score, 11, 1), vec![2, 3]);
    assert_eq!(kept(&score, 21, 1), vec![2]);
    // The arriving stanza is never eroded: slot 20's host — stanza 2's first line, bar 2 —
    // sounds every syllable.
    assert_eq!(kept(&score, 20, 2), vec![0, 1, 2, 3]);
    // An eroding line keeps its whole line's beats, so what is left sounds where it always did.
    let slot = &score.arrangement().slots()[20];
    let eroding = slot.placements()[1];
    assert_eq!(score.placed(eroding).beats(), score.beats_in(0));
    assert_eq!(score.placed(eroding).first_beat(), 0);
}

#[test]
fn eight_lines_are_not_long() {
    // Eight-line stanzas on both sides are below the threshold: transformed a line a pass under
    // both variants, exactly as before the erosion existed.
    let eight = |letters: &str| -> String {
        letters
            .chars()
            .map(|c| format!("{c}a {c}A {c}a {c}A"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    };
    let source = format!("{}\n{}", eight("bcdfghjk"), eight("lmnprstv"));
    let erode = compile_with(&source, &layers_only("erode"));
    let gradual = compile_with(&source, &layers_only("gradual"));
    assert_eq!(erode.arrangement(), gradual.arrangement());
    assert!(
        slots(&erode)
            .iter()
            .flatten()
            .any(|(_, cause)| matches!(cause, Cause::Yielding { .. }))
    );
    assert!(
        slots(&erode)
            .iter()
            .flatten()
            .all(|(_, cause)| !matches!(cause, Cause::Eroding { .. }))
    );
    // One more line in the second stanza makes it long, and it erodes.
    let source = format!("{}\n{}", eight("bcdfghjk"), eight("lmnprstvw"));
    let erode = compile_with(&source, &layers_only("erode"));
    assert!(
        slots(&erode)
            .iter()
            .flatten()
            .any(|(_, cause)| matches!(cause, Cause::Eroding { .. }))
    );
}
