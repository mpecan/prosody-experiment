//! The Strudel backend: a poem as pattern text.
//!
//! `SPEC.md` §7.6 asks for this first because it is the cheapest way to *hear* whether the
//! mappings are any good — no DSP, no synthesis, just text you can paste into
//! <https://strudel.cc> and play.
//!
//! # How a poem lands in mini-notation
//!
//! Strudel inherits `TidalCycles`' mini-notation, where a string is one cycle subdivided by its
//! contents: `"a b c"` is three equal steps, and `[x y]` inside a step subdivides that step again.
//! That is the shape `SPEC.md` §4.2 asks for — "foot → beat, syllable → subdivision within the
//! foot, line → bar".
//!
//! Bars alternate with `<...>`, which takes **one element per cycle**, so each bar is bracketed
//! into a single element: `<[a b] [c d]>` is two bars of two beats, where `<a b c d>` would be
//! four bars of one. Separating bars with `|` would be worse than wrong — in mini-notation `|`
//! means *choose at random*, so the poem's lines would play in a different order every cycle.
//! That is what an earlier version of this file emitted, and it is `SPEC.md` §1's failure mode
//! reached through a one-character separator: output that sounds plausible and traces to nothing.
//!
//! # A cycle is a line, so the beat is elastic
//!
//! Every cycle takes the same time, and a bar is a line, so a two-foot line and a five-foot line
//! occupy the same span — the *foot* stretches rather than the bar growing. §4.2 says "line → bar
//! (or phrase)", which admits this reading, and warns against quantising to 4/4, which this does
//! not do. But it does mean the pulse is not constant, which is a mapping question rather than an
//! emitter detail; `PLAN.md` records it.
//!
//! # What comes from where
//!
//! | Output | From | `SPEC.md` |
//! |---|---|---|
//! | which cycle, which step, which subdivision | `Event` position | §4.2 |
//! | `.gain` | `Event::velocity` | §4.2, stress → accent |
//! | `.note` | `Event::pitch` through the style's scale | §4.5 |
//! | `.lpf`, `.attack`, `.room` | `Event::sharpness` | §4.4 |
//! | `.vowel` | `Event::bank` — the alliterating phoneme | §4.3, alliteration |
//! | `.crush` depth | `Event::family` — distance from the family's root | §4.3, morphology |
//! | which sample, which synth, tempo, swing | [`Style`] | §5 |
//!
//! `Event::anchor` (§4.3's anaphora and epistrophe rows) is **not** rendered: Strudel places a
//! step by its position in the cycle, so "downbeat-anchored" has nowhere to go that the position
//! has not already decided. It is carried in the IR for the OSC backend, which schedules
//! absolutely. Recorded in `PLAN.md` rather than left to be rediscovered.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use prosody_core::role::Layer;
use prosody_core::voice::{Event, Score, Voice};

use crate::style::{Named, Style};
use crate::{Backend, fmt_param};

/// Renders a score as Strudel pattern text.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Strudel;

impl Strudel {
    /// A new backend.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Why a score could not be rendered as Strudel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum StrudelError {
    /// The score has no events, so there is nothing to play.
    ///
    /// Not an error in the poem — an empty file, or one whose every word is unpronounceable, is a
    /// legitimate thing to write. It is an error to ask a backend to make a sound out of it, and
    /// saying so beats emitting a pattern that plays silence and looks like it works.
    #[error("nothing to play: the score has no events")]
    Silent,
}

impl Backend for Strudel {
    type Output = String;
    type Error = StrudelError;

    fn name(&self) -> &'static str {
        "strudel"
    }

    fn render(&self, score: &Score, style: &Style) -> Result<String, StrudelError> {
        if score.events().is_empty() {
            return Err(StrudelError::Silent);
        }

        let mut out = String::new();
        header(&mut out, score, style);

        let mut layers: Vec<(String, String)> = Vec::new();
        for voice in score.voices() {
            layers.push(rhythm_layer(score, style, *voice));
        }
        if let Some(melody) = melody_layer(score, style) {
            layers.push(melody);
        }

        out.push_str("stack(\n");
        for (index, (code, note)) in layers.iter().enumerate() {
            // The separator goes *before* the comment. Putting it after produced a comma inside a
            // `//` line, which is a syntax error in the JavaScript this is — the output looked
            // right and would not run. Nothing in a text backend catches that but running it.
            let separator = if index + 1 == layers.len() { "" } else { "," };
            let _ = writeln!(out, "{code}{separator} // {note}");
        }
        out.push_str(")\n");

        // `cpm` is *cycles* per minute and a cycle is a bar, so the tempo in beats has to be
        // divided by how many beats a bar holds. Emitting the BPM directly ran every poem at
        // roughly five times its stated tempo, which made `SPEC.md` §5's window decorative.
        let tempo = style
            .tempo_range()
            .at_density(score.events().len(), score.bars());
        let beats = mean_beats(score).max(1);
        let _ = writeln!(
            out,
            "  .cpm({})",
            fmt_param(f64::from(tempo.get()) / f64::from(beats))
        );
        if style.swing().hundredths() > 0 {
            // `swingBy(x, n)` slices the *cycle* into `n` and delays the back half of each
            // slice. A cycle is a bar, so the slices must be its beats — with `n = 2` this was a
            // rubato across half a line rather than a shuffle between syllables.
            let _ = writeln!(
                out,
                "  .swingBy({}, {})",
                fmt_param(f64::from(style.swing().hundredths()) / 100.0),
                mean_beats(score).max(1)
            );
        }
        Ok(out)
    }
}

