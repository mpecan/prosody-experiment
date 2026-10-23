//! The straight stack, through the real passes: a stacked line fitted to its host's subdivision
//! of a beat where the two would cross (`prosody_core::stack`), and left alone everywhere else.
//!
//! The phonemiser is a toy: each `a` is an unstressed syllable and each `A` a stressed one, `o`
//! and `O` the same with another vowel for a second rhyme, `e` and `E` for a third, every other
//! letter silent — so
//! `da da dA` is an anapaest and `mo mO` an iamb by construction.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use proptest::prelude::{Strategy, prop_assert, prop_assert_eq, proptest};
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Cause, Poem, annotate, meter, repetition, rhyme, role, sound, surprisal};

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
                'e' => Some((ARPABET.phoneme("IH").unwrap(), Stress::Unstressed)),
                'E' => Some((ARPABET.phoneme("EH").unwrap(), Stress::Primary)),
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

fn stacked(stack: Variant) -> Mappings {
    Mappings::all().with(Mapping::Stack.choose(stack).unwrap())
}

/// Every placement of a slot as `(bar, [(beat, index, of, parts)])`, host first, read through
/// `Score::placed_in` — what every backend and the scorer read.
type Onsets = Vec<(usize, usize, usize, usize)>;
fn heard(score: &Score, slot: usize) -> Vec<(usize, Onsets)> {
    let slot = &score.arrangement().slots()[slot];
    slot.placements()
        .iter()
        .map(|placement| {
            let placed = score.placed_in(slot, *placement);
            (
                placement.bar().get(),
                placed
                    .onsets()
                    .map(|(event, at)| (event.beat().get(), at.index(), at.of(), at.parts()))
                    .collect(),
            )
        })
        .collect()
}

/// ABAB, the first A line opening on an anapaest — `da da dA | da dA` — and every other line two
/// iambs: the fixture `prosody-score` measures one crossed beat of sixteen in.
const ABAB: &str = "da da dA da dA\nmo mO mo mO\nta tA ta tA\nso sO so sO\n";

#[test]
fn the_fixture_stacks_the_anapaest_under_an_iamb() {
    // The rest of this file depends on this arrangement, so it checks it. One stanza of period 2:
    // slot 1 hosts the B line with the A line held under it until its answer in slot 2; slot 2
    // hosts the second A line with the B line held; slots 4-7 are the outro, nothing held.
    use Cause::{Holding, Repeating, Stated};
    let score = compile_with(ABAB, &Mappings::all());
    let slots: Vec<Vec<(usize, Cause)>> = score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| {
            slot.placements()
                .iter()
                .map(|p| (p.bar().get(), p.cause()))
                .collect()
        })
        .collect();
    assert_eq!(
        slots,
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
    assert!(score.arrangement().slots().iter().all(|s| s.beats() == 2));
}

#[test]
fn crossing_keeps_every_line_on_its_own_feet() {
    let score = compile_with(ABAB, &stacked(Variant::Crossing));
    assert_eq!(score.stack(), Variant::Crossing);
    // Slot 1: the host's iambs in halves, the held anapaest in thirds — three against two.
    assert_eq!(
        heard(&score, 1),
        vec![
            (
                1,
                vec![(0, 0, 2, 1), (0, 1, 2, 1), (1, 0, 2, 1), (1, 1, 2, 1)]
            ),
            (
                0,
                vec![
                    (0, 0, 3, 1),
                    (0, 1, 3, 1),
                    (0, 2, 3, 1),
                    (1, 0, 2, 1),
                    (1, 1, 2, 1)
                ]
            ),
        ]
    );
    for (index, slot) in score.arrangement().slots().iter().enumerate() {
        for placement in slot.placements() {
            let placed = score.placed_in(slot, *placement);
            assert!(placed.straightened().is_empty(), "slot {index}");
            assert_eq!(placed, score.placed(*placement), "slot {index}");
        }
    }
}

