//! Erosion for long stanzas (`SPEC.md` §4.6) through the real dictionary into every consumer:
//! Lowell's "Patterns", whose stanzas of 9 to 22 lines erode the loops they arrive into. In every
//! slot of the piece the Strudel drum lanes strike, the trace lights and the OSC schedule sends
//! exactly the syllables the arrangement's placements sound there — so a syllable an eroding loop
//! has lost is absent from all three, and every one that sounds keeps its span (I4). The reading
//! reads each line once, never where its loop is eroding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // test code

use std::collections::BTreeMap;

use prosody_cmudict::CmuDict;
use prosody_core::arrange::Cause;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::Score;
use prosody_core::{
    Fallback, Mapping, Mappings, Poem, Span, Variant, meter, repetition, rhyme, role, sound,
    surprisal,
};
use prosody_emit::{Backend as _, Osc, Strudel, Style, reading, trace};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

const PATTERNS: &str = include_str!("../../../examples/lowell-patterns.poem");

fn compile(source: &str, long_stanzas: Variant) -> (Poem, Score, Style) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let meter = meter::scan(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    let score = lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &phonemiser),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &phonemiser),
            surprisal: &surprisal::analyse(&poem, &phonemiser, &EnglishLexicon::new()),
            roles: &roles,
        },
        &Mappings::all().with(Mapping::LongStanzas.choose(long_stanzas).unwrap()),
    )
    .unwrap();
    (poem, score, style)
}

/// One level of a mini-notation sequence, split at its top-level spaces.
fn items(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    for (index, c) in inner.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ' ' if depth == 0 => {
                out.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

/// `element@weight`, or `None` when the item carries no weight of its own.
fn weighted(item: &str) -> Option<(&str, u64)> {
    let (element, weight) = item.rsplit_once('@')?;
    Some((element, weight.parse().ok()?))
}

/// A group of bars (`strudel::nested`): bracketed, of more than one item, every item weighted,
/// the weights summing to the group's own.
fn group(element: &str, weight: u64) -> Option<Vec<(&str, u64)>> {
    let inner = element.strip_prefix('[')?.strip_suffix(']')?;
    let level: Vec<(&str, u64)> = items(inner)
        .into_iter()
        .map(weighted)
        .collect::<Option<_>>()?;
    (level.len() > 1 && level.iter().map(|(_, w)| w).sum::<u64>() == weight).then_some(level)
}

/// The elements of a `"<a@n b@n …>"` sequence, one per slot of the piece, whose slots last
/// `weights` cycles: a group of bars (`[a@3 b@4]@7`) is opened, and a run of rests (`~@7`) is
/// split back into the slots it spans.
fn elements<'a>(sequence: &'a str, weights: &[u64]) -> Vec<&'a str> {
    fn walk<'a>(level: Vec<(&'a str, u64)>, weights: &[u64], out: &mut Vec<&'a str>) {
        for (element, weight) in level {
            if element == "~" {
                let mut total = 0;
                while total < weight {
                    total += weights[out.len()];
                    out.push("~");
                }
                assert_eq!(total, weight, "a run of rests ends inside a slot");
            } else if let Some(inner) = group(element, weight) {
                walk(inner, weights, out);
            } else {
                assert_eq!(weights[out.len()], weight, "{element}");
                out.push(element);
            }
        }
    }
    let inner = sequence
        .strip_prefix('<')
        .and_then(|rest| rest.strip_suffix('>'))
        .unwrap();
    let top: Vec<(&str, u64)> = items(inner)
        .into_iter()
        .map(|i| weighted(i).unwrap())
        .collect();
    // What makes the piece playable: Strudel visits every element of a level on every query.
    assert!(top.len() <= 16, "{} elements at the top", top.len());
    let mut out = Vec::new();
    walk(top, weights, &mut out);
    assert_eq!(out.len(), weights.len());
    out
}

/// The values one element strikes, rests and weights left out.
fn struck(element: &str) -> Vec<&str> {
    element
        .split(|c: char| c == '[' || c == ']' || c.is_whitespace())
        .map(|token| token.split('@').next().unwrap())
        .filter(|token| !token.is_empty() && *token != "~")
        .collect()
}

/// The first quoted argument of every row starting with `call`.
fn sequences<'a>(pattern: &'a str, call: &str) -> Vec<&'a str> {
    pattern
        .lines()
        .filter_map(|row| row.trim_start().strip_prefix(call))
        .map(|rest| &rest[..rest.find('"').unwrap()])
        .collect()
}

/// What each slot sounds by the arrangement: the spans of its placements' events, sorted.
fn sounded(score: &Score) -> Vec<Vec<Span>> {
    score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| {
            let mut spans: Vec<Span> = slot
                .placements()
                .iter()
                .flat_map(|placement| {
                    score
                        .placed_in(slot, *placement)
                        .events()
                        .iter()
                        .map(|event| event.span())
                        .collect::<Vec<_>>()
                })
                .collect();
            spans.sort();
            spans
        })
        .collect()
}