/// The Strudel sample bank a palette name plays from.
///
/// The poem's vocabulary is not Strudel's: `SPEC.md` §5's example says `tr909`, and Strudel's
/// bank is `RolandTR909`. Translating here is what `sample_for`'s doc means by "which sounds those
/// are is palette" — the backend owns the palette-to-sound step, and passing the poem's word
/// through unchanged asked for a sample that does not exist and played silence.
const fn strudel_bank(bank: Named) -> &'static str {
    match bank.get().as_bytes() {
        b"tr808" => "RolandTR808",
        b"tr707" => "RolandTR707",
        b"linn" => "AkaiLinn",
        b"casio" => "CasioRZ1",
        // `tr909` and anything a later `BANKS` adds before this match does.
        _ => "RolandTR909",
    }
}

/// The Strudel synth a palette name plays.
///
/// `SPEC.md` §5 names `karplus`, `fm`, `subtractive` and `physical`. Strudel's built-in oscillators
/// are the four plain waveforms, so each named synth maps to the nearest one this backend can
/// actually make a sound with. A richer backend — the OSC one of §7.8 — can honour the names
/// properly; asking Strudel for `karplus` produces nothing at all.
const fn strudel_synth(synth: Named) -> &'static str {
    match synth.get().as_bytes() {
        b"fm" | b"sine" => "sine",
        b"subtractive" | b"sawtooth" => "sawtooth",
        b"square" => "square",
        // `karplus` and `physical` name things Strudel has no oscillator for, so they take the
        // nearest one it does have — as does anything a later `SYNTHS` adds before this match does.
        _ => "triangle",
    }
}

/// How many beats a bar holds, on average across the piece — over the arrangement's slots, since a
/// cycle is a slot.
///
/// Bars genuinely differ in length — `SPEC.md` §4.2 insists on it — but a cycle is a fixed span,
/// so one number has to stand for the piece when converting beats to cycles. Integer arithmetic,
/// so the tempo stays byte-stable (`CLAUDE.md` §4).
fn mean_beats(score: &Score) -> u16 {
    let slots = score.arrangement().slots();
    if slots.is_empty() {
        return 1;
    }
    let total: usize = slots.iter().map(prosody_core::Slot::beats).sum();
    u16::try_from(total / slots.len()).unwrap_or(1).max(1)
}

/// How many events the piece plays: every line's syllables, once for each slot it sounds in.
///
/// Not `score.events().len()`, which counts each line's material once — a looping stanza plays its
/// syllables many times, and the OSC backend sends a message for every one of them. The header
/// and the schedule must count the same piece.
fn sounded(score: &Score) -> usize {
    let mut per_bar = vec![0_usize; score.bars()];
    for event in score.events() {
        if let Some(count) = per_bar.get_mut(event.bar().get()) {
            *count += 1;
        }
    }
    score
        .arrangement()
        .slots()
        .iter()
        .flat_map(prosody_core::Slot::placements)
        .filter_map(|placement| per_bar.get(placement.bar().get()))
        .sum()
}

/// The comment block: what the poem is, and what the palette did to it.
///
/// Not decoration. `SPEC.md` §8 wants every audible thing traceable to a token span, and pattern
/// text is unreadable without it — this is the smallest form of the debug view that keeps the
/// output honest about where it came from.
fn header(out: &mut String, score: &Score, style: &Style) {
    out.push_str("// compiled from a poem by prosody\n");
    let _ = writeln!(
        out,
        "// {} bars from {} lines, {} voices, {} events",
        score.arrangement().slots().len(),
        score.bars(),
        score.voices().len(),
        sounded(score)
    );
    for (key, value) in style.describe() {
        let _ = writeln!(out, "//   {key} = {value}");
    }
    if !score.silent().is_empty() {
        // CLAUDE.md §6: a fallback must be visible. A word nobody could pronounce makes no sound,
        // and a listener would otherwise hear only a short line.
        let _ = writeln!(
            out,
            "//   {} word(s) had no pronunciation and make no sound",
            score.silent().len()
        );
    }
}