#[test]
fn straight_fits_the_held_triplet_to_the_hosts_pair() {
    let score = compile_with(ABAB, &stacked(Variant::Straight));
    assert_eq!(score.stack(), Variant::Straight);
    // By hand: beat 0 of the held line has three syllables over the host's two halves. The first
    // takes the first half (2 parts of 4), the other two share the second (¼ each): onsets 0, ½,
    // ¾. Beat 1 is an iamb over an iamb and stays. The host is never moved.
    assert_eq!(
        heard(&score, 1),
        vec![
            (
                1,
                vec![(0, 0, 2, 1), (0, 1, 2, 1), (1, 0, 2, 1), (1, 1, 2, 1)]
            ),
            (
                0,
                vec![
                    (0, 0, 4, 2),
                    (0, 2, 4, 1),
                    (0, 3, 4, 1),
                    (1, 0, 2, 1),
                    (1, 1, 2, 1)
                ]
            ),
        ]
    );
    let slot = &score.arrangement().slots()[1];
    let held = slot.placements()[1];
    let fitted = score.placed_in(slot, held);
    assert_eq!(fitted.straightened().len(), 1);
    let only = fitted.straightened()[0];
    assert_eq!((only.beat(), only.own(), only.host()), (0, 3, 2));

    // Every other slot sounds as under crossing: where the anapaest hosts (slots 0 and 4) it has
    // no host to fit to, and slot 2 stacks iamb on iamb.
    let crossing = compile_with(ABAB, &stacked(Variant::Crossing));
    for index in [0, 2, 3, 4, 5, 6, 7] {
        assert_eq!(
            heard(&score, index),
            heard(&crossing, index),
            "slot {index}"
        );
    }
    // The line's own material is untouched: the score's events are the same under both.
    assert_eq!(score.events(), crossing.events());
    assert_eq!(score.arrangement(), crossing.arrangement());
}

#[test]
fn a_loop_squeezed_into_a_shorter_bar_is_left_alone() {
    // The A line now has three feet — `da da dA | da dA | da dA` — and the B lines two.
    let source = "da da dA da dA da dA\nmo mO mo mO\nta tA ta tA da dA\nso sO so sO\n";

    // With the stanza period, every bar lasts three beats: the B host rests in its third, the
    // held A line fills all three on the slot's grid, and its anapaest beat is fitted.
    let with_period = compile_with(source, &stacked(Variant::Straight));
    let slot = &with_period.arrangement().slots()[1];
    assert_eq!(slot.beats(), 3);
    let held = slot.placements()[1];
    assert_eq!(held.bar().get(), 0);
    let fitted: Vec<_> = with_period
        .placed_in(slot, held)
        .straightened()
        .iter()
        .map(|s| (s.beat(), s.own(), s.host()))
        .collect();
    assert_eq!(fitted, vec![(0, 3, 2)]);

    // Without it, slot 1 lasts its host's two beats and the three-beat A line is squeezed in:
    // its beats fall between the host's, so no beat of it is compared, and nothing moves.
    let squeezed = compile_with(
        source,
        &stacked(Variant::Straight).without(Mapping::StanzaPeriod),
    );
    let slot = &squeezed.arrangement().slots()[1];
    assert_eq!(slot.beats(), 2);
    let held = slot.placements()[1];
    assert_eq!(held.bar().get(), 0);
    assert!(!slot.on_grid(3));
    let placed = squeezed.placed_in(slot, held);
    assert!(placed.straightened().is_empty());
    assert_eq!(placed, squeezed.placed(held));
}

/// A poem of the toy's words, so that feet of two and three syllables, rhymes (held lines),
/// repeated lines (refrains) and commas (calls) all occur: every lane that stacks a line.
fn stacking_poem() -> impl Strategy<Value = String> {
    let word = proptest::sample::select(vec!["da", "dA", "ta", "tA,", "mo", "mO", "so", "sO,"]);
    let line = proptest::collection::vec(word, 2..8).prop_map(|words| words.join(" "));
    let stanza = proptest::collection::vec(line, 1..5).prop_map(|lines| lines.join("\n"));
    proptest::collection::vec(stanza, 1..4)
        .prop_map(|stanzas| format!("{}\n", stanzas.join("\n\n")))
}

