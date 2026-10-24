//! The trace: which syllable is sounding, on the music's own clock, for the page to light up.
//!
//! `SPEC.md` §8 makes the debug view the enforcement of I4 — every event attributable to a token
//! span. The annotation shows that at rest; the trace shows it while the piece plays: the page
//! lights each syllable of the poem as its event sounds, in its layer's colour, dimmer where the
//! line is only looping.
//!
//! # Why a pattern, built by the music's own walk
//!
//! The trace is a Strudel pattern the page evaluates but never plays: one lane per lane of the
//! music's rhythm layers, each built by the same `grid_by_slot` walk as the music, with an
//! event's *id* where the music has its sample, and with the music's own swing and tempo
//! (`strudel::clock`). So a trace event begins exactly where its sound does, by construction — a second
//! walk of bars and beats is how an earlier version of the emitter attached every gain to the
//! wrong syllable (see `grid`'s docs). The page queries it at Strudel's current time each
//! animation frame and looks each id up in [`Trace::marks`].
//!
//! The rhythm layers are enough: every syllable that sounds is an event of exactly one voice, and
//! the pitched layers sound a subset of those same events. A mark says whether its syllable also
//! carries a melody note or a bass note, so the page can say so too.
//!
//! Like the reading ([`crate::reading`]) it is not part of the pattern text a writer pastes, and
//! nothing the rest of the project measures reads it. It changes no event and reads no mapping
//! beyond the score's own arrangement.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use prosody_core::Span;
use prosody_core::arrange::Cause;
use prosody_core::role::Layer;
use prosody_core::voice::{Event, Score};

use crate::Style;
use crate::strudel::{Colour, Lane, clock, cycles_per_minute, grid_by_slot, starts};

/// One syllable as the page lights it: its span, its colour, and whether it is new here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Mark {
    span: Span,
    colour: Colour,
    fresh: bool,
    melody: bool,
    bass: bool,
}

impl Mark {
    /// The syllable's bytes in the source.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Its layer's colour, as CSS takes it — the same as its notes on the roll.
    #[must_use]
    pub const fn colour(&self) -> &'static str {
        self.colour.hex()
    }

    /// Whether its line is being heard anew here — stated, arriving or overruling (§4.6) — rather
    /// than looping, yielding, holding, calling, refraining or repeating in the outro.
    #[must_use]
    pub const fn fresh(&self) -> bool {
        self.fresh
    }

    /// Whether the syllable also plays a rhyme note (§4.5).
    #[must_use]
    pub const fn melody(&self) -> bool {
        self.melody
    }

    /// Whether it also plays a bass note (§4.1).
    #[must_use]
    pub const fn bass(&self) -> bool {
        self.bass
    }
}

/// The trace of one piece: a pattern whose values are indices into [`Trace::marks`], and where
/// its bars fall.
#[derive(Debug, Clone, PartialEq)]
pub struct Trace {
    pattern: String,
    marks: Vec<Mark>,
    bars: Vec<u64>,
    cpm: f64,
}

impl Trace {
    /// The pattern, to evaluate and query — not to play: it has no sound.
    #[must_use]
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// What each of the pattern's values stands for.
    #[must_use]
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The cycle of the pattern each bar of the piece begins on, from 0, and last the piece's
    /// length, where it loops round — the music's own bars (`strudel::starts`). Under the steady
    /// pulse a cycle is a beat, so bars of different length are unevenly spaced; under the
    /// elastic pulse a cycle is a bar, and these count 0, 1, 2, ….
    #[must_use]
    pub fn bars(&self) -> &[u64] {
        &self.bars
    }

    /// The pattern's cycles a minute — the music's `.cpm` — so a cycle lasts `60 / cpm` seconds
    /// on a scheduler at one cycle a second. A value only; print it through
    /// [`fmt_param`](crate::fmt_param), as the pattern does.
    #[must_use]
    pub const fn cycles_per_minute(&self) -> f64 {
        self.cpm
    }
}