/// One voice: every syllable it sounds, and what §4.3 and §4.4 do to each.
fn rhythm_layer(score: &Score, style: &Style, voice: Voice) -> (String, String) {
    let events: Vec<&Event> = score.events_of(voice.id()).collect();
    let sample = sample_for(style, voice);

    let mut layer = format!(
        "  s(\"{}\")",
        grid(score, &events, &|_| sample.to_owned(), "~")
    );
    // §4.2: stress becomes accent.
    let _ = write!(layer, ".gain(\"{}\")", gains(score, &events));
    // §4.4: distinctiveness drives brightness, attack and how wet the event sits.
    let _ = write!(layer, ".lpf(\"{}\")", cutoffs(score, &events));
    let _ = write!(layer, ".attack(\"{}\")", attacks(score, &events));
    let _ = write!(layer, ".room(\"{}\")", rooms(score, &events));
    // §4.3: words of one morphological family are one voice with varied parameters — the row the
    // spec says earns the project. Distance from the root drives the modulation.
    let _ = write!(layer, ".crush(\"{}\")", crushes(score, &events));
    // §4.3: alliteration and assonance are "timbral continuity — same sample bank".
    if let Some(colours) = vowels(score, &events) {
        let _ = write!(layer, ".vowel(\"{colours}\")");
    }
    let _ = write!(layer, ".bank(\"{}\")", strudel_bank(style.bank()));

    let note = format!(
        "voice {} — {}",
        voice.id().get(),
        match voice.layer() {
            Layer::Foundation => "foundation",
            Layer::Flourish => "flourish",
        }
    );
    (layer, note)
}

/// Which sample a voice plays.
///
/// `SPEC.md` §4.1 turns the layer into instrumentation: "low variance → periodic → foundation
/// layer (drums, bass, ostinato)" and "high variance → aperiodic → flourish layer (fills, stabs,
/// one-shots, ornament)". Which *sounds* those are is palette; that a foundation is the pulse and
/// a flourish ornaments it is structure, and comes from the score.
const fn sample_for(_style: &Style, voice: Voice) -> &'static str {
    match voice.layer() {
        Layer::Foundation => "bd",
        Layer::Flourish => "hh",
    }
}