/// A placement's grid in each of its beats, counted from its first beat: the least common
/// multiple of its onsets' reduced denominators, as `prosody-score`'s `cross_rhythm` reads them.
fn grids(placed: &prosody_core::Placed<'_>) -> std::collections::BTreeMap<usize, usize> {
    const fn gcd(a: usize, b: usize) -> usize {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let mut grids = std::collections::BTreeMap::new();
    for (event, at) in placed.onsets() {
        let denominator = at.of() / gcd(at.index(), at.of()).max(1);
        let grid = grids
            .entry(event.beat().get() - placed.first_beat())
            .or_insert(1);
        *grid = *grid / gcd(*grid, denominator) * denominator;
    }
    grids
}

proptest! {
    /// The straight stack moves nothing but where a stacked syllable falls inside its beat:
    /// under any choice of the other mappings the score's events, voices, arrangement and
    /// harmony are crossing's; every placement sounds the same events in the same beats, each
    /// at an instant of its own (I4); the host never moves; a beat it did not fit is as crossing
    /// plays it; and wherever a stacked line and its host share a beat on the slot's grid, the two
    /// no longer cross.
    #[test]
    fn straight_only_moves_stacked_onsets_onto_the_hosts_grid(
        source in stacking_poem(),
        pick in 0_usize..1 << 16,
    ) {
        let every = Mappings::every();
        let mappings = every[pick % every.len()].clone();
        let choose = |variant| mappings.clone().with(Mapping::Stack.choose(variant).unwrap());
        let crossing = compile_with(&source, &choose(Variant::Crossing));
        let straight = compile_with(&source, &choose(Variant::Straight));
        prop_assert_eq!(crossing.events(), straight.events());
        prop_assert_eq!(crossing.voices(), straight.voices());
        prop_assert_eq!(crossing.arrangement(), straight.arrangement());
        prop_assert_eq!(crossing.harmony(), straight.harmony());

        for slot in straight.arrangement().slots() {
            let host = slot.host();
            let host_grids = host
                .map(|host| grids(&straight.placed_in(slot, host)))
                .unwrap_or_default();
            for placement in slot.placements() {
                let before = crossing.placed_in(slot, *placement);
                let after = straight.placed_in(slot, *placement);
                prop_assert_eq!(before.events(), after.events());
                prop_assert!(before.straightened().is_empty());
                if Some(*placement) == host {
                    prop_assert_eq!(&before, &after);
                }
                // Each onset inside its beat, and no two of one beat at the same instant.
                let mut seen = std::collections::BTreeSet::new();
                for (event, at) in after.onsets() {
                    prop_assert!(at.parts() >= 1 && at.index() + at.parts() <= at.of());
                    let instant = (event.beat().get(), at.index() * 2520 / at.of());
                    prop_assert!(seen.insert(instant), "two syllables at {:?}", instant);
                }
                let fitted: std::collections::BTreeSet<usize> =
                    after.straightened().iter().map(|s| s.beat()).collect();
                for ((event, was), (_, now)) in before.onsets().zip(after.onsets()) {
                    if !fitted.contains(&(event.beat().get() - after.first_beat())) {
                        prop_assert_eq!(was, now);
                    }
                }
                let on_grid = host.is_some_and(|host| {
                    *placement != host
                        && slot.on_grid(after.beats())
                        && slot.on_grid(straight.placed(host).beats())
                });
                if on_grid {
                    for (beat, grid) in grids(&after) {
                        if let Some(&with) = host_grids.get(&beat) {
                            prop_assert!(
                                grid % with == 0 || with % grid == 0,
                                "beat {} still crosses: {} against the host's {}",
                                beat, grid, with
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn the_explain_view_names_the_straight_stack_and_each_fitted_beat() {
    let view = |stack| {
        let poem = Poem::parse(ABAB.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Toy);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let score = compile_with(ABAB, &stacked(stack));
        annotate::render(&poem, &annotate::run(&poem, &score, &roles))
    };
    let crossing = view(Variant::Crossing);
    let straight = view(Variant::Straight);
    // The default says nothing new, as the height bass does not.
    assert!(!crossing.contains("stack"), "{crossing}");
    assert!(!crossing.contains("  straight "), "{crossing}");
    assert!(
        straight.contains("stack straight · a line stacked under a bar's line is fitted to its subdivision where the two would cross: 1 beat(s) (§4.2, §4.6)"),
        "{straight}"
    );
    // Under the A line only: its beat 0 in slot 1, three into two.
    let fitted: Vec<&str> = straight
        .lines()
        .filter(|l| l.starts_with("  straight "))
        .collect();
    assert_eq!(
        fitted,
        vec!["  straight 3 into 2 at 1.0 — fitted to the bar's line (slot.beat)"]
    );
    let a_line = straight.find("da da dA da dA").unwrap();
    let b_line = straight.find("mo mO mo mO").unwrap();
    let at = straight.find("  straight 3 into 2").unwrap();
    assert!(a_line < at && at < b_line, "{straight}");
    // Everything else in the view is as under crossing.
    let rest: Vec<&str> = straight
        .lines()
        .filter(|l| !l.starts_with("stack straight") && !l.starts_with("  straight "))
        .collect();
    assert_eq!(rest, crossing.lines().collect::<Vec<_>>());
}

/// ABCABC, the A and B lines opening on an anapaest, every other line two iambs: by hand, slot 2
/// hosts the C line (a pair in beat 0) with both A and B held under it, each a triplet there; slot
/// 3 hosts the second A line (a pair) with B (a triplet) and C (a pair) held.
const ABCABC: &str =
    "da da dA da dA\nmo mo mO mo mO\nte tE te tE\nsa sA sa sA\nlo lO lo lO\nre rE re rE\n";

/// A placement's fitted beats as `(beat, own grid, host grid)`.
type Fitted = Vec<(usize, usize, usize)>;

#[test]
fn two_lines_fitted_in_one_beat_count_it_once_in_every_view() {
    let score = compile_with(ABCABC, &stacked(Variant::Straight));
    let fitted = |slot: usize| -> Vec<(usize, Fitted)> {
        let slot = &score.arrangement().slots()[slot];
        slot.placements()
            .iter()
            .map(|placement| {
                (
                    placement.bar().get(),
                    score
                        .placed_in(slot, *placement)
                        .straightened()
                        .iter()
                        .map(|at| (at.beat(), at.own(), at.host()))
                        .collect(),
                )
            })
            .collect()
    };
    // The premise: two lines fitted three into two in slot 2's beat 0, one in slot 3's.
    assert_eq!(
        fitted(2),
        vec![(2, vec![]), (0, vec![(0, 3, 2)]), (1, vec![(0, 3, 2)])]
    );
    assert_eq!(
        fitted(3),
        vec![(3, vec![]), (1, vec![(0, 3, 2)]), (2, vec![])]
    );
    // Three fitted placements, but two beats of the piece: 2.0 and 3.0.
    assert_eq!(score.straightened_beats(), 2);
    let poem = Poem::parse(ABCABC.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Toy);
    let roles = role::assign(&meter, &repetition::analyse(&poem)).unwrap();
    let annotated = annotate::run(&poem, &score, &roles);
    assert_eq!(annotated.straightened_beats(), 2);
    let view = annotate::render(&poem, &annotated);
    assert!(
        view.contains("where the two would cross: 2 beat(s)"),
        "{view}"
    );
    // Each line still lists its own fitted beats: A at 2.0, B at 2.0 and 3.0.
    let listed: Vec<&str> = view
        .lines()
        .filter(|l| l.starts_with("  straight "))
        .collect();
    assert_eq!(
        listed,
        vec![
            "  straight 3 into 2 at 2.0 — fitted to the bar's line (slot.beat)",
            "  straight 3 into 2 at 2.0 3.0 — fitted to the bar's line (slot.beat)",
        ]
    );
}