/// Traces `score` under `style`'s tempo and swing.
#[must_use]
pub fn plan(score: &Score, style: &Style) -> Trace {
    // Ids in the order the walk meets them — slot by slot, voice by voice — so the same piece
    // always numbers its marks the same way (I1).
    let ids: RefCell<BTreeMap<Mark, usize>> = RefCell::new(BTreeMap::new());
    let marks: RefCell<Vec<Mark>> = RefCell::new(Vec::new());
    let mut lanes = Vec::new();
    for voice in score.voices() {
        let events: Vec<&Event> = score.events_of(voice.id()).collect();
        let colour = match voice.layer() {
            Layer::Foundation => Colour::Foundation,
            Layer::Flourish => Colour::Flourish,
        };
        for lane in Lane::all(score, &events, false) {
            let value = |event: &Event, slot: usize| {
                let mark = Mark {
                    span: event.span(),
                    colour,
                    fresh: fresh(score, slot, event),
                    melody: event.pitch().is_some(),
                    bass: event.bass().is_some(),
                };
                let mut ids = ids.borrow_mut();
                let next = ids.len();
                let id = *ids.entry(mark).or_insert_with(|| {
                    marks.borrow_mut().push(mark);
                    next
                });
                id.to_string()
            };
            lanes.push(format!("  n(\"{}\")", grid_by_slot(lane, &value, "~")));
        }
    }

    let mut pattern = String::from(
        "// the trace: which syllable sounds when, for the page to light (SPEC.md §8) — not sound\n",
    );
    if lanes.is_empty() {
        pattern.push_str("silence\n");
    } else {
        pattern.push_str("stack(\n");
        let _ = writeln!(pattern, "{}", lanes.join(",\n"));
        pattern.push_str(")\n");
        pattern.push_str(&clock(score, style));
    }
    Trace {
        pattern,
        marks: marks.into_inner(),
        // The page draws the roll's bar lines from these: they can no longer be assumed equal.
        bars: starts(score),
        cpm: cycles_per_minute(score, style),
    }
}

/// Whether `event` is heard anew in slot `slot`: its line's placement there is a statement, an
/// arrival or an overruling rather than a loop of something already heard.
fn fresh(score: &Score, slot: usize, event: &Event) -> bool {
    score
        .arrangement()
        .slots()
        .get(slot)
        .and_then(|slot| {
            slot.placements()
                .iter()
                .find(|placement| score.sounds(**placement, event))
        })
        .is_some_and(|placement| {
            matches!(
                placement.cause(),
                Cause::Stated | Cause::Arriving { .. } | Cause::Overruling
            )
        })
}