#[test]
fn patterns_erodes_and_stays_linear() {
    let (_, eroding, _) = compile(PATTERNS, Variant::Erode);
    let (_, gradual, _) = compile(PATTERNS, Variant::Gradual);
    // Stanzas of 9, 18, 15, 16, 22, 10 and 17 lines. Gradual: 1,331 bars (PLAN.md, "Eight more
    // poems"). Eroding: whatever each arrival takes, three statements of a long stanza at most.
    assert_eq!(gradual.arrangement().slots().len(), 1331);
    let bars = eroding.arrangement().slots().len();
    assert!(bars <= 3 * 107 + 17, "{bars}");
    let causes: Vec<Cause> = eroding
        .arrangement()
        .slots()
        .iter()
        .flat_map(|slot| slot.placements().iter().map(|p| p.cause()))
        .collect();
    assert!(causes.iter().any(|c| matches!(c, Cause::Eroding { .. })));
    assert!(causes.iter().all(|c| !matches!(c, Cause::Yielding { .. })));
    // Somewhere a loop has lost syllables: what the slots sound is less than whole lines.
    let whole: usize = eroding
        .arrangement()
        .slots()
        .iter()
        .flat_map(prosody_core::Slot::placements)
        .map(|placement| {
            eroding
                .events()
                .iter()
                .filter(|event| event.bar() == placement.bar())
                .count()
        })
        .sum();
    let heard: usize = sounded(&eroding).iter().map(Vec::len).sum();
    assert!(heard < whole, "{heard} of {whole}");
}

#[test]
fn every_consumer_sounds_what_the_arrangement_sounds_in_every_slot() {
    let (_, score, style) = compile(PATTERNS, Variant::Erode);
    let expected = sounded(&score);
    let slots = expected.len();
    // Each slot lasts its beats under the steady pulse (`strudel::weights`).
    let weights: Vec<u64> = score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| u64::try_from(slot.beats().max(1)).unwrap())
        .collect();

    // The Strudel drum lanes: each sounding syllable is one drum stroke, in its slot's element.
    let pattern = Strudel::new().render(&score, &style).unwrap();
    let mut drums = vec![0_usize; slots];
    for sequence in sequences(&pattern, "s(\"") {
        let elements = elements(sequence, &weights);
        for (slot, element) in elements.iter().enumerate() {
            drums[slot] += struck(element).len();
        }
    }
    let counts: Vec<usize> = expected.iter().map(Vec::len).collect();
    assert_eq!(drums, counts);

    // The trace: each value is a mark, and its span is the syllable's word — slot by slot, the
    // same words the arrangement sounds there, so an eroded syllable is never lit.
    let traced = trace::plan(&score, &style);
    let mut lit = vec![Vec::new(); slots];
    for sequence in sequences(traced.pattern(), "n(\"") {
        let elements = elements(sequence, &weights);
        for (slot, element) in elements.iter().enumerate() {
            for id in struck(element) {
                lit[slot].push(traced.marks()[id.parse::<usize>().unwrap()].span());
            }
        }
    }
    for spans in &mut lit {
        spans.sort();
    }
    assert_eq!(lit, expected);

    // The OSC schedule: one note message per sounding syllable, at an onset inside its slot.
    let schedule = Osc::new().render(&score, &style).unwrap();
    let text = schedule.to_text();
    let onsets: Vec<f64> = text
        .lines()
        .filter(|line| line.contains("/prosody/note"))
        .map(|line| line.split_whitespace().next().unwrap().parse().unwrap())
        .collect();
    let mut sent = vec![0_usize; slots];
    let mut start = 0.0;
    let mut starts = Vec::new();
    for slot in score.arrangement().slots() {
        starts.push(start);
        start += f64::from(u32::try_from(slot.beats().max(1)).unwrap());
    }
    for onset in onsets {
        let slot = starts.iter().rposition(|&s| s <= onset + 1e-9).unwrap();
        sent[slot] += 1;
    }
    assert_eq!(sent, counts);
}

#[test]
fn the_reading_reads_each_line_once_and_never_where_it_erodes() {
    let (poem, score, style) = compile(PATTERNS, Variant::Erode);
    let plan = reading::plan(&poem, &score, &style, &Mappings::all()).unwrap();
    let mut read: BTreeMap<usize, usize> = BTreeMap::new();
    for cue in plan.cues() {
        assert!(
            read.insert(cue.line(), cue.slot()).is_none(),
            "line {} twice",
            cue.line()
        );
        let eroding = score.arrangement().slots()[cue.slot()]
            .placements()
            .iter()
            .any(|p| p.bar().get() == cue.line() && matches!(p.cause(), Cause::Eroding { .. }));
        assert!(!eroding, "line {} read where it erodes", cue.line());
    }
    assert_eq!(read.len(), 107);
    assert!(plan.unread().is_empty());
}