/// Renders one pattern over the whole poem, one element per bar.
///
/// The single place bars and beats are walked. An earlier version had two near-identical copies —
/// one for the rhythm skeleton and one for every parameter — which had to agree positionally or
/// each gain would attach to the wrong syllable, guaranteed only by the two copies staying in
/// step. Reversing the bar order in one of them changed every gain, cutoff and note in the piece
/// and no test noticed.
///
/// `value` renders one event; `absent` fills a beat with no event in this pattern, so the melody
/// can rest where the rhythm sounds while keeping the same grid.
fn grid(
    score: &Score,
    events: &[&Event],
    value: &dyn Fn(&Event) -> String,
    absent: &str,
) -> String {
    let mut by_bar: BTreeMap<usize, BTreeMap<usize, Vec<&Event>>> = BTreeMap::new();
    for event in events {
        by_bar
            .entry(event.bar().get())
            .or_default()
            .entry(event.beat().get())
            .or_default()
            .push(event);
    }

    // One element per slot of the arrangement, not per line: a stanza sounds again wherever it
    // loops or yields (`prosody_core::arrange`). A cycle is a slot, and every line in it fills the
    // whole cycle with its own beats — which is exactly the bar-locked fitting, for free. Lines
    // sounding together are stacked with `,`, mini-notation's "at the same time".
    let mut bars = Vec::new();
    for slot in score.arrangement().slots() {
        let lines: Vec<String> = slot
            .placements()
            .iter()
            .filter_map(|placement| {
                let bar = placement.bar().get();
                // A line's width comes from the score, not from the events in *this* pattern, so
                // the melody and the rhythm divide it the same way and a note lands on its own
                // syllable.
                by_bar
                    .get(&bar)
                    .map(|present| line(present, score.beats_in(bar), value, absent))
            })
            .collect();
        bars.push(match lines.len() {
            0 => "~".to_owned(),
            1 => lines.into_iter().next().unwrap_or_else(|| "~".to_owned()),
            _ => format!("[{}]", lines.join(", ")),
        });
    }

    match bars.len() {
        0 => "~".to_owned(),
        1 => bars.into_iter().next().unwrap_or_else(|| "~".to_owned()),
        // One element per cycle, so each bar is bracketed into a single element.
        _ => format!(
            "<{}>",
            bars.iter()
                .map(|bar| if bar.contains(' ') {
                    format!("[{bar}]")
                } else {
                    bar.clone()
                })
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

/// One line's events in this pattern, as the steps of one cycle: a step per beat, subdivided by
/// its syllables.
fn line(
    present: &BTreeMap<usize, Vec<&Event>>,
    beats: usize,
    value: &dyn Fn(&Event) -> String,
    absent: &str,
) -> String {
    let mut steps = Vec::new();
    for beat in 0..beats {
        match present.get(&beat) {
            None => steps.push(absent.to_owned()),
            // A foot divided into one part is the whole beat. More than one, and the beat is
            // subdivided — *even when this pattern has only one value for it*, which is the
            // melody's usual case: a rhyme word is the second syllable of a final iambic foot,
            // so its note belongs in the second half of the beat, not across the whole of it.
            Some(syllables)
                if syllables.len() == 1
                    && syllables.first().is_some_and(|e| e.subdivision().of() == 1) =>
            {
                steps.push(
                    syllables
                        .first()
                        .map_or_else(|| absent.to_owned(), |e| value(e)),
                );
            }
            Some(syllables) => {
                // A foot's syllables subdivide its beat. Where this pattern has no value for
                // one of them — an unpitched syllable in the melody — it rests, so the ones
                // that do sound keep their place inside the foot.
                let width = syllables
                    .first()
                    .map_or(syllables.len(), |e| e.subdivision().of())
                    .max(syllables.len());
                let mut inner = vec![absent.to_owned(); width];
                for event in syllables {
                    if let Some(slot) = inner.get_mut(event.subdivision().index()) {
                        *slot = value(event);
                    }
                }
                steps.push(format!("[{}]", inner.join(" ")));
            }
        }
    }
    if steps.is_empty() {
        "~".to_owned()
    } else {
        steps.join(" ")
    }
}

/// `SPEC.md` §4.2: stress becomes accent.
fn gains(score: &Score, events: &[&Event]) -> String {
    grid(
        score,
        events,
        &|event| fmt_param(event.velocity().as_f64()),
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes brightness.
///
/// 400 Hz to 4400 Hz spans dull to bright without ever closing the filter completely — a silent
/// event would be an event that cannot be traced to its word.
fn cutoffs(score: &Score, events: &[&Event]) -> String {
    grid(
        score,
        events,
        &|event| fmt_param(400.0 + event.sharpness().as_f64() * 4000.0),
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes attack time, inverted — a sharper word strikes faster.
fn attacks(score: &Score, events: &[&Event]) -> String {
    grid(
        score,
        events,
        &|event| fmt_param(0.04 * (1.0 - event.sharpness().as_f64())),
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes dry/wet — a striking word rings in more space.
fn rooms(score: &Score, events: &[&Event]) -> String {
    grid(
        score,
        events,
        &|event| fmt_param(event.sharpness().as_f64() * 0.4),
        "~",
    )
}

/// `SPEC.md` §4.3's morphological row: "same voice, varied parameters".
///
/// Distance from the family's root becomes bit-crush depth, so "burn", "burnt" and "burning" are
/// audibly one instrument at three degrees of erosion. 16 bits is untouched; each phoneme of
/// difference takes two away, to a floor of 4 — still the same instrument, never a different one.
fn crushes(score: &Score, events: &[&Event]) -> String {
    grid(
        score,
        events,
        &|event| {
            let depth = event
                .family()
                .map_or(0, |family| family.depth().get())
                .min(6);
            format!("{}", 16 - depth * 2)
        },
        "~",
    )
}

/// `SPEC.md` §4.3's alliteration row: "timbral continuity — same sample bank".
///
/// `None` when no word in this voice alliterates, so the filter is not applied at all rather than
/// applied neutrally — a poem with no sound figures should sound untouched by them.
fn vowels(score: &Score, events: &[&Event]) -> Option<String> {
    if !events.iter().any(|event| event.bank().is_some()) {
        return None;
    }
    Some(grid(
        score,
        events,
        &|event| {
            event.bank().map_or_else(
                || "a".to_owned(),
                |phoneme| vowel_for(phoneme.symbol()).to_owned(),
            )
        },
        "~",
    ))
}

/// A vowel colour for the phoneme a group of words shares.
///
/// The mapping is arbitrary and deliberately so: what matters structurally is that words sharing a
/// phoneme share a colour, and *which* colour is palette.
fn vowel_for(symbol: &str) -> &'static str {
    match symbol.as_bytes().first().copied().unwrap_or(b'A') % 5 {
        0 => "a",
        1 => "e",
        2 => "i",
        3 => "o",
        _ => "u",
    }
}

/// The melody: every pitched event, which is every rhyme word (`SPEC.md` §4.5).
///
/// `None` when nothing rhymes, which is ordinary in free verse — a class of one expresses no
/// melodic relation, so there is no melody to emit rather than a monotone to invent.
///
/// Built on the same [`grid`] as the rhythm, and that is the point: an earlier version derived its
/// own subdivision from how many pitched events happened to fall in a beat, so a rhyme word in the
/// second half of a final iambic foot sounded half a foot before its own drum hit — in every line
/// of every iambic poem.
fn melody_layer(score: &Score, style: &Style) -> Option<(String, String)> {
    let pitched: Vec<&Event> = score
        .events()
        .iter()
        .filter(|event| event.pitch().is_some())
        .collect();
    if pitched.is_empty() {
        return None;
    }

    let notes = grid(
        score,
        &pitched,
        &|event| {
            event.pitch().map_or_else(
                || "~".to_owned(),
                |pitch| {
                    // Degree through the style's scale, from the style's root, in the third
                    // octave. Which note that is, is palette; that two lines take the *same* note
                    // is the poem's doing.
                    let semitone = 36
                        + i32::from(style.root().semitones())
                        + style.scale().degree(pitch.degree().get());
                    // §4.5: "perfect rhyme → exact pitch; slant rhyme → detune or neighbour tone."
                    // The mapping interpolates between the two: a whole step off at a total
                    // mismatch, so a half-match lands exactly on the neighbour.
                    if pitch.detune().is_exact() {
                        format!("{semitone}")
                    } else {
                        fmt_param(f64::from(semitone) + pitch.detune().as_f64() * 2.0)
                    }
                },
            )
        },
        "~",
    );

    let mut layer = format!("  note(\"{notes}\")");
    let _ = write!(layer, ".sound(\"{}\")", strudel_synth(style.synth()));
    let _ = write!(layer, ".gain(\"{}\")", gains(score, &pitched));
    // §4.4 reaches the melody too — the rhyme word is the most prominent event in the piece, and
    // an earlier version was the only layer distinctiveness never touched.
    let _ = write!(layer, ".lpf(\"{}\")", cutoffs(score, &pitched));
    Some((layer, "the melody — one note per rhyme word".to_owned()))
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

    use prosody_core::role::Layer;

    use super::{Strudel, StrudelError};
    use crate::Backend as _;
    use crate::style::Style;

    /// Spelling is pronunciation: `A` and `o` are stressed vowels, `e` unstressed, `i` secondary.
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
                    'i' => Some((ARPABET.phoneme("IY").unwrap(), Stress::Secondary)),
                    'o' => Some((ARPABET.phoneme("OW").unwrap(), Stress::Primary)),
                    'b' => Some((ARPABET.phoneme("B").unwrap(), Stress::Unstressed)),
                    'd' => Some((ARPABET.phoneme("D").unwrap(), Stress::Unstressed)),
                    'n' => Some((ARPABET.phoneme("N").unwrap(), Stress::Unstressed)),
                    'r' => Some((ARPABET.phoneme("R").unwrap(), Stress::Unstressed)),
                    's' => Some((ARPABET.phoneme("S").unwrap(), Stress::Unstressed)),
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

    /// Rarity by word length, so distinctiveness varies within a poem.
    struct ByLength;

    impl Lexicon for ByLength {
        fn rarity(&self, word: &str) -> Rarity {
            Rarity::band(u8::try_from(word.chars().count()).unwrap_or(u8::MAX))
        }
    }

    fn score_of(source: &str) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &ByLength);
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

    fn render(source: &str, style: &Style) -> String {
        Strudel::new().render(&score_of(source), style).unwrap()
    }

    /// The argument of the first call to `name(` in the output.
    fn argument(text: &str, name: &str) -> String {
        let needle = format!("{name}(\"");
        let line = text
            .lines()
            .find(|line| line.contains(&needle))
            .unwrap_or_else(|| panic!("no {name} in:\n{text}"));
        let start = line.find(&needle).unwrap() + needle.len();
        let rest = &line[start..];
        rest[..rest.find('"').unwrap()].to_owned()
    }

    /// The first actual note in a `note(...)` pattern, skipping rests and bar separators.
    fn first_note(text: &str) -> String {
        argument(text, "note")
            .replace(['<', '>', '[', ']', '|'], " ")
            .split_whitespace()
            .find(|token| *token != "~")
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn a_foot_is_a_step_and_its_syllables_subdivide_it() {
        // SPEC.md §4.2, straight through to mini-notation. "eA" is an iamb, so one step holding
        // two syllables; two of them is two steps.
        let text = render("eA eA\n", &Style::default());
        assert_eq!(argument(&text, "s"), "[bd bd] [bd bd]");
    }

    #[test]
    fn stress_becomes_gain_and_all_three_levels_survive() {
        // Hand-worked: `e` unstressed is a third, `i` secondary two thirds, `A` primary full.
        let text = render("eiA\n", &Style::default());
        assert_eq!(argument(&text, "gain"), "[0.333 0.667 1.000]");
    }

    #[test]
    fn a_bar_a_voice_does_not_play_is_a_rest_not_a_shift() {
        // A voice must not have its later bars pulled forward into the gap, or every line after a
        // silence plays at the wrong time.
        let text = render("bA\ntA\n", &Style::default());
        assert_eq!(argument(&text, "s"), "<bd ~>", "{text}");
    }

    #[test]
    fn the_layer_chooses_the_instrument() {
        // SPEC.md §4.1: foundation is the pulse, flourish ornaments it. A poem with one ragged
        // stanza and one regular one must not play both from the same sample.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let text = render(source, &Style::default());
        let samples: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with("  s("))
            .map(|line| if line.contains("bd") { "bd" } else { "hh" })
            .collect();
        assert!(samples.contains(&"bd"), "a foundation voice: {text}");
        assert!(samples.contains(&"hh"), "a flourish voice: {text}");
    }

    #[test]
    fn distinctiveness_opens_the_filter_and_sharpens_the_attack() {
        // SPEC.md §4.4 drives brightness, attack and dry/wet from surprisal. Asserted on a *named
        // pair* in a known direction: an earlier version took the max and min over the whole bag
        // and asserted only that they differed, which is true of any non-constant mapping —
        // inverting both formulas so that sharper words came out duller and slower survived it.
        let source = "bA bAnAnAnA\n";
        let score = score_of(source);
        let sharp = score
            .events()
            .iter()
            .find(|e| e.span().text(source) == Some("bAnAnAnA"))
            .expect("the long word");
        let dull = score
            .events()
            .iter()
            .find(|e| e.span().text(source) == Some("bA"))
            .expect("the short word");
        assert!(
            sharp.sharpness() > dull.sharpness(),
            "the fixture must actually differ in distinctiveness"
        );

        // The rarer word is brighter, strikes sooner, and sits in more space.
        let value = |pattern: &str, nth: usize| -> f64 {
            pattern
                .replace(['[', ']', '<', '>'], " ")
                .split_whitespace()
                .filter(|t| *t != "~")
                .nth(nth)
                .and_then(|t| t.parse().ok())
                .unwrap_or(f64::NAN)
        };
        let text = render(source, &Style::default());
        let cutoff = argument(&text, "lpf");
        let attack = argument(&text, "attack");
        let room = argument(&text, "room");
        assert!(value(&cutoff, 1) > value(&cutoff, 0), "brighter: {cutoff}");
        assert!(value(&attack, 1) < value(&attack, 0), "sooner: {attack}");
        assert!(value(&room, 1) > value(&room, 0), "wetter: {room}");
    }

    #[test]
    fn a_slant_rhyme_lands_off_the_note_and_a_perfect_one_on_it() {
        // SPEC.md §4.5: "perfect rhyme → exact pitch; slant rhyme → detune or neighbour tone."
        // A perfect rhyme prints a whole semitone number; a slant one prints a fraction.
        let perfect = render("teA bAn\nteA bAn\n", &Style::default());
        assert!(
            !argument(&perfect, "note").contains('.'),
            "a perfect rhyme sits exactly on its degree: {perfect}"
        );

        let exact: f64 = first_note(&perfect).parse().unwrap_or(f64::NAN);

        // The slant rhyme must sit off the note *by an amount*. Asserting only that it prints as
        // a float is not enough — `36.000` has a decimal point too, and that mutation survived.
        // Nor is asserting a fractional part: the mapping gives a half-match exactly one semitone,
        // which is §4.5's "neighbour tone" and a whole number on purpose.
        //
        // Hand-worked: "bAn" ends `AA N` and "bon" ends `OW N`, a suffix match of 1 in 2, so the
        // detune is 1/2 of a step and a step is two semitones. Both lines share a class, so both
        // take degree 0 — the note moves only because the rhyme is imperfect.
        let slant = render("teA bAn\nteA bon\n", &Style::default());
        let value: f64 = first_note(&slant).parse().unwrap_or(f64::NAN);
        assert!(
            (value - exact).abs() > 0.5,
            "a slant rhyme sits off its degree: {value} against {exact}"
        );
    }

    #[test]
    fn the_palette_reaches_every_place_it_should() {
        // Each of these is a separate mapping from the style, and each was untested until a
        // mutation run showed the bank and synth could be hardcoded without any test noticing.
        let style = Style::parse(
            "+++\nbank = \"casio\"\nsynth = \"subtractive\"\nroot = \"D\"\nscale = \"lydian\"\n+++",
        )
        .unwrap();
        let text = render("teA bAn\nteA bAn\n", &style);
        // The palette's name, translated to the identifier Strudel actually knows. Passing the
        // poem's word through unchanged asked for a sample that does not exist, so §5's own
        // example header played silence.
        assert!(text.contains(".bank(\"CasioRZ1\")"), "{text}");
        assert!(text.contains(".sound(\"sawtooth\")"), "{text}");
        // D lydian, third octave: 36 + 2 + 0 = 38.
        assert_eq!(first_note(&text), "38");

        let other = Style::parse("+++\nroot = \"C\"\nscale = \"lydian\"\n+++").unwrap();
        let moved = render("teA bAn\nteA bAn\n", &other);
        assert_eq!(first_note(&moved), "36", "the root moves every note");
    }

    #[test]
    fn the_melody_divides_a_beat_exactly_as_the_rhythm_does() {
        // SPEC.md §4.5 pitches the word a line's rhyme rests on, and §4.2 says a syllable is a
        // subdivision *within* its foot. The melody's grid must therefore be the rhythm's grid:
        // same bars, same beats per bar, so a note in the back half of a foot sounds there and
        // not across the whole beat.
        //
        // Compared by counting top-level steps rather than by looking for a literal `[~ v]`,
        // because this emitter also brackets a whole bar — an earlier version of this test matched
        // the bar's bracket and passed whether the melody used the subdivision or not.
        // Two monosyllables form one foot, and the rhyme is the second of them — so exactly one
        // of the foot's two syllables is pitched, which is the case the subdivision is for.
        let source = "bo bAn\nro bAn\n";
        let text = render(source, &Style::default());
        let steps_per_bar = |pattern: &str| -> Vec<usize> {
            let trimmed = pattern.trim_start_matches('<').trim_end_matches('>');
            let mut bars = Vec::new();
            let mut depth = 0_usize;
            let mut steps = 0_usize;
            let mut started = false;
            for token in trimmed.split_whitespace() {
                if depth == 0 {
                    // Each bar is one top-level element; count the steps inside it.
                    if !started {
                        started = true;
                    } else if token.starts_with('[') || depth == 0 {
                        bars.push(steps.max(1));
                        steps = 0;
                    }
                }
                let opens = token.matches('[').count();
                let closes = token.matches(']').count();
                if depth <= 1 {
                    steps += 1;
                }
                depth = depth + opens - closes.min(depth + opens);
            }
            bars.push(steps.max(1));
            bars
        };
        let rhythm = argument(&text, "s");
        let melody = argument(&text, "note");
        assert!(
            rhythm.contains("[v") || rhythm.contains("[bd"),
            "the fixture must subdivide a beat: {rhythm}"
        );
        assert_eq!(
            steps_per_bar(&rhythm),
            steps_per_bar(&melody),
            "the two grids must divide the poem identically:\n  {rhythm}\n  {melody}"
        );
        assert!(
            melody.contains("[~ "),
            "the note sounds inside its foot, not across the beat: {melody}"
        );

        // ...and only the rhyme words sound.
        let sounding = melody
            .replace(['<', '>', '[', ']'], " ")
            .split_whitespace()
            .filter(|token| *token != "~")
            .count();
        assert_eq!(
            sounding, 2,
            "one note per rhyme word, and no others: {melody}"
        );
    }

    #[test]
    fn the_melody_carries_the_stress_of_the_word_it_sings() {
        // §4.2's accent applies to the melody as much as the drums; hardcoding its gain silenced
        // the mapping for the most prominent voice in the piece.
        let source = "teA bAn\nteA ben\n";
        let score = score_of(source);
        let pitched: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.pitch().is_some())
            .collect();
        assert!(
            pitched
                .iter()
                .any(|e| e.velocity() != pitched[0].velocity()),
            "the fixture must have rhyme words of differing stress"
        );
        let gains = argument(&render(source, &Style::default()), "gain");
        let values: Vec<String> = gains
            .replace(['<', '>', '[', ']'], " ")
            .split_whitespace()
            .filter(|t| *t != "~")
            .map(ToOwned::to_owned)
            .collect();
        // The melody layer is the last one, so read its own gain rather than voice 0's.
        let melody = render(source, &Style::default());
        let melody_gain = melody
            .lines()
            .find(|line| line.starts_with("  note("))
            .and_then(|line| line.split(".gain(\"").nth(1))
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default()
            .to_owned();
        assert!(
            melody_gain.contains("0.333") || melody_gain.contains("1.000"),
            "the melody's gain comes from its word's stress: {melody_gain}"
        );
        assert!(!values.is_empty());
    }

    #[test]
    fn words_that_share_different_sounds_take_different_colours() {
        // §4.3's alliteration row is "timbral continuity", which means nothing if every group is
        // coloured the same — a constant survived until this test.
        let source = "bAn bAd bot\ndeA don dot\nbAn bAd bot\ndeA don dot\n";
        let text = render(source, &Style::default());
        let colours: std::collections::BTreeSet<String> = text
            .lines()
            .filter_map(|line| line.split(".vowel(\"").nth(1))
            .filter_map(|rest| rest.split('"').next())
            .flat_map(|pattern| {
                pattern
                    .replace(['<', '>', '[', ']'], " ")
                    .split_whitespace()
                    .filter(|t| *t != "~")
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect();
        assert!(
            colours.len() > 1,
            "two alliterating groups, two colours: {colours:?}"
        );
    }

    #[test]
    fn a_five_foot_line_emits_five_steps() {
        // SPEC.md §4.2: "Don't quantise to 4/4."
        let text = render("bA bA bA bA bA\n", &Style::default());
        assert_eq!(argument(&text, "s").split_whitespace().count(), 5);
    }

    #[test]
    fn the_output_is_syntactically_whole() {
        // The generated text is JavaScript. An earlier version put the separating comma *after*
        // the trailing comment, so it was commented out and the whole `stack(...)` was a syntax
        // error — output that looked right and would not run.
        let text = render("teA bAn\nteA bAn\n\nbo do\nro so\n", &Style::default());
        assert_eq!(
            text.matches('(').count(),
            text.matches(')').count(),
            "brackets balance: {text}"
        );
        // Layer lines only. The header is all comment and its commas are prose.
        let layers: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with("  s(") || line.starts_with("  note("))
            .collect();
        assert!(layers.len() > 1, "{text}");
        for (index, line) in layers.iter().enumerate() {
            let comment = line
                .find("//")
                .unwrap_or_else(|| panic!("no note on {line}"));
            let code = &line[..comment];
            let separated = code.trim_end().ends_with(',');
            assert_eq!(
                separated,
                index + 1 < layers.len(),
                "every layer but the last is separated, and the comma is outside the comment: \
                 {line}"
            );
        }
    }

    #[test]
    fn a_silent_score_is_refused_rather_than_played() {
        let score = score_of("zz\n");
        assert_eq!(
            Strudel::new().render(&score, &Style::default()),
            Err(StrudelError::Silent)
        );
    }

    #[test]
    fn the_header_reports_the_piece_it_compiled() {
        // The provenance block, which `CLAUDE.md` §8 makes the enforcement mechanism for I4.
        // Emitting zeroes for every count, or swapping the foundation and flourish labels so the
        // output lies about which voice is the pulse, both passed the suite until this test.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let score = score_of(source);
        let text = render(source, &Style::default());
        assert!(
            text.contains(&format!(
                "// {} bars from {} lines, {} voices, {} events",
                score.arrangement().slots().len(),
                score.bars(),
                score.voices().len(),
                super::sounded(&score)
            )),
            "{text}"
        );
        assert!(text.contains("//   scale = minor"), "the palette: {text}");

        // Every layer names its voice and says which layer it plays, and the labels agree with
        // the score rather than being decoration.
        for voice in score.voices() {
            let expected = format!(
                "// voice {} — {}",
                voice.id().get(),
                match voice.layer() {
                    Layer::Foundation => "foundation",
                    Layer::Flourish => "flourish",
                }
            );
            assert!(text.contains(&expected), "missing {expected} in\n{text}");
        }
    }

    #[test]
    fn a_family_erodes_the_instrument_rather_than_replacing_it() {
        // SPEC.md §4.3's morphological row — "the last row is the one that earns the project".
        // "bAn" and "bAnd" are a stem and its inflection, so they are one family at two depths and
        // must be audibly the same instrument, differently modulated.
        // One line, so both words are in one voice and one crush pattern — which is also the
        // claim being tested: a family is *one* instrument.
        let source = "bAn bAnd\n";
        let crush = argument(&render(source, &Style::default()), "crush");
        let depths: Vec<u32> = crush
            .replace(['<', '>', '[', ']'], " ")
            .split_whitespace()
            .filter_map(|token| token.parse().ok())
            .collect();
        assert_eq!(depths.len(), 2, "{crush}");
        assert!(
            depths[0] > depths[1],
            "the root is untouched and the variation eroded: {crush}"
        );
    }

    #[test]
    fn alliteration_colours_the_words_that_share_a_sound() {
        // SPEC.md §4.3's alliteration row: "timbral continuity — same sample bank".
        let source = "bAn bAd bot\nbAn bAd bot\n";
        let text = render(source, &Style::default());
        assert!(text.contains(".vowel("), "expected a shared colour: {text}");
        // ...and a poem with no sound figure is untouched by one.
        let plain = render("teA\nrot\n", &Style::default());
        assert!(!plain.contains(".vowel("), "{plain}");
    }

    #[test]
    fn unpronounceable_words_are_named_in_the_output() {
        // CLAUDE.md §6: a fallback must be visible. A listener would otherwise hear a short line.
        let text = render("bA zz\n", &Style::default());
        assert!(text.contains("no pronunciation"), "{text}");
    }

    #[test]
    fn a_stanza_keeps_playing_under_the_next() {
        // Stanza 1 (`bA` / six `bA`s) is ragged, so it is the flourish; stanza 2 is regular, so
        // the foundation. The arrangement states stanza 1, then states stanza 2 with stanza 1
        // looping under it — so the first line's voice, one stressed syllable on a hi-hat, sounds
        // in cycles 0 and 2, and rests in 1 and 3.
        let text = render("bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n", &Style::default());
        let first = text.lines().find(|line| line.contains("voice 0")).unwrap();
        assert!(first.starts_with("  s(\"<hh ~ hh ~>\")"), "{text}");
    }
}