#[cfg(test)]
mod tests {
    use prosody_core::lower::{self, Analyses};
    use prosody_core::phoneme::arpabet::ARPABET;
    use prosody_core::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };
    use prosody_core::surprisal::{Lexicon, Rarity};
    use prosody_core::voice::Score;
    use prosody_core::{Poem, meter, repetition, rhyme, role, sound, surprisal};

    use super::plan;
    use crate::Style;

    /// Spelling is pronunciation: `A` a stressed vowel, `e` an unstressed one — the stub
    /// `reading`'s tests use, so syllables can be worked out by hand.
    struct Spelled;

    impl Phonemiser for Spelled {
        fn inventory(&self) -> &'static Inventory {
            &ARPABET
        }

        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            let phones: Vec<_> = word
                .chars()
                .filter_map(|letter| match letter {
                    'A' => Some((ARPABET.phoneme("AA").unwrap(), Stress::Primary)),
                    'e' => Some((ARPABET.phoneme("IH").unwrap(), Stress::Unstressed)),
                    'b' => Some((ARPABET.phoneme("B").unwrap(), Stress::Unstressed)),
                    'd' => Some((ARPABET.phoneme("D").unwrap(), Stress::Unstressed)),
                    't' => Some((ARPABET.phoneme("T").unwrap(), Stress::Unstressed)),
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

        fn rarity(&self, _word: &str) -> Rarity {
            Rarity::band(1)
        }
    }

    fn lowered(source: &str) -> (Poem, Score) {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &Flat);
        let roles = role::assign(&meter, &repetition).unwrap();
        let score = lower::run(Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme,
            repetition: &repetition,
            sound: &sound,
            surprisal: &surprisal,
            roles: &roles,
        })
        .unwrap();
        (poem, score)
    }

    fn at_120() -> Style {
        Style::parse("+++\ntempo_range = [120, 120]\n+++").unwrap()
    }

    #[test]
    fn every_syllable_that_sounds_is_marked_and_resolves_to_its_source() {
        // I4, made visible: each mark is a span of the source, and every sounding event has one.
        let (poem, score) = lowered("eA eA\nteA teA\n");
        let trace = plan(&score, &at_120());
        for event in score.events() {
            assert!(
                trace.marks().iter().any(|mark| mark.span() == event.span()),
                "{:?} unmarked",
                event.span()
            );
        }
        for mark in trace.marks() {
            assert!(mark.span().text(poem.source()).is_some());
        }
    }

    #[test]
    fn the_trace_walks_the_bars_the_music_walks() {
        // Hand-worked. Two two-foot lines of one stanza, one layer: the arrangement states each
        // line once, so the trace's first lane is the music's rhythm grid with ids for samples.
        let (_, score) = lowered("eA eA\nteA teA\n");
        let trace = plan(&score, &at_120());
        let music = crate::Backend::render(&crate::Strudel::new(), &score, &at_120()).unwrap();
        let first_music_lane = music
            .lines()
            .find(|line| line.trim_start().starts_with("s(\""))
            .unwrap();
        let first_trace_lane = trace
            .pattern()
            .lines()
            .find(|line| line.trim_start().starts_with("n(\""))
            .unwrap();
        // Same shape: replace each step's value by `x` and the two grids are the same string.
        let shape = |line: &str| -> String {
            let inside = line.split('"').nth(1).unwrap();
            inside
                .split(|c: char| " []<>".contains(c))
                .map(|step| {
                    if step.is_empty() || step == "~" {
                        step
                    } else {
                        "x"
                    }
                })
                .collect::<Vec<_>>()
                .join("|")
        };
        assert_eq!(shape(first_trace_lane), shape(first_music_lane));
        // And the same clock: the music's `.cpm` closes the trace too.
        let cpm = music.lines().last().unwrap();
        assert_eq!(trace.pattern().lines().last().unwrap(), cpm);
    }

    #[test]
    fn a_loop_is_marked_apart_from_its_statement() {
        // Hand-worked from `prosody_core::arrange`. Two stanzas of one layer: the first is stated
        // (slots 0, 1), then the second transforms its loop one line a pass — pass 1 replaces
        // line 1 and leaves line 2 yielding (slot 3), pass 2 replaces line 2. So "teA teA"
        // (byte 6) is heard fresh in slot 1 and again, not fresh, while it yields.
        let (_, score) = lowered("eA eA\nteA teA\n\nbeA beA\ndeA deA\n");
        let trace = plan(&score, &at_120());
        let second: Vec<bool> = trace
            .marks()
            .iter()
            .filter(|m| m.span().start() == 6)
            .map(super::Mark::fresh)
            .collect();
        assert!(second.contains(&true), "{second:?}");
        assert!(second.contains(&false), "{second:?}");
        // Line 1 is replaced on the first pass, so it never loops: every mark of it is fresh.
        assert!(
            trace
                .marks()
                .iter()
                .filter(|m| m.span().start() == 0)
                .all(super::Mark::fresh)
        );
    }

    #[test]
    fn the_same_piece_traces_the_same_bytes() {
        // I1.
        let (_, score) = lowered("eA eA\nteA teA\n\nbeA beA\ndeA deA\n");
        assert_eq!(plan(&score, &at_120()), plan(&score, &at_120()));
    }
}
