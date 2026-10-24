//! Erosion for long stanzas (`SPEC.md` §4.6), as properties over random poems with long stanzas:
//! what an eroding line keeps is a deterministic, monotone thinning of its own events, ordered
//! unstressed first and rhyme word last (I1, I4); it reads nothing outside the line (I2); and a
//! poem with no stanza longer than eight lines arranges exactly as it did before (the threshold).
//!
//! The phonemiser is a toy: `a` an unstressed syllable, `A` a stressed one, `o`/`O` the same with
//! another vowel; other letters silent.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use std::collections::BTreeMap;

use proptest::prelude::*;

use prosody_core::arrange::{Cause, EROSION_PASSES, LONG_STANZA};
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::{Event, Score};
use prosody_core::{Poem, meter, repetition, rhyme, role, sound, surprisal};

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
    // Two-letter words are function words, so the light variant is exercised too.
    fn is_function_word(&self, word: &str) -> bool {
        word.len() <= 2
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

/// A word of the toy alphabet: one to three syllables, stressed or not.
fn word() -> impl Strategy<Value = String> {
    proptest::sample::select(vec![
        "ba", "bA", "do", "dO", "kaba", "kAba", "kabA", "mOda", "tA", "sa", "lo", "rAbo", "pabAda",
    ])
    .prop_map(str::to_owned)
}

/// A line of two to five words.
fn line() -> impl Strategy<Value = String> {
    proptest::collection::vec(word(), 2..6).prop_map(|words| words.join(" "))
}

/// A poem of two or three stanzas of one to `most` lines each.
fn poem(most: usize) -> impl Strategy<Value = String> {
    let stanza = proptest::collection::vec(line(), 1..=most).prop_map(|lines| lines.join("\n"));
    proptest::collection::vec(stanza, 2..4)
        .prop_map(|stanzas| format!("{}\n", stanzas.join("\n\n")))
}

/// Any way of choosing the mappings but the long stanzas, which `with` sets.
fn any_mappings(pick: usize, long_stanzas: Variant) -> Mappings {
    let every = Mappings::every();
    every[pick % every.len()]
        .clone()
        .with(Mapping::LongStanzas.choose(long_stanzas).unwrap())
}

/// A line's own events, in playing order.
fn line_events(score: &Score, bar: usize) -> Vec<&Event> {
    score
        .events()
        .iter()
        .filter(|event| event.bar().get() == bar)
        .collect()
}

/// Every eroding placement: (slot, bar, pass, which of its line's syllables it keeps, by index).
fn erosions(score: &Score) -> Vec<(usize, usize, usize, Vec<usize>)> {
    let mut found = Vec::new();
    for (index, slot) in score.arrangement().slots().iter().enumerate() {
        for placement in slot.placements() {
            if let Cause::Eroding { pass } = placement.cause() {
                let bar = placement.bar().get();
                let line = line_events(score, bar);
                let kept = score
                    .placed_in(slot, *placement)
                    .events()
                    .iter()
                    .map(|event| line.iter().position(|own| own == event).unwrap())
                    .collect();
                found.push((index, bar, pass, kept));
            }
        }
    }
    found
}

/// Which of a line's syllables survive each pass, wherever the line erodes, by the line's text.
fn kept_by_text(source: &str, score: &Score) -> BTreeMap<(String, usize), Vec<usize>> {
    erosions(score)
        .into_iter()
        .map(|(_, bar, pass, kept)| {
            let first = line_events(score, bar)[0].span();
            let last = line_events(score, bar).last().unwrap().span();
            let text = source[first.start()..last.end()].to_owned();
            ((text, pass), kept)
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// On pass *p* an eroding line of *k* syllables sounds exactly ⌈k · (4 − p) / 4⌉ of them, a
    /// subset of what it sounded on the pass before; every one it keeps outranks every one it
    /// lost — (in the rhyme word, force, position), lowest first; and each is an event of its own
    /// line, carrying a span that resolves in the source (I4).
    #[test]
    fn an_eroding_line_thins_out_by_its_own_rule(
        source in poem(12),
        pick in 0_usize..1 << 16,
    ) {
        let mappings = any_mappings(pick, Variant::Erode);
        let score = compile_with(&source, &mappings);
        let mut by_line: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (slot, bar, pass, kept) in erosions(&score) {
            prop_assert!((1..=EROSION_PASSES).contains(&pass));
            let line = line_events(&score, bar);
            let total = line.len();
            prop_assert_eq!(kept.len(), (total * (4 - pass)).div_ceil(4), "slot {}", slot);
            let rank = |index: usize| {
                let event = line[index];
                (event.span() == line.last().unwrap().span(), event.velocity(), index)
            };
            let lowest_kept = kept.iter().map(|&index| rank(index)).min();
            for lost in (0..total).filter(|index| !kept.contains(index)) {
                prop_assert!(Some(rank(lost)) < lowest_kept, "slot {slot}: {lost} outranks");
            }
            for &index in &kept {
                let span = line[index].span();
                prop_assert!(span.text(&source).is_some());
            }
            if let Some(before) = by_line.get(&(bar, pass - 1)) {
                prop_assert!(kept.iter().all(|index| before.contains(index)));
            }
            by_line.insert((bar, pass), kept);
        }
    }

    /// I1: the same poem under the same choices erodes the same way, byte for byte.
    #[test]
    fn i1_erosion_is_reproducible(source in poem(12), pick in 0_usize..1 << 16) {
        let mappings = any_mappings(pick, Variant::Erode);
        let one = compile_with(&source, &mappings);
        let two = compile_with(&source, &mappings);
        prop_assert_eq!(one.arrangement(), two.arrangement());
        prop_assert_eq!(erosions(&one), erosions(&two));
    }

    /// The threshold: a poem none of whose stanzas has more than eight lines is arranged — and
    /// sounds — exactly as before erosion existed, under every other choice.
    #[test]
    fn short_stanzas_arrange_as_before(
        source in poem(LONG_STANZA),
        pick in 0_usize..1 << 16,
    ) {
        let erode = compile_with(&source, &any_mappings(pick, Variant::Erode));
        let gradual = compile_with(&source, &any_mappings(pick, Variant::Gradual));
        prop_assert_eq!(erode, gradual);
    }

    /// Bars grow linearly: under erosion an arrival into a loop where either stanza is long takes
    /// exactly three statements of the arriving stanza — every bar of them over the eroding loop,
    /// on the host's own pass — and then the old loop is gone.
    #[test]
    fn a_long_arrival_takes_three_statements(source in poem(14), pick in 0_usize..1 << 16) {
        let score = compile_with(&source, &any_mappings(pick, Variant::Erode));
        let poem = Poem::parse(source.clone()).unwrap();
        let mut stanza_of = Vec::new();
        for (index, stanza) in poem.stanzas().iter().enumerate() {
            stanza_of.extend(std::iter::repeat_n(index, stanza.lines().len()));
        }
        let mut over: BTreeMap<usize, usize> = BTreeMap::new();
        for (slot, _, pass, _) in erosions(&score) {
            let host = score.arrangement().slots()[slot].host().unwrap();
            prop_assert_eq!(host.cause(), Cause::Arriving { pass });
            *over.entry(stanza_of[host.bar().get()]).or_default() += 1;
        }
        for (stanza, bars) in over {
            let lines = poem.stanzas()[stanza].lines().len();
            prop_assert_eq!(bars, EROSION_PASSES * lines, "stanza {}", stanza);
        }
    }

    /// I2: what a line keeps on each pass reads only the line. Editing a word of another stanza
    /// leaves every line of stanza `kept` that erodes in both poems eroding the same way — the
    /// same syllables on the same pass. (Which of its lines sound under which pass, and whether
    /// it erodes at all, can move: they follow the arriving stanza's length — disclosed, SPEC.md
    /// §4.6.)
    #[test]
    fn i2_an_edit_elsewhere_does_not_change_what_a_line_keeps(
        source in poem(12),
        pick in 0_usize..1 << 16,
        replacement in word(),
    ) {
        let mappings = any_mappings(pick, Variant::Erode);
        let mut poem = Poem::parse(source.clone()).unwrap();
        let stanzas = poem.stanzas().len();
        let target = pick % stanzas;
        let lines = poem.stanzas()[target].lines();
        let line = (pick / 7) % lines.len();
        let tokens = lines[line].tokens();
        let span = tokens[(pick / 13) % tokens.len()].span();
        prop_assume!(span.text(poem.source()) != Some(replacement.as_str()));
        poem.apply(span, &replacement).unwrap();
        prop_assume!(poem.stanzas().len() == stanzas);
        let edited = poem.source().to_owned();

        let before = compile_with(&source, &mappings);
        let after = compile_with(&edited, &mappings);
        let before_kept = kept_by_text(&source, &before);
        let after_kept = kept_by_text(&edited, &after);
        let edited_text = Poem::parse(edited.clone()).unwrap().stanzas()[target]
            .span()
            .text(&edited)
            .unwrap()
            .to_owned();
        for ((text, pass), kept) in &before_kept {
            if edited_text.contains(text.as_str()) {
                continue;
            }
            if let Some(again) = after_kept.get(&(text.clone(), *pass)) {
                prop_assert_eq!(kept, again, "{} on pass {}", text, pass);
            }
        }
    }
}

#[test]
fn the_generators_reach_erosion() {
    // The properties are vacuous if no generated poem erodes: count how many of the generator's
    // first poems (a fixed seed, so the count is stable) erode at least one line.
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    let mut runner = TestRunner::deterministic();
    let strategy = poem(12);
    let eroding = (0..64)
        .filter(|_| {
            let source = strategy.new_tree(&mut runner).unwrap().current();
            !erosions(&compile_with(&source, &Mappings::all())).is_empty()
        })
        .count();
    assert!(eroding >= 8, "only {eroding} of 64 generated poems erode");
}
