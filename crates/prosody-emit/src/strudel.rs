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
//! # How long a bar lasts: the pulse
//!
//! Which is a mapping, `Mapping::Pulse`, chosen per piece and recorded on the score:
//!
//! - **steady** (the default). A bar lasts as many beats as it holds, so every beat of the piece
//!   lasts the same time — §4.2's "foot → beat" taken at its word. A **cycle is a beat**: the
//!   whole arrangement is one `<…>` whose bars carry mini-notation weights, `<[bar]@3 [bar]@4>`,
//!   and `.cpm` is the tempo itself. In `@strudel/mini` 1.2.6 `<…>` is a `polymeter_slowcat`: its
//!   sequence is a `timeCat` of the weighted bars, slowed by the weights' sum, so a bar of weight
//!   *w* lasts exactly *w* cycles — integers all the way, no rounded tempo (`sequence`).
//! - **elastic** — the pattern as it was. A **cycle is a bar**: `<[bar] [bar]>`, one element per
//!   cycle, every cycle the same time, at one `.cpm` for the piece (the tempo over the integer
//!   mean bar length). A two-foot line and a five-foot line occupy the same span, so the *foot*
//!   stretches or hurries — Wilde's Theocritus plays its 4-beat bars at 161 bpm among 3-beat bars
//!   at 121.
//!
//! Either way the bars, beats and subdivisions inside each bar are the same strings: only how long
//! a bar lasts differs. Every consumer that must land on the music's clock — this pattern, the
//! page's trace ([`crate::trace`]), the reading's cues ([`crate::reading`]) — asks the same
//! functions here (`weights`, `cycles_per_minute`, `clock`).
//!
//! # What comes from where
//!
//! | Output | From | `SPEC.md` |
//! |---|---|---|
//! | which cycle, which step, which subdivision | `Event` position | §4.2 |
//! | `.gain` | `Event::velocity` | §4.2, stress → accent |
//! | `.note` | `Event::pitch` through the style's scale | §4.5 |
//! | a rhyme note's chord tones | the same degree, stacked in the style's scale ([`Voicing`]) | §5 |
//! | `.lpf`, `.attack`, `.room` | `Event::sharpness` | §4.4 |
//! | `.vowel` | `Event::bank` — the alliterating phoneme | §4.3, alliteration |
//! | `.crush` depth | `Event::family` — distance from the family's root | §4.3, morphology |
//! | which sample, which instrument, tempo, swing | [`Style`] | §5 |
//! | `.room` floor, `.roomsize`, `.delay`, `.shape`, `.compressor`, `.postgain` | [`Style::mix`] | §5, §9 "tuning the ranges" |
//! | `.nudge`, `.velocity` — the human stray | each line's own seed, scaled by [`Style::mix`] | §6 I2 |
//! | the chord lane: which bar has a chord, on which degree | `Score::harmony_in` — the harmony mapping; a stanza's last bar under cadence split, its own chord then the turnaround from the half | §4.6, candidate |
//! | the chord lane's sound, tones, register, level | [`Style::chord_sound`], [`Voicing`], the melody's octave less one, `chord_gain` | §5 |
//!
//! # Three pitched and percussive roles
//!
//! Every line's voice is a drum (§4.1's foundation and flourish). The pitched events split by what
//! pitched them into the **melody** (`Event::pitch`, a rhyme word, §4.5) and the **bass**
//! (`Event::bass`, §4.6 — by vowel height, or walking under the bass mapping's W1 and W2). They are
//! separate layers only so that each can have its own instrument and level; which events exist and
//! where they fall is the score's, and splitting them moves nothing. Under the walking variants
//! one event can carry both, and then both layers play it.
//!
//! `Event::anchor` (§4.3's anaphora and epistrophe rows) is **not** rendered: Strudel places a
//! step by its position in the cycle, so "downbeat-anchored" has nowhere to go that the position
//! has not already decided. It is carried in the IR for the OSC backend, which schedules
//! absolutely. Recorded in `PLAN.md` rather than left to be rediscovered.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use prosody_core::mapping::Variant;
use prosody_core::role::Layer;
use prosody_core::voice::{Event, Score, Subdivision, Voice};
use prosody_core::{BassNote, Chord, Chromatic, Harmony, Silence, VoiceId};
use rand_core::Rng as _;

use crate::style::{Kit, Named, RideFallback, Style, Voicing};
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

        // The tempo in beats. `cpm` is *cycles* per minute, and what a cycle is depends on the
        // pulse ([`cycles_per_minute`]); emitting the BPM as the `.cpm` of a cycle-per-bar pattern
        // once ran every poem at roughly five times its stated tempo.
        let tempo = style
            .tempo_range()
            .at_density(score.events().len(), score.bars());

        let mut out = String::new();
        header(&mut out, score, style);

        let strays = strays(score);
        let density = density(score, style);
        let mut layers: Vec<(String, String)> = Vec::new();
        for voice in score.voices() {
            layers.extend(rhythm_layer(score, style, &strays, &density, *voice));
        }
        for role in [Pitched::Melody, Pitched::Bass] {
            layers.extend(pitched_layer(
                score,
                style,
                &strays,
                &density,
                role,
                tempo.get(),
            ));
        }
        layers.extend(chord_layer(score, style, &density));

        out.push_str("stack(\n");
        for (index, (code, note)) in layers.iter().enumerate() {
            // The separator goes *before* the comment. Putting it after produced a comma inside a
            // `//` line, which is a syntax error in the JavaScript this is — the output looked
            // right and would not run. Nothing in a text backend catches that but running it.
            let separator = if index + 1 == layers.len() { "" } else { "," };
            let _ = writeln!(out, "{code}{separator} // {note}");
        }
        out.push_str(")\n");

        out.push_str(&clock(score, style));
        Ok(out)
    }
}

/// The calls after the stack that set its time: swing, then the tempo.
///
/// One function because two patterns must keep the same time: the music, and the page's trace
/// ([`crate::trace`]), whose events must land where the music's do.
pub(crate) fn clock(score: &Score, style: &Style) -> String {
    let mut clock = String::new();
    if style.swing().hundredths() > 0 {
        // `swingBy(x, n)` is `inside(n, late(seq(0, x / 2)))` (`@strudel/core` 1.2.6
        // `pattern.mjs`): it slices the *cycle* into `n` and delays the back half of each slice.
        // The slices must be the beats — with `n = 2` across a bar this was a rubato across half
        // a line rather than a shuffle between syllables. Under the steady pulse a cycle *is* a
        // beat, so `n = 1` swings every beat whatever its bar's length; under the elastic pulse a
        // cycle is a bar, sliced by the mean bar length ([`swing_slices`]).
        //
        // It must come *before* `.cpm`. `cpm` is `_fast(cpm / 60)` (`@strudel/core` 1.2.6
        // `pattern.mjs`, which `@strudel/web` 1.3.0 pins), so after it a cycle is the
        // scheduler's one second, not a beat or a bar: `swingBy` then sliced the second into `n`,
        // out of step with the beats — onsets moved to irregular times, and a held chord's
        // onset could fall in no slice's front half (never sounding) or be split across two
        // (sounding twice). Swinging first slices the pattern's own cycles, and `cpm` then speeds
        // the swung piece up as a whole.
        let _ = writeln!(
            clock,
            "  .swingBy({}, {})",
            fmt_param(f64::from(style.swing().hundredths()) / 100.0),
            swing_slices(score)
        );
    }
    let _ = writeln!(
        clock,
        "  .cpm({})",
        fmt_param(cycles_per_minute(score, style))
    );
    clock
}

/// How many cycles of the pattern each slot of the arrangement lasts, in slot order.
///
/// `SPEC.md` §4.2, the pulse (`Mapping::Pulse`): under the steady pulse a cycle is a beat, so a
/// slot lasts its beats (at least one — a bar of nothing still takes its place); under the elastic
/// pulse a cycle is a slot, so every slot lasts one. Read from the score alone, never the style:
/// when a bar falls is structure (I3). The OSC schedule counts its beats with the same numbers.
#[must_use]
pub(crate) fn weights(score: &Score) -> Vec<u64> {
    score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| match score.pulse() {
            Variant::Elastic => 1,
            _ => u64::try_from(slot.beats()).unwrap_or(1).max(1),
        })
        .collect()
}

/// The cycle of the pattern each slot begins on, from 0, and after them the piece's length — the
/// cycle it loops round on. Exact: integers.
#[must_use]
pub(crate) fn starts(score: &Score) -> Vec<u64> {
    let mut at = 0_u64;
    let mut out = vec![0];
    for weight in weights(score) {
        at += weight;
        out.push(at);
    }
    out
}

/// One element per slot of the arrangement, laid end to end as the piece: the single place a
/// pattern's bars become a sequence, so the music, the trace and the reading's cues all last
/// exactly as long, bar for bar.
///
/// Elastic: `<a [b c]>`, one element per cycle (`<…>` takes one a cycle), or a single bar bare.
/// Steady: `<a@3 [b c]@4>` — in `@strudel/mini` 1.2.6 the angle brackets are a
/// `polymeter_slowcat` whose sequence is a `timeCat` of its weighted elements, slowed by their
/// weights' sum, so an element of weight *w* lasts exactly *w* cycles: *w* beats.
///
/// A bar with a space in it is bracketed into one element. Separating bars with `|` would be worse
/// than wrong: in mini-notation `|` means *choose at random* (see the module docs).
pub(crate) fn sequence(score: &Score, bars: &[String]) -> String {
    let element = |bar: &String| {
        if bar.contains(' ') {
            format!("[{bar}]")
        } else {
            bar.clone()
        }
    };
    match (score.pulse(), bars) {
        (_, []) => "~".to_owned(),
        (Variant::Elastic, [bar]) => bar.clone(),
        (Variant::Elastic, _) => format!(
            "<{}>",
            bars.iter().map(element).collect::<Vec<_>>().join(" ")
        ),
        (_, _) => format!(
            "<{}>",
            bars.iter()
                .zip(weights(score))
                .map(|(bar, weight)| format!("{}@{weight}", element(bar)))
                .collect::<Vec<_>>()
                .join(" ")
        ),
    }
}

/// A layer's colour, for Strudel's drawing (`.color`): the page's piano roll, or `pianoroll()` on
/// strudel.cc. Display only — `color` is read by Strudel's visualisations, never by its sound.
fn colour(colour: Colour) -> String {
    format!(".color(\"{}\")", colour.hex())
}

/// The colour each layer is drawn in, on the page's roll and in its lit poem alike, so a syllable
/// lighting up and the note it plays read as one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Colour {
    /// §4.1's pulse.
    Foundation,
    /// §4.1's ornament.
    Flourish,
    /// The vowel-height bass.
    Bass,
    /// The rhyme melody.
    Melody,
    /// The harmony layer's chords.
    Chords,
}

impl Colour {
    /// As CSS takes it.
    pub(crate) const fn hex(self) -> &'static str {
        match self {
            Self::Foundation => "#c9a6ff",
            Self::Flourish => "#6fd6c5",
            Self::Bass => "#ffb86b",
            Self::Melody => "#ff7aa8",
            Self::Chords => "#8fb3ff",
        }
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
        b"r8" => "RolandR8",
        // `tr909` and anything a later `BANKS` adds before this match does.
        _ => "RolandTR909",
    }
}

/// Which pitched role a layer plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pitched {
    /// The rhyme words (`SPEC.md` §4.5).
    Melody,
    /// The bass notes (§4.6): stressed foundation vowels, or a walking bass.
    Bass,
}

impl Pitched {
    /// Whether this role sounds `event`: the melody a rhyme note, the bass a bass note.
    const fn plays(self, event: &Event) -> bool {
        match self {
            Self::Melody => event.pitch().is_some(),
            Self::Bass => event.bass().is_some(),
        }
    }
}

/// The Strudel sound a palette instrument plays in a role.
///
/// A soundfont name (`gm_…`) is Strudel's own and passes through. `SPEC.md` §5's four synth names
/// are models Strudel does not have, so each takes the recorded instrument nearest the model,
/// per role: Karplus–Strong is a plucked string, so a plucked bass and a nylon guitar; FM's
/// signature sound is the DX7 electric piano and its synth bass; physical modelling's classic
/// case is a struck bar. An earlier version mapped all three to bare oscillators, which is why
/// every example poem sounded like a test tone. The plain waveforms stay what they say.
fn strudel_instrument(instrument: Named, role: Pitched) -> &'static str {
    let name = instrument.get();
    if name.starts_with("gm_") {
        return name;
    }
    match (name.as_bytes(), role) {
        (b"karplus" | b"physical", Pitched::Bass) => "gm_acoustic_bass",
        (b"karplus", Pitched::Melody) => "gm_acoustic_guitar_nylon",
        (b"fm", Pitched::Bass) => "gm_synth_bass_2",
        (b"fm", Pitched::Melody) => "gm_epiano1",
        (b"physical", Pitched::Melody) => "gm_marimba",
        (b"sine", _) => "sine",
        (b"subtractive" | b"sawtooth", _) => "sawtooth",
        (b"square", _) => "square",
        // `triangle`, and anything a later `SYNTHS` adds before this match does.
        _ => "triangle",
    }
}

/// The key that identifies one event within the score, for looking up its stray.
type EventKey = (VoiceId, usize, usize, usize);

const fn key(event: &Event) -> EventKey {
    (
        event.voice(),
        event.bar().get(),
        event.beat().get(),
        event.subdivision().index(),
    )
}

/// One event's stray from the machine: how late, and how much softer, each a fraction in `0..1`
/// that [`Style::mix`]'s `humanise` scales.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Stray {
    late: f64,
    soft: f64,
}

/// The latest an event strays at `humanise = 1`, in seconds. Thirty milliseconds is a loose
/// human player; past it a stray stops sounding like feel and starts sounding like a mistake.
const MAX_LATE: f64 = 0.030;

/// The most an event is softened at `humanise = 1`. Only ever softer, never louder, so the stray
/// cannot push a mix that fits into clipping.
const MAX_SOFTEN: f64 = 0.30;

/// Every event's stray, drawn from its own line's seed.
///
/// `CLAUDE.md` §4 and invariant I2: the randomness is the voice's [`Voice::seed`] — derived from
/// the line's own text — under a child named for this decision, and the draws restart for every
/// bar the voice sounds in. So a line's strays depend on that line and nothing else: an edit in
/// another stanza cannot move them, and a refrain line, which shares its voice and its text,
/// strays identically every time it returns — §4.3's "literal loop". Only late, never early:
/// Strudel's sampler starts a nudged sample after its envelope opens, and an early start would
/// sound before it, as a click.
fn strays(score: &Score) -> BTreeMap<EventKey, Stray> {
    let mut out = BTreeMap::new();
    for voice in score.voices() {
        let seed = voice.seed().child("humanise");
        let mut by_bar: BTreeMap<usize, Vec<&Event>> = BTreeMap::new();
        for event in score.events_of(voice.id()) {
            by_bar.entry(event.bar().get()).or_default().push(event);
        }
        for events in by_bar.values_mut() {
            events.sort_by_key(|event| key(event));
            let mut rng = seed.rng();
            for event in events.iter() {
                let late = unit(rng.next_u32());
                let soft = unit(rng.next_u32());
                out.insert(key(event), Stray { late, soft });
            }
        }
    }
    out
}

/// A draw as a fraction in `0..1`. A value only — it never decides anything (`CLAUDE.md` §4).
fn unit(draw: u32) -> f64 {
    f64::from(draw) / 4_294_967_296.0
}

/// `.nudge` and `.velocity` for a layer's events, or nothing when `humanise` is zero.
fn humanised(lane: Lane<'_>, style: &Style, strays: &BTreeMap<EventKey, Stray>) -> String {
    let amount = style.mix().humanise();
    if amount.is_off() {
        return String::new();
    }
    let stray = |event: &Event| {
        strays.get(&key(event)).copied().unwrap_or(Stray {
            late: 0.0,
            soft: 0.0,
        })
    };
    let nudges = grid(
        lane,
        &|event| fmt_param(stray(event).late * amount.as_f64() * MAX_LATE),
        "~",
    );
    let velocities = grid(
        lane,
        &|event| fmt_param(1.0 - stray(event).soft * amount.as_f64() * MAX_SOFTEN),
        "~",
    );
    format!(".nudge(\"{nudges}\").velocity(\"{velocities}\")")
}

/// The calls every layer shares: the reverb's size, compression, and the layer's level.
///
/// Reverb and echo are per orbit in Strudel, not per event, so `roomsize` must be the same on
/// every layer or each event would rebuild the room — it is written everywhere for that reason.
///
/// `share` divides the level between the tones of a chord ([`chord_share`]); it is 1 for a single
/// sound.
fn bus(style: &Style, level: crate::style::Amount, share: f64) -> String {
    let mix = style.mix();
    let mut calls = format!(
        ".roomsize({})",
        fmt_param(f64::from(mix.room_size_tenths()) / 10.0)
    );
    if !mix.compress().is_off() {
        // From a gentle −4 dB at 2:1 to −24 dB at 7:1. Strudel compresses each event's own
        // chain; a fixed master stage on the page (compressor, then a soft clipper) catches the
        // sum.
        let amount = mix.compress().as_f64();
        let _ = write!(
            calls,
            ".compressor({}).compressorRatio({})",
            fmt_param(-4.0 - 20.0 * amount),
            fmt_param(2.0 + 5.0 * amount)
        );
    }
    let _ = write!(calls, ".postgain({})", fmt_param(level.as_f64() * share));
    calls
}

/// The level each tone of a chord takes, so a role's level is the level of the chord.
///
/// One over the square root of the tone count: soundfont tones at different pitches are
/// uncorrelated, so their powers add, and four tones at half level carry the power of one at full.
/// The melody level a palette sets therefore means the same thing under either voicing.
fn chord_share(voicing: Voicing) -> f64 {
    let tones = u32::try_from(voicing.degrees().len()).unwrap_or(1).max(1);
    1.0 / f64::from(tones).sqrt()
}

/// How many cycles of the pattern the piece plays a minute — its `.cpm`.
///
/// One number for every consumer that has to land on the same clock: the pattern's own `.cpm`,
/// the trace's, and the reading ([`crate::reading`]), whose cues must fall on the bars they name.
///
/// Steady: a cycle is a beat, so this is the tempo itself, an integer, and every beat lasts
/// `60 / tempo` seconds. Elastic: a cycle is a bar, so the tempo in beats is divided by how many
/// beats a bar holds on average ([`mean_beats`]), and a bar of more beats than that hurries them.
pub(crate) fn cycles_per_minute(score: &Score, style: &Style) -> f64 {
    let tempo = f64::from(
        style
            .tempo_range()
            .at_density(score.events().len(), score.bars())
            .get(),
    );
    match score.pulse() {
        Variant::Elastic => tempo / f64::from(mean_beats(score).max(1)),
        _ => tempo,
    }
}

/// How many slices `swingBy` cuts a cycle into, so that each slice is a beat: one under the
/// steady pulse, whose cycle is a beat; the mean bar length under the elastic pulse, whose cycle
/// is a bar — which is a beat only in a bar of exactly that length.
fn swing_slices(score: &Score) -> u16 {
    match score.pulse() {
        Variant::Elastic => mean_beats(score).max(1),
        _ => 1,
    }
}

/// How many beats a bar holds, on average across the piece — over the arrangement's slots — for
/// the elastic pulse, whose cycle is a slot.
///
/// Bars genuinely differ in length — `SPEC.md` §4.2 insists on it — but an elastic cycle is a
/// fixed span, so one number has to stand for the piece when converting beats to cycles. Integer
/// arithmetic, so the tempo stays byte-stable (`CLAUDE.md` §4).
fn mean_beats(score: &Score) -> u16 {
    let slots = score.arrangement().slots();
    if slots.is_empty() {
        return 1;
    }
    let total: usize = slots.iter().map(prosody_core::Slot::beats).sum();
    u16::try_from(total / slots.len()).unwrap_or(1).max(1)
}

/// The header's line for the pulse: how long a bar lasts, and so what tempo each bar's beats are
/// heard at — the one place the pattern says its tempo in beats a minute.
fn pulse_header(out: &mut String, score: &Score, style: &Style) {
    let tempo = style
        .tempo_range()
        .at_density(score.events().len(), score.bars())
        .get();
    let mut lengths: Vec<usize> = score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| slot.beats().max(1))
        .collect();
    lengths.sort_unstable();
    lengths.dedup();
    let named: Vec<String> = lengths.iter().map(ToString::to_string).collect();
    match score.pulse() {
        Variant::Elastic => {
            // What the elastic bar does to each length, spelled out: the hurrying is the point.
            let cpm = cycles_per_minute(score, style);
            let heard: Vec<String> = lengths
                .iter()
                .map(|beats| {
                    let beats_f = f64::from(u32::try_from(*beats).unwrap_or(u32::MAX));
                    format!("{beats} beats at {} bpm", fmt_param(cpm * beats_f))
                })
                .collect();
            let _ = writeln!(
                out,
                "//   pulse = elastic: every bar one cycle, {} a minute (the tempo {tempo} over \
                 the mean bar of {} beats), so the beat follows the bar's length: {}",
                fmt_param(cpm),
                mean_beats(score).max(1),
                heard.join(", ")
            );
        }
        _ => {
            let _ = writeln!(
                out,
                "//   pulse = steady: a cycle is a beat at {tempo} bpm, and every bar lasts its \
                 beats (bars of {} beats)",
                named.join(", ")
            );
        }
    }
}

/// The header's line for the straight stack, which names its variant and how many beats it
/// fitted. Crossing — each line by its own feet — is the piece as it shipped, and says nothing new.
fn stack_header(out: &mut String, score: &Score) {
    if score.stack() != Variant::Straight {
        return;
    }
    let _ = writeln!(
        out,
        "//   stack = straight: a line stacked under a bar's line is fitted to that line's \
         subdivision wherever the two would cross, in {} beat(s) — three syllables over two are \
         [a@2 b c], two over three [a@2 b] (SPEC.md §4.2, §4.6, candidate)",
        straightened(score)
    );
}

/// The header's line for the light function words, which names the variant and how many
/// syllables it struck lightly. Full — every syllable at its dictionary stress — is the piece as it
/// shipped, and says nothing new.
fn function_header(out: &mut String, score: &Score) {
    if score.function() != Variant::Light {
        return;
    }
    let _ = writeln!(
        out,
        "//   function-words = light: a one-syllable function word off the metre's strong \
         positions is struck at 2/3 of full force, in {} syllable(s) (SPEC.md §4.2, candidate)",
        score.lightened()
    );
}

/// How many beats of the piece the straight stack fitted: [`Score::straightened_beats`], the
/// count the explain view prints too.
pub(crate) fn straightened(score: &Score) -> usize {
    score.straightened_beats()
}

/// How many events the piece plays: every line's syllables, once for each slot it sounds in.
///
/// Not `score.events().len()`, which counts each line's material once — a looping stanza plays its
/// syllables many times, and the OSC backend sends a message for every one of them. The header
/// and the schedule must count the same piece.
fn sounded(score: &Score) -> usize {
    score
        .arrangement()
        .slots()
        .iter()
        .flat_map(prosody_core::Slot::placements)
        .map(|placement| score.placed(*placement).events().len())
        .sum()
}

/// The palette keys the header lists only when the piece has a chord lane: they colour nothing
/// else, and a pattern without the harmony mapping reads as it did before they existed.
const LISTED_WITH_CHORDS: &[&str] = &["chord_sound", "chord_gain"];

/// The palette keys the header lists only when they are not at their default (see [`header`]).
const LISTED_WHEN_SET: &[&str] = &[
    "bass_octave",
    "melody_octave",
    "density_level",
    "ride_fallback",
];

/// How many lines sound in each slot of the arrangement, in slot order: every placement that plays
/// at least one event — a stated line, a stanza looping under the next, a held line, a call.
fn lines_sounding(score: &Score) -> Vec<usize> {
    score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| {
            slot.placements()
                .iter()
                .filter(|placement| !score.placed(**placement).events().is_empty())
                .count()
        })
        .collect()
}

/// The level a bar of `lines` lines takes at this density exponent: `lines^-exponent`, and exactly
/// 1 for a bar of one line or none. A value only (`CLAUDE.md` §4).
fn level_for(lines: usize, exponent: f64) -> f64 {
    if lines <= 1 {
        return 1.0;
    }
    let lines = f64::from(u32::try_from(lines).unwrap_or(u32::MAX));
    lines.powf(-exponent)
}

/// Each slot's level under the palette's `density_level`, in slot order — every 1 at zero.
///
/// # Why this is palette (invariant I3)
///
/// It is the one palette value that reads the score's structure: how many lines a bar sounds.
/// It reads it and changes only gain *values* — every event, onset and pattern step stays where it
/// was, and a bar of one line is untouched — the way a mixing engineer rides the faders in a dense
/// chorus without touching the arrangement. The I3 arrangement property varies it. It was asked
/// for because a villanelle's bars of three lines drove the page's master soft clipper (PLAN.md,
/// *sound and voice, step 1*).
///
/// What it costs, disclosed in `SPEC.md` §5: a line's level in a bar it shares now depends on how
/// many *other* lines share it, so an edit that changes how many lines sound in a bar — a stanza
/// that loops longer — changes the level of another stanza's line in that bar. Levels only; its
/// events, and its voice in the score, are untouched, which is what I2's property tests check.
fn density(score: &Score, style: &Style) -> Vec<f64> {
    let exponent = style.mix().density_level().as_f64();
    if style.mix().density_level().is_off() {
        return vec![1.0; score.arrangement().slots().len()];
    }
    lines_sounding(score)
        .into_iter()
        .map(|lines| level_for(lines, exponent))
        .collect()
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
    // Keys added after the corpus was recorded are listed only when they are set away from their
    // default, so a pattern that does not use them reads exactly as it did before they existed.
    let defaults = Style::default().describe();
    let has_chords = score.chords().next().is_some();
    for (key, value) in style.describe() {
        if LISTED_WHEN_SET.contains(&key) && defaults.get(key) == Some(&value) {
            continue;
        }
        if LISTED_WITH_CHORDS.contains(&key) && !has_chords {
            continue;
        }
        let _ = writeln!(out, "//   {key} = {value}");
    }
    // The translation from the palette's words to Strudel's sounds, so `karplus` in a header is
    // not a mystery in the output.
    let ride = ride_stand_in(style).map_or_else(
        || "ride".to_owned(),
        |bank| {
            format!(
                "{} in place of the ride, which {bank} does not have,",
                style.ride_fallback().spoken()
            )
        },
    );
    let (pulse, ornament) = match style.kit() {
        Kit::Machine => ("kick", "hat"),
        Kit::Ride => (ride.as_str(), "snare"),
    };
    // Stacking every other degree makes a seventh chord only in a seven-note scale; elsewhere it
    // makes something else, and the header names what is heard rather than what was asked for.
    let scale = style.scale();
    let chords = match style.voicing() {
        Voicing::Note => String::new(),
        Voicing::Seventh if scale.len() == 7 => {
            " as seventh chords (each rhyme word's degree with \
             the 3rd, 5th and 7th above it in the scale)"
                .to_owned()
        }
        Voicing::Seventh => format!(
            " as four-tone chords on every other degree of {} (not sevenths: the scale has {} \
             notes, not 7)",
            scale.name(),
            scale.len()
        ),
    };
    let _ = writeln!(
        out,
        "//   plays: drums from {} ({pulse} on the foundation, {ornament} on the flourish), melody on {}{chords}, bass on {}",
        strudel_bank(style.bank()),
        strudel_instrument(style.melody(), Pitched::Melody),
        strudel_instrument(style.bass(), Pitched::Bass)
    );
    if has_chords {
        chords_header(out, score, style);
    }
    bass_header(out, score, has_chords);
    pulse_header(out, score, style);
    stack_header(out, score);
    function_header(out, score);
    let humanise = style.mix().humanise();
    if !humanise.is_off() {
        let _ = writeln!(
            out,
            "//   humanise: each syllable up to {} s late and {} softer, drawn from its own line's seed",
            fmt_param(humanise.as_f64() * MAX_LATE),
            fmt_param(humanise.as_f64() * MAX_SOFTEN)
        );
    }
    if !style.mix().density_level().is_off() {
        // Which bars were turned down, and by how much: the mapping from "lines sounding" to a
        // level is the one place this palette value reads the score, so it is spelled out.
        let exponent = style.mix().density_level().as_f64();
        let mut counts: Vec<usize> = lines_sounding(score);
        counts.sort_unstable();
        counts.dedup();
        let factors: Vec<String> = counts
            .iter()
            .map(|n| format!("{n}: ×{}", fmt_param(level_for(*n, exponent))))
            .collect();
        let _ = writeln!(
            out,
            "//   density: every gain in a bar divided by n^{}, n = the lines sounding in it ({})",
            fmt_param(exponent),
            factors.join(", ")
        );
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

/// The header's line for a walking bass, which names its variant. The bass by vowel height is the
/// piece as it shipped, and its header says nothing new.
fn bass_header(out: &mut String, score: &Score, has_chords: bool) {
    match score.bass() {
        Variant::Syllables => {
            let _ = writeln!(
                out,
                "//   walking bass (mapping bass = syllables): a note on every foundation \
                 syllable — a stressed one on its vowel-height note, the ones between by scale \
                 step, the last before each a semitone below it (SPEC.md §4.6, candidate W1); \
                 one walk a bar, the bar's foundation line's"
            );
        }
        Variant::Feet => {
            let _ = writeln!(
                out,
                "//   walking bass (mapping bass = feet): a note a foot on the beat — the chord \
                 tone the foot's vowel height chooses ({}), a scale step where it would repeat \
                 (SPEC.md §4.6, candidate W2); one walk a bar, the bar's foundation line's",
                if has_chords {
                    "of the bar's chord, or the scale's where a bar has none"
                } else {
                    "root, third, fifth of the scale: no harmony"
                }
            );
        }
        _ => {}
    }
}

/// One voice: every syllable it sounds, and what §4.3 and §4.4 do to each.
fn rhythm_layer(
    score: &Score,
    style: &Style,
    strays: &BTreeMap<EventKey, Stray>,
    density: &[f64],
    voice: Voice,
) -> Vec<(String, String)> {
    let events: Vec<&Event> = score.events_of(voice.id()).collect();
    let sample = sample_for(style, voice.layer());
    let name = format!(
        "voice {} — {}",
        voice.id().get(),
        match voice.layer() {
            Layer::Foundation => "foundation",
            Layer::Flourish => "flourish",
        }
    );

    let mut layers = Vec::new();
    for lane in Lane::all(score, &events, false) {
        let mut layer = format!("  s(\"{}\")", grid(lane, &|_| sample.to_owned(), "~"));
        // §4.2: stress becomes accent.
        let _ = write!(layer, ".gain(\"{}\")", gains(lane, density));
        // §4.4: distinctiveness drives brightness, attack and how wet the event sits.
        let _ = write!(layer, ".lpf(\"{}\")", cutoffs(lane, style));
        let _ = write!(layer, ".attack(\"{}\")", attacks(lane));
        let _ = write!(layer, ".room(\"{}\")", rooms(lane, style));
        // §4.3: words of one morphological family are one voice with varied parameters — the row
        // the spec says earns the project. Distance from the root drives the modulation.
        let _ = write!(layer, ".crush(\"{}\")", crushes(lane));
        // §4.3: alliteration and assonance are "timbral continuity — same sample bank".
        if let Some(colours) = vowels(lane) {
            let _ = write!(layer, ".vowel(\"{colours}\")");
        }
        let _ = write!(layer, ".bank(\"{}\")", strudel_bank(style.bank()));
        layer.push_str(&humanised(lane, style, strays));
        if !style.mix().drive().is_off() {
            // Soft saturation, at most half of Strudel's `shape`: past that it is distortion, not
            // warmth, and it gets loud.
            let _ = write!(
                layer,
                ".shape({})",
                fmt_param(style.mix().drive().as_f64() * 0.5)
            );
        }
        layer.push_str(&bus(style, style.mix().drums_gain(), 1.0));
        layer.push_str(&colour(match voice.layer() {
            Layer::Foundation => Colour::Foundation,
            Layer::Flourish => Colour::Flourish,
        }));
        layers.push((layer, lane.label(&name)));
    }
    layers
}

/// Which sample a voice plays.
///
/// `SPEC.md` §4.1 turns the layer into instrumentation: "low variance → periodic → foundation
/// layer (drums, bass, ostinato)" and "high variance → aperiodic → flourish layer (fills, stabs,
/// one-shots, ornament)". Which *sounds* those are is palette; that a foundation is the pulse and
/// a flourish ornaments it is structure, and comes from the score.
///
/// The [`Kit`] is that palette choice: the drum machine's kick and hat, or a jazz section's ride
/// keeping time and snare comping round it. Either way the foundation is the time and the flourish
/// the ornament.
///
/// Not every bank has a ride: the TR-808 and TR-707 in the sample map have none. Asking for one
/// there makes Strudel log "sound not found" on every hit and the whole pulse layer go silent, so a
/// palette choice would in effect remove events — which is not palette's to do (I3) and leaves
/// sound nobody can trace (I4). Such a bank plays the palette's [`RideFallback`] instead — its
/// open hat by default, the drum machine's usual stand-in for a ride, or its crash — and the
/// pattern's header says so ([`ride_stand_in`]).
fn sample_for(style: &Style, layer: Layer) -> &'static str {
    match (style.kit(), layer) {
        (Kit::Machine, Layer::Foundation) => "bd",
        (Kit::Machine, Layer::Flourish) => "hh",
        (Kit::Ride, Layer::Foundation) => {
            if bank_has(strudel_bank(style.bank()), RIDE) {
                RIDE
            } else {
                stand_in(style.ride_fallback())
            }
        }
        (Kit::Ride, Layer::Flourish) => "sd",
    }
}

/// The ride cymbal's sample name in the drum-machine sample map.
const RIDE: &str = "rd";

/// What a bank without a ride plays in its place — the palette's [`RideFallback`]: the open hat or
/// the crash, which every bank in [`BANK_SAMPLES`] has.
const fn stand_in(fallback: RideFallback) -> &'static str {
    match fallback {
        RideFallback::OpenHat => "oh",
        RideFallback::Crash => "cr",
    }
}

/// `Some(bank)` when the style asks for a ride its bank does not have, so the header can say which
/// drum stands in — `CLAUDE.md` §6: a fallback must be visible.
fn ride_stand_in(style: &Style) -> Option<&'static str> {
    let bank = strudel_bank(style.bank());
    (style.kit() == Kit::Ride && !bank_has(bank, RIDE)).then_some(bank)
}

/// The samples each bank has in the sample map the page loads (`tidal-drum-machines.json` on
/// `strudel.b-cdn.net`, checked 2026-10-22). Only what a kit can ask for has to be right here; the
/// rest is listed so the table reads as the map does.
const BANK_SAMPLES: &[(&str, &[&str])] = &[
    (
        "RolandTR909",
        &[
            "bd", "cp", "cr", "hh", "ht", "lt", "mt", "oh", "rd", "rim", "sd",
        ],
    ),
    (
        "RolandTR808",
        &[
            "bd", "cb", "cp", "cr", "hh", "ht", "lt", "mt", "oh", "perc", "rim", "sd", "sh",
        ],
    ),
    (
        "RolandTR707",
        &[
            "bd", "cb", "cp", "cr", "hh", "ht", "lt", "mt", "oh", "rim", "sd", "tb",
        ],
    ),
    (
        "AkaiLinn",
        &[
            "bd", "cb", "cp", "cr", "hh", "ht", "lt", "mt", "oh", "rd", "sd", "sh", "tb",
        ],
    ),
    (
        "CasioRZ1",
        &[
            "bd", "cb", "cp", "cr", "hh", "ht", "lt", "mt", "rd", "rim", "sd",
        ],
    ),
    (
        "RolandR8",
        &[
            "bd", "cb", "cp", "cr", "hh", "ht", "lt", "mt", "oh", "perc", "rd", "rim", "sd", "sh",
            "tb",
        ],
    ),
];

/// Whether a Strudel bank has a sample of that name.
fn bank_has(bank: &str, sample: &str) -> bool {
    BANK_SAMPLES
        .iter()
        .any(|(name, samples)| *name == bank && samples.contains(&sample))
}

/// One of the lines a layer sounds at the same time, as one pattern of its own.
///
/// A layer can sound several lines in one bar — a stanza looping under the next, a held line under
/// its answer, a call under everything. An earlier version stacked them inside each pattern with
/// mini-notation's `,`. That is right for `s("…")` alone and wrong for everything chained after
/// it: Strudel matches each parameter pattern to the events by time, so an event in a bar of two
/// stacked lines met *both* lines' gains, both cutoffs, both crushes — and became one event per
/// combination. Two lines and five parameters made one syllable thirty-two drum hits; a bass note
/// under a held line sounded thirty-two times over and clipped the page at twelve times full
/// scale. Events that are not in the score are invariant I4 broken, audibly.
///
/// So each simultaneous line is its own stack element: lane 0 is the first line sounding in every
/// bar, lane 1 the second where there is one, and so on, each a pattern with no `,` in it whose
/// parameters therefore meet exactly one event each.
#[derive(Clone, Copy)]
pub(crate) struct Lane<'a> {
    score: &'a Score,
    events: &'a [&'a Event],
    index: usize,
    /// Whether each event fills its whole beat instead of its syllable's share of it: the walking
    /// bass's W2, whose note is its foot's (`prosody_core::bass`). Every pattern of the lane is
    /// written the same way, so its parameters still meet exactly one event each.
    whole_beats: bool,
    /// Whether this is the bass, which sounds only where [`Score::sounds_bass`] says: under a
    /// walking bass, one walk a bar — the bar's foundation line's — rather than one per line.
    bass: bool,
}

impl<'a> Lane<'a> {
    /// Every lane a layer of these events needs: as many as the most lines it sounds in one bar.
    /// `is_bass` counts only the lines whose bass sounds there ([`Score::sounds_bass`]).
    pub(crate) fn all(score: &'a Score, events: &'a [&'a Event], is_bass: bool) -> Vec<Self> {
        let bars: std::collections::BTreeSet<usize> =
            events.iter().map(|event| event.bar().get()).collect();
        let deepest = score
            .arrangement()
            .slots()
            .iter()
            .map(|slot| {
                slot.placements()
                    .iter()
                    .filter(|placement| {
                        (!is_bass || score.sounds_bass(slot, placement))
                            && bars.contains(&placement.bar().get())
                            && events.iter().any(|event| {
                                event.bar() == placement.bar()
                                    && placement.part().covers(event.span())
                            })
                    })
                    .count()
            })
            .max()
            .unwrap_or(0)
            .max(1);
        (0..deepest)
            .map(|index| Self {
                score,
                events,
                index,
                whole_beats: false,
                bass: is_bass,
            })
            .collect()
    }

    /// The same lane, each event filling its whole beat.
    const fn whole_beats(mut self) -> Self {
        self.whole_beats = true;
        self
    }

    /// The comment that names it, after the layer's own.
    fn label(self, layer: &str) -> String {
        if self.index == 0 {
            layer.to_owned()
        } else {
            format!("{layer} (lane {})", self.index + 1)
        }
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
fn grid(lane: Lane<'_>, value: &dyn Fn(&Event) -> String, absent: &str) -> String {
    grid_by_slot(lane, &|event, _| value(event), absent)
}

/// [`grid`], with each value also told which slot of the arrangement it sounds in — for the one
/// parameter that differs between the passes of a line: its level under `density_level`.
pub(crate) fn grid_by_slot(
    lane: Lane<'_>,
    value: &dyn Fn(&Event, usize) -> String,
    absent: &str,
) -> String {
    let (score, events) = (lane.score, lane.events);
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
    // loops or yields (`prosody_core::arrange`). A slot is one element of the piece's sequence,
    // lasting as long as the pulse says ([`sequence`]); a line divides it into the slot's beats,
    // or its own when it has more — which is the bar-locked fitting. Lines sounding
    // together are *not* stacked with `,` inside the pattern: see [`Lane`]. This pattern carries
    // the `lane.index`-th of them, and rests where a bar has fewer.
    let mut bars = Vec::new();
    for (index, slot) in score.arrangement().slots().iter().enumerate() {
        let value = |event: &Event| value(event, index);
        let lines: Vec<String> = slot
            .placements()
            .iter()
            .filter(|placement| !lane.bass || score.sounds_bass(slot, placement))
            .filter_map(|placement| {
                // A whole line, or a call lifted out of one: its own events, its beats counted
                // from where it starts. A line's width comes from the score, not from the events
                // in *this* pattern, so the melody and the rhythm divide it the same way and a
                // note lands on its own syllable. Each event's place in its beat is the one it
                // has *in this slot* — under the straight stack, fitted to the host where the two
                // would cross (`Score::placed_in`, which the OSC schedule and the scorer read too).
                let placed = score.placed_in(slot, *placement);
                let present: BTreeMap<usize, Vec<(&Event, Subdivision)>> = by_bar
                    .get(&placement.bar().get())?
                    .iter()
                    .filter_map(|(beat, syllables)| {
                        let covered: Vec<(&Event, Subdivision)> = syllables
                            .iter()
                            .copied()
                            .filter(|event| placement.part().covers(event.span()))
                            .map(|event| (event, placed.subdivision(event)))
                            .collect();
                        (!covered.is_empty())
                            .then(|| (beat.saturating_sub(placed.first_beat()), covered))
                    })
                    .collect();
                // A line given less than the whole bar keeps one beat per foot and rests for the
                // rest of the cycle; one given all of it fills the cycle with its own beats,
                // squeezed or stretched (`Slot::room_for`).
                let steps = if slot.room_for(placed.beats()) < slot.beats() {
                    slot.beats()
                } else {
                    placed.beats()
                };
                (!present.is_empty())
                    .then(|| line(&present, steps, &value, absent, lane.whole_beats))
            })
            .collect();
        bars.push(
            lines
                .into_iter()
                .nth(lane.index)
                .unwrap_or_else(|| "~".to_owned()),
        );
    }

    // The piece: one element per slot, each lasting as long as the pulse says (`sequence`).
    sequence(score, &bars)
}

/// One line's events in this pattern, as the steps of one cycle: a step per beat, subdivided by
/// its syllables.
fn line(
    present: &BTreeMap<usize, Vec<(&Event, Subdivision)>>,
    beats: usize,
    value: &dyn Fn(&Event) -> String,
    absent: &str,
    whole_beats: bool,
) -> String {
    let mut steps = Vec::new();
    for beat in 0..beats {
        match present.get(&beat) {
            None => steps.push(absent.to_owned()),
            // A note that is its whole foot (W2) holds the beat, whichever syllable it rides on.
            Some(syllables) if whole_beats => steps.push(
                syllables
                    .first()
                    .map_or_else(|| absent.to_owned(), |(e, _)| value(e)),
            ),
            // A foot divided into one part is the whole beat. More than one, and the beat is
            // subdivided — *even when this pattern has only one value for it*, which is the
            // melody's usual case: a rhyme word is the second syllable of a final iambic foot,
            // so its note belongs in the second half of the beat, not across the whole of it.
            Some(syllables)
                if syllables.len() == 1
                    && syllables.first().is_some_and(|(_, at)| at.of() == 1) =>
            {
                steps.push(
                    syllables
                        .first()
                        .map_or_else(|| absent.to_owned(), |(e, _)| value(e)),
                );
            }
            Some(syllables) => steps.push(subdivided(syllables, value, absent)),
        }
    }
    if steps.is_empty() {
        "~".to_owned()
    } else {
        steps.join(" ")
    }
}

/// One beat subdivided by its syllables: `[a b]`, `[a b c]`.
///
/// A foot's syllables subdivide its beat. Where this pattern has no value for one of them — an
/// unpitched syllable in the melody — it rests, so the ones that do sound keep their place inside
/// the foot. A syllable lasting more than one part — only a stacked line the straight stack fitted
/// to its host (`prosody_core::stack`) — is weighted with mini-notation's `@`: three syllables
/// over a host's pair are `[a@2 b c]`, onsets at 0, ½ and ¾. A rest under a long syllable this
/// pattern does not sound stays a run of `~`, which lasts the same.
fn subdivided(
    syllables: &[(&Event, Subdivision)],
    value: &dyn Fn(&Event) -> String,
    absent: &str,
) -> String {
    let width = syllables
        .first()
        .map_or(syllables.len(), |(_, at)| at.of())
        .max(syllables.len());
    // `None` is a part covered by a long syllable before it, written as that syllable's weight.
    let mut inner: Vec<Option<String>> = vec![Some(absent.to_owned()); width];
    for (event, at) in syllables {
        let parts = at.parts().max(1);
        if let Some(slot) = inner.get_mut(at.index()) {
            *slot = Some(if parts > 1 {
                format!("{}@{parts}", value(event))
            } else {
                value(event)
            });
        }
        for covered in at.index() + 1..at.index() + parts {
            if let Some(slot) = inner.get_mut(covered) {
                *slot = None;
            }
        }
    }
    let written: Vec<String> = inner.into_iter().flatten().collect();
    format!("[{}]", written.join(" "))
}

/// `SPEC.md` §4.2: stress becomes accent — times the bar's level under `density_level`, which is 1
/// unless the palette asks levels to follow density ([`density`]).
fn gains(lane: Lane<'_>, density: &[f64]) -> String {
    gains_of(lane, density, &|event| event.velocity())
}

/// [`gains`], with the stress read from `velocity` — for the bass, whose W2 note takes its foot's
/// stressed syllable's accent rather than the syllable it rides on.
fn gains_of(
    lane: Lane<'_>,
    density: &[f64],
    velocity: &dyn Fn(&Event) -> prosody_core::Velocity,
) -> String {
    grid_by_slot(
        lane,
        &|event, slot| {
            let level = density.get(slot).copied().unwrap_or(1.0);
            fmt_param(velocity(event).as_f64() * level)
        },
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes brightness.
///
/// 400 Hz up to the palette's high cut (4400 Hz by default) spans dull to bright without ever
/// closing the filter completely — a silent event would be an event that cannot be traced to its
/// word. The high cut scales the range rather than clipping it, so under a darker palette a
/// striking word is still brighter than a plain one (§9: tuning the ranges is palette work).
fn cutoffs(lane: Lane<'_>, style: &Style) -> String {
    let span = f64::from(style.mix().high_cut()) - 400.0;
    grid(
        lane,
        &|event| fmt_param(400.0 + event.sharpness().as_f64() * span),
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes attack time, inverted — a sharper word strikes faster.
fn attacks(lane: Lane<'_>) -> String {
    grid(
        lane,
        &|event| fmt_param(0.04 * (1.0 - event.sharpness().as_f64())),
        "~",
    )
}

/// `SPEC.md` §4.4: distinctiveness becomes dry/wet — a striking word rings in more space.
///
/// The palette's reverb raises the floor the mapping stands on, at half strength for the drums so
/// a kick does not wash out; the rise with distinctiveness is the same under every palette.
fn rooms(lane: Lane<'_>, style: &Style) -> String {
    let floor = style.mix().reverb().as_f64() * 0.5;
    grid(
        lane,
        &|event| fmt_param(floor + event.sharpness().as_f64() * 0.4),
        "~",
    )
}

/// `SPEC.md` §4.3's morphological row: "same voice, varied parameters".
///
/// Distance from the family's root becomes bit-crush depth, so "burn", "burnt" and "burning" are
/// audibly one instrument at three degrees of erosion. 16 bits is untouched; each phoneme of
/// difference takes two away, to a floor of 4 — still the same instrument, never a different one.
fn crushes(lane: Lane<'_>) -> String {
    grid(
        lane,
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
fn vowels(lane: Lane<'_>) -> Option<String> {
    if !lane.events.iter().any(|event| event.bank().is_some()) {
        return None;
    }
    Some(grid(
        lane,
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

/// A pitched layer: the melody (every rhyme word, `SPEC.md` §4.5) or the bass (every
/// primary-stressed syllable of a foundation line, by its vowel — `prosody_core::lower`).
///
/// `None` when nothing in the poem is pitched that way.
///
/// Built on the same [`grid`] as the rhythm, and that is the point: an earlier version derived its
/// own subdivision from how many pitched events happened to fall in a beat, so a rhyme word in the
/// second half of a final iambic foot sounded half a foot before its own drum hit — in every line
/// of every iambic poem. A pitched event is the same [`Event`] as its drum hit, so it takes the
/// same stray too and the note stays on its syllable.
fn pitched_layer(
    score: &Score,
    style: &Style,
    strays: &BTreeMap<EventKey, Stray>,
    density: &[f64],
    role: Pitched,
    tempo: u16,
) -> Vec<(String, String)> {
    let pitched: Vec<&Event> = score
        .events()
        .iter()
        .filter(|event| role.plays(event))
        .collect();
    if pitched.is_empty() {
        return Vec::new();
    }
    // Which octave a role sits in is palette (`SPEC.md` §5, `bass_octave` and `melody_octave`):
    // it moves every note of the role by whole octaves and no event. By default the bass is in the
    // second (MIDI 36), where a bass instrument has its body, and the melody two octaves above
    // (MIDI 60), where keys and mallets sing. Before the roles were split both sat at 36, so the
    // rhyme melody was in the bass.
    let base = match role {
        Pitched::Melody => style.melody_octave().midi_c(),
        Pitched::Bass => style.bass_octave().midi_c(),
    };

    // The melody may be voiced as chords (`Voicing`, palette — its doc argues it against I3); the
    // bass is always one note, the root a chord would stand on.
    let voicing = match role {
        Pitched::Melody => style.voicing(),
        Pitched::Bass => Voicing::Note,
    };
    let note = |event: &Event| match role {
        Pitched::Melody => melody_note(event, style, base, voicing),
        Pitched::Bass => event
            .bass()
            .map_or_else(|| "~".to_owned(), |note| bass_note(note, style).to_string()),
    };
    let mix = style.mix();
    let instrument = strudel_instrument(
        match role {
            Pitched::Melody => style.melody(),
            Pitched::Bass => style.bass(),
        },
        role,
    );
    let name = match role {
        Pitched::Melody => "the melody — a note per rhyme word",
        Pitched::Bass => bass_name(score.bass()),
    };

    let mut layers = Vec::new();
    for lane in Lane::all(score, &pitched, role == Pitched::Bass) {
        // W2's note is its foot: it holds the beat (`prosody_core::bass`).
        let lane = if role == Pitched::Bass && score.bass() == Variant::Feet {
            lane.whole_beats()
        } else {
            lane
        };
        let mut layer = format!("  note(\"{}\")", grid(lane, &note, "~"));
        let _ = write!(layer, ".sound(\"{instrument}\")");
        let gain = match role {
            Pitched::Melody => gains(lane, density),
            Pitched::Bass => gains_of(lane, density, &|event| {
                event
                    .bass()
                    .map_or_else(|| event.velocity(), BassNote::velocity)
            }),
        };
        let _ = write!(layer, ".gain(\"{gain}\")");
        // §4.4 reaches the pitched layers too — the rhyme word is the most prominent event in the
        // piece, and an earlier version was the only layer distinctiveness never touched.
        let _ = write!(layer, ".lpf(\"{}\")", cutoffs(lane, style));
        layer.push_str(&humanised(lane, style, strays));
        // A struck note rings past its syllable rather than stopping dead at the next one: a
        // soundfont's own release is a hundredth of a second, which made every note a click.
        let (release, room) = match role {
            Pitched::Melody => (0.4, mix.reverb().as_f64()),
            // The bass sits nearly dry, so the low end stays clear.
            Pitched::Bass => (0.15, mix.reverb().as_f64() * 0.3),
        };
        let _ = write!(
            layer,
            ".release({}).room({})",
            fmt_param(release),
            fmt_param(room)
        );
        match role {
            Pitched::Melody if !mix.delay().is_off() => {
                // An echo a dotted eighth behind — three quarters of a beat at this piece's tempo,
                // so it falls between the syllables rather than on them.
                let _ = write!(
                    layer,
                    ".delay({}).delaytime({}).delayfeedback(0.350)",
                    fmt_param(mix.delay().as_f64()),
                    fmt_param(0.75 * 60.0 / f64::from(tempo.max(1)))
                );
            }
            Pitched::Bass if !mix.drive().is_off() => {
                let _ = write!(layer, ".shape({})", fmt_param(mix.drive().as_f64() * 0.5));
            }
            _ => {}
        }
        layer.push_str(&bus(
            style,
            match role {
                Pitched::Melody => mix.melody_gain(),
                Pitched::Bass => mix.bass_gain(),
            },
            chord_share(voicing),
        ));
        layer.push_str(&colour(match role {
            Pitched::Melody => Colour::Melody,
            Pitched::Bass => Colour::Bass,
        }));
        layers.push((layer, lane.label(name)));
    }
    layers
}

/// One melody event's note — or chord, under the seventh voicing — as a mini-notation step: its
/// rhyme's degree through the style's scale from `base`, detuned by how far the rhyme is off.
fn melody_note(event: &Event, style: &Style, base: i32, voicing: Voicing) -> String {
    event.pitch().map_or_else(
        || "~".to_owned(),
        |pitch| {
            let tones: Vec<String> = voicing
                .degrees()
                .iter()
                .map(|above| {
                    // Degree through the style's scale, from the style's root. Which note that
                    // is, is palette; that two lines take the *same* note is the poem's doing.
                    // A chord tone is the same degree plus every other step above it, in the
                    // same scale — nothing from any other event.
                    let semitone = base
                        + i32::from(style.root().semitones())
                        + style.scale().degree(pitch.degree().get() + above);
                    // §4.5: "perfect rhyme → exact pitch; slant rhyme → detune or neighbour
                    // tone." The mapping interpolates between the two: a whole step off at a
                    // total mismatch, so a half-match lands exactly on the neighbour. A chord
                    // is detuned as a whole, so a slant rhyme's chord is off as one.
                    if pitch.detune().is_exact() {
                        format!("{semitone}")
                    } else {
                        fmt_param(f64::from(semitone) + pitch.detune().as_f64() * 2.0)
                    }
                })
                .collect();
            // One tone is the note itself. Several are one mini-notation step stacked with
            // `,` — the only `,` the emitter writes inside a pattern. It cannot multiply
            // events the way stacked *lines* did (see [`Lane`]): every parameter pattern
            // still has exactly one value at this step, so each tone takes that value, and
            // the one event sounds as its chord, once.
            match tones.as_slice() {
                [one] => one.clone(),
                _ => format!("[{}]", tones.join(",")),
            }
        },
    )
}

/// The layer comment that names the bass, by the variant that made it.
const fn bass_name(variant: Variant) -> &'static str {
    match variant {
        Variant::Syllables => {
            "the bass — walking every foundation syllable: stressed by vowel height, steps between, \
             a semitone approach before each (mapping bass = syllables)"
        }
        Variant::Feet => {
            "the bass — walking a note a foot on the beat: the chord tone the foot's vowel height \
             chooses, a step where it would repeat (mapping bass = feet)"
        }
        _ => "the bass — every stressed foundation vowel, by its height",
    }
}

/// One bass note's MIDI number: its steps above its root through the style's scale, from the
/// style's root note in the bass octave, a semitone lower for W1's approach tone.
///
/// The root is a chord's under W2 and is folded into one octave of the scale, as the chord lane
/// folds it (`chord_note`), so the walk stays in the bass's register while the harmony moves;
/// a step below the root sounds below it ([`crate::style::Scale::step`]). Register is palette.
fn bass_note(note: BassNote, style: &Style) -> i32 {
    let scale = style.scale();
    let root = note.root().get() % scale.len().max(1);
    let step = i64::try_from(root).unwrap_or(0) + i64::from(note.step().get());
    let below = match note.chromatic() {
        Chromatic::Diatonic => 0,
        Chromatic::SemitoneBelow => 1,
    };
    style.bass_octave().midi_c() + i32::from(style.root().semitones()) + scale.step(step) - below
}

/// The scale-degree offsets a chord of the chord lane stacks on its root: the triad under
/// [`Voicing::Note`] — one tone would be no chord — and the seventh chord under
/// [`Voicing::Seventh`], every other degree of the style's scale either way.
const fn chord_tones(voicing: Voicing) -> &'static [usize] {
    match voicing {
        Voicing::Note => &[0, 2, 4],
        Voicing::Seventh => &[0, 2, 4, 6],
    }
}

/// One chord's tones, as a mini-notation stack: its root folded into one octave of the style's
/// scale, then [`chord_tones`] above it, an octave below the melody's.
///
/// Folding is register, so palette: the core counts V of degree 5 as degree 9, which in a
/// seven-note scale is degree 2 an octave up; the chord lane sounds it in the same octave as every
/// other chord, so the harmony moves by its roots rather than climbing with the degree numbers.
fn chord_note(chord: Chord, style: &Style) -> String {
    let scale = style.scale();
    let root = chord.root().get() % scale.len().max(1);
    let base = style.melody_octave().midi_c() - 12 + i32::from(style.root().semitones());
    let tones: Vec<String> = chord_tones(style.voicing())
        .iter()
        .map(|above| format!("{}", base + scale.degree(root + above)))
        .collect();
    format!("[{}]", tones.join(","))
}

/// The Strudel sound the chord lane plays: a soundfont as named, a synth name as the melody would
/// play it — a chord is keys, not a bass.
fn chord_instrument(style: &Style) -> &'static str {
    strudel_instrument(style.chord_sound(), Pitched::Melody)
}

/// The chord lane: one chord a slot, held for the bar, wherever the arrangement plays a foundation
/// line the harmony mapping gave a chord (`SPEC.md` §4.6, [`Score::harmony_in`]); a rest
/// elsewhere. A stanza's last bar under H2 is two halves (`Harmony::Turning`): its own chord, or a
/// rest, then the turnaround — `[a b]` in the slot's element, so the change lands exactly on the
/// bar's half under either pulse.
///
/// `None` when the score has no chord — the mapping is off, or nothing in the poem earned one — so
/// a piece without the harmony is byte for byte what it was.
///
/// The slots are laid end to end by [`sequence`], as every other pattern's bars are: under the
/// steady pulse a chord lasts its bar's beats, not one cycle — one beat — as it did when this lane
/// wrote its own `<…>`.
///
/// Which slots have a chord, and on which degree, is the score's. Everything here is palette: the
/// instrument (`chord_sound`), how many tones ([`chord_tones`], from the [`Voicing`]), the
/// register (an octave below the melody, between it and the bass), the level (`chord_gain`,
/// shared between the tones like the melody's, times the bar's density level) and a soft
/// envelope, so a held chord swells in under the syllables rather than striking with them.
fn chord_layer(score: &Score, style: &Style, density: &[f64]) -> Option<(String, String)> {
    let harmonies: Vec<Option<Harmony>> = score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| score.harmony_in(slot))
        .collect();
    if harmonies
        .iter()
        .all(|harmony| harmony.is_none_or(|harmony| harmony.chords().next().is_none()))
    {
        return None;
    }
    // One element per slot: `~`, one chord, or a turning bar's two halves.
    let element = |harmony: &Option<Harmony>, value: &dyn Fn(Chord) -> String| -> String {
        let one = |chord: Option<Chord>| chord.map_or_else(|| "~".to_owned(), value);
        match harmony {
            Some(harmony @ Harmony::Turning { turn, .. }) => {
                format!("{} {}", one(harmony.chord()), value(*turn))
            }
            Some(harmony) => one(harmony.chord()),
            None => "~".to_owned(),
        }
    };
    let notes = sequence(
        score,
        &harmonies
            .iter()
            .map(|harmony| element(harmony, &|chord| chord_note(chord, style)))
            .collect::<Vec<_>>(),
    );
    let gains = sequence(
        score,
        &harmonies
            .iter()
            .enumerate()
            .map(|(slot, harmony)| {
                let level = fmt_param(density.get(slot).copied().unwrap_or(1.0));
                element(harmony, &|_| level.clone())
            })
            .collect::<Vec<_>>(),
    );
    let mix = style.mix();
    let mut layer = format!(
        "  note(\"{notes}\").sound(\"{}\").gain(\"{gains}\")",
        chord_instrument(style)
    );
    let _ = write!(
        layer,
        ".attack(0.050).release(0.600).lpf({}).room({})",
        mix.high_cut(),
        fmt_param(mix.reverb().as_f64())
    );
    let tones = u32::try_from(chord_tones(style.voicing()).len()).unwrap_or(1);
    layer.push_str(&bus(
        style,
        mix.chord_gain(),
        1.0 / f64::from(tones.max(1)).sqrt(),
    ));
    layer.push_str(&colour(Colour::Chords));
    Some((
        layer,
        "the chords — one a bar of a foundation stanza (a stanza's last bar under cadence: its \
         own, then the turnaround), from the harmony mapping"
            .to_owned(),
    ))
}

/// The header's account of the chord lane: what plays it, and each line's chord or rest.
fn chords_header(out: &mut String, score: &Score, style: &Style) {
    let scale = style.scale();
    let heptatonic = scale.len() == 7;
    let voiced = match style.voicing() {
        Voicing::Note if heptatonic => "triads",
        Voicing::Note => "three-tone chords on every other degree",
        Voicing::Seventh if heptatonic => "seventh chords",
        Voicing::Seventh => "four-tone chords on every other degree",
    };
    let _ = writeln!(
        out,
        "//   chords: on {}, as {voiced} in {} an octave below the melody, each root folded into \
         one octave; one a bar of a foundation stanza (SPEC.md §4.6, the harmony mapping)",
        chord_instrument(style),
        scale.name()
    );
    // The core counts a chord's function in seven steps — V is four degrees above, IV three — so
    // the numerals name a dominant and a subdominant only in a seven-note scale. In any other
    // scale the same steps land elsewhere (in a five-note one, "V" folds onto the degree just
    // below its root), and the header says so rather than letting the numerals mislead.
    if !heptatonic {
        let _ = writeln!(
            out,
            "//   chords: {} has {} notes, not 7, so the numerals below count steps of it, not \
             functions: V is not a dominant nor IV a subdominant here",
            scale.name(),
            scale.len()
        );
    }
    // Line by line, counting from 1 as a reader does: `V/4` is V on degree 4, `-` a rest, and
    // `I/0+V/8` a stanza's last bar split — its own chord, then the turnaround from its half.
    // Flourish lines carry no harmony and are left out.
    let name = |chord: Chord| format!("{}/{}", chord.function().numeral(), chord.root().get());
    let lines: Vec<String> = score
        .harmony()
        .iter()
        .filter_map(|harmony| match harmony {
            Harmony::Chord(chord) => Some(format!("{}:{}", chord.bar().get() + 1, name(*chord))),
            Harmony::Turning { own, turn } => Some(format!(
                "{}:{}+{}",
                turn.bar().get() + 1,
                own.map_or_else(|| "-".to_owned(), name),
                name(*turn)
            )),
            Harmony::Rest(rest) if rest.why() == Silence::Flourish => None,
            Harmony::Rest(rest) => Some(format!("{}:-", rest.bar().get() + 1)),
        })
        .collect();
    if score
        .harmony()
        .iter()
        .any(|harmony| harmony.turnaround().is_some())
    {
        let _ = writeln!(
            out,
            "//   chords: a stanza's last bar is split — its own chord, then from the bar's half V \
             of the stanza's first chord, turning round onto it (a+b below)"
        );
    }
    let _ = writeln!(out, "//   chords by line: {}", lines.join(" "));
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

    use super::{BANK_SAMPLES, Strudel, StrudelError, bank_has, sample_for, strudel_bank};
    use crate::Backend as _;
    use crate::fmt_param;
    use crate::style::{BANKS, KITS, RIDE_FALLBACKS, Style};

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
        // No closed class: every word keeps its dictionary stress under either variant.
        fn is_function_word(&self, _: &str) -> bool {
            false
        }

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
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
            .find(|token| *token != "~")
            .unwrap_or_default()
            .to_owned()
    }

    #[test]
    fn a_foot_is_a_step_and_its_syllables_subdivide_it() {
        // SPEC.md §4.2, straight through to mini-notation. "eA" is an iamb, so one step holding
        // two syllables; two of them is two steps. One bar for the line, one for the outro, each
        // two beats long: weight 2, two cycles of a beat each.
        let text = render("eA eA\n", &Style::default());
        assert_eq!(
            argument(&text, "s"),
            "<[[bd bd] [bd bd]]@2 [[bd bd] [bd bd]]@2>"
        );
    }

    #[test]
    fn stress_becomes_gain_and_all_three_levels_survive() {
        // Hand-worked: `e` unstressed is a third, `i` secondary two thirds, `A` primary full.
        // Twice: the line, then the outro.
        let text = render("eiA\n", &Style::default());
        assert_eq!(
            argument(&text, "gain"),
            "<[[0.333 0.667 1.000]]@1 [[0.333 0.667 1.000]]@1>"
        );
    }

    #[test]
    fn a_bar_a_voice_does_not_play_is_a_rest_not_a_shift() {
        // A voice must not have its later bars pulled forward into the gap, or every line after a
        // silence plays at the wrong time.
        // Voice 0 plays bar 0 and rests in bar 1 — in the statement and again in the outro.
        let text = render("bA\ntA\n", &Style::default());
        // Every bar one beat long, so weight 1 each.
        assert_eq!(argument(&text, "s"), "<bd@1 ~@1 bd@1 ~@1>", "{text}");
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
        // A perfect rhyme prints a whole semitone number; a slant one prints a fraction. `te` is
        // unstressed, so the rhyme word is the only note — no bass ahead of it.
        let perfect = render("te bAn\nte bAn\n", &Style::default());
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
        let slant = render("te bAn\nte bon\n", &Style::default());
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
        let text = render("te bAn\nte bAn\n", &style);
        // The palette's name, translated to the identifier Strudel actually knows. Passing the
        // poem's word through unchanged asked for a sample that does not exist, so §5's own
        // example header played silence.
        assert!(text.contains(".bank(\"CasioRZ1\")"), "{text}");
        assert!(text.contains(".sound(\"sawtooth\")"), "{text}");
        // D lydian, the melody's octave (MIDI 60): 60 + 2 + 0 = 62.
        assert_eq!(first_note(&text), "62");

        let other = Style::parse("+++\nroot = \"C\"\nscale = \"lydian\"\n+++").unwrap();
        let moved = render("te bAn\nte bAn\n", &other);
        assert_eq!(first_note(&moved), "60", "the root moves every note");
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
        // of the foot's two syllables is pitched, which is the case the subdivision is for. The
        // first is unstressed, so the bass (stressed foundation vowels) leaves it unpitched.
        let source = "be bAn\nre bAn\n";
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
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
            .filter(|token| *token != "~")
            .count();
        assert_eq!(
            sounding, 4,
            "one note per rhyme word, and no others — two words, two passes: {melody}"
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
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
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
                    // A bar's `@` weight is how long it lasts, not a value.
                    .map(|t| t.split('@').next().unwrap_or(t))
                    .filter(|t| !t.is_empty())
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
        // Five syllables a pass, and two passes: the statement and the outro (§4.6).
        let text = render("bA bA bA bA bA\n", &Style::default());
        assert_eq!(argument(&text, "s").split_whitespace().count(), 10);
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
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
            .filter_map(|token| token.parse().ok())
            .collect();
        // Two words, in the statement and again in the outro.
        assert_eq!(
            depths,
            [depths[0], depths[1], depths[0], depths[1]],
            "{crush}"
        );
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
        // in cycles 0 and 2, and rests in 1 and 3. In cycle 0 the bar is stanza 1's period, the
        // three iambs of its long line, so `bA` takes the first beat and rests for two; in cycle 2
        // the bar is stanza 2's one beat, which `bA` fills.
        let text = render("bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n", &Style::default());
        let first = text.lines().find(|line| line.contains("voice 0")).unwrap();
        // The outro repeats stanza 2, and stanza 1 keeps looping under it from bar 0: `hh ~`.
        // Under the steady pulse each bar lasts its beats: stanza 1's bars three, stanza 2's one.
        assert!(
            first.starts_with("  s(\"<[hh ~ ~]@3 ~@3 hh@1 ~@1 hh@1 ~@1>\")"),
            "{text}"
        );
    }

    #[test]
    fn a_call_is_its_own_cycle() {
        // The first line's voice plays the whole line in cycle 0 — two iambs, two beats — and only
        // its call, `de dA`, in cycle 1: one beat of two syllables, then a rest, because the bar
        // lasts two beats and a loop that fits keeps its pace.
        let text = render("de dA, se sA\nne nA ne nA\n", &Style::default());
        let first = text.lines().find(|line| line.contains("voice 0")).unwrap();
        assert!(
            // ...and then both again, in the outro: the line with its call silent under it, and
            // the call under bar 1.
            first.starts_with(
                "  s(\"<[[bd bd] [bd bd]]@2 [[bd bd] ~]@2 [[bd bd] [bd bd]]@2 [[bd bd] ~]@2>\")"
            ),
            "{text}"
        );
    }

    #[test]
    fn lines_sounding_together_are_separate_lanes_so_no_event_is_multiplied() {
        // Stanza 1 loops under stanza 2 within one voice's bars, so some bar sounds two lines of
        // the same layer. Stacked with `,` inside one pattern, Strudel matched every parameter
        // pattern to both lines and multiplied each event by every combination — two lines and
        // five parameters made one syllable 32 hits. Each lane is its own pattern now: no
        // pattern holds a `,`, and the hits across all drum lanes are exactly the syllables the
        // score sounds, placement by placement.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let score = score_of(source);
        let text = render(source, &Style::default());
        for line in text
            .lines()
            .filter(|l| l.starts_with("  s(") || l.starts_with("  note("))
        {
            for (index, part) in line.split('"').enumerate() {
                assert!(
                    index % 2 == 0 || !part.contains(','),
                    "a stacked pattern: {line}"
                );
            }
        }
        assert!(
            text.contains("(lane 2)"),
            "the fixture must stack lines: {text}"
        );
        let hits: usize = text
            .lines()
            .filter(|l| l.starts_with("  s("))
            .map(|l| {
                l.split('"')
                    .nth(1)
                    .unwrap()
                    .replace(['<', '>', '[', ']'], " ")
                    .split_whitespace()
                    // A bar's `@` weight is how long it lasts, not a value.
                    .map(|t| t.split('@').next().unwrap_or(t))
                    .filter(|t| !t.is_empty())
                    .filter(|t| *t != "~")
                    .count()
            })
            .sum();
        assert_eq!(hits, super::sounded(&score), "{text}");
    }

    /// The numbers inside the first `name("…")` pattern on the line that starts with `prefix`.
    fn values_on(text: &str, prefix: &str, name: &str) -> Vec<f64> {
        let line = text
            .lines()
            .find(|line| line.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix} in:\n{text}"));
        let needle = format!(".{name}(\"");
        let Some(start) = line.find(&needle) else {
            return Vec::new();
        };
        let rest = &line[start + needle.len()..];
        rest[..rest.find('"').unwrap()]
            .replace(['<', '>', '[', ']', ','], " ")
            .split_whitespace()
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
            .filter(|t| *t != "~")
            .map(|t| t.parse().unwrap())
            .collect()
    }

    fn humanised(amount: &str) -> Style {
        Style::parse(&format!("+++\nhumanise = {amount}\n+++")).unwrap()
    }

    #[test]
    fn the_bass_and_the_melody_are_separate_instruments() {
        // "bAn" ends both lines, so it is a rhyme word and takes the melody; "rot" is a stressed
        // foundation vowel in a non-rhyming position, so it is bass. Two layers, each with its
        // palette's instrument, in its own octave.
        let source = "rot bAn\nrot bAn\n";
        let style = Style::parse(
            "+++\nbass = \"gm_electric_bass_finger\"\nmelody = \"gm_vibraphone\"\n+++",
        )
        .unwrap();
        let text = render(source, &style);
        let melody = text.lines().find(|l| l.contains("the melody")).unwrap();
        let bass = text.lines().find(|l| l.contains("the bass")).unwrap();
        assert!(melody.contains(".sound(\"gm_vibraphone\")"), "{melody}");
        assert!(
            bass.contains(".sound(\"gm_electric_bass_finger\")"),
            "{bass}"
        );
        // Hand-worked, C minor: the rhyme class is degree 0, so 60. "rot" is OW, a mid vowel, so
        // the third degree — minor's 3 semitones — in the bass octave: 36 + 3 = 39.
        assert!(values_on(&text, "  note(", "gain").len() > 1);
        let notes = |line: &str| -> Vec<String> {
            line.split('"')
                .nth(1)
                .unwrap()
                .replace(['<', '>', '[', ']'], " ")
                .split_whitespace()
                // A bar's `@` weight is how long it lasts, not a value.
                .map(|t| t.split('@').next().unwrap_or(t))
                .filter(|t| !t.is_empty())
                .filter(|t| *t != "~")
                .map(ToOwned::to_owned)
                .collect()
        };
        assert!(notes(melody).iter().all(|n| n == "60"), "{melody}");
        assert!(notes(bass).iter().all(|n| n == "39"), "{bass}");
    }

    #[test]
    fn a_seventh_voicing_stacks_the_scale_on_each_rhyme_degree() {
        // Two rhyme classes, "bAn" (degree 0) then "dos" (degree 1), under D dorian:
        // D E F G A B C = 62 64 65 67 69 71 72 74 from MIDI 60 + 2. Hand-worked:
        //   degree 0 + {0, 2, 4, 6} = D F A C = 62 65 69 72  (Dm7)
        //   degree 1 + {0, 2, 4, 6} = E G B D = 64 67 71 74  (Em7)
        let source = "rot bAn\nrot bAn\nrot dos\nrot dos\n";
        let header = "+++\nroot = \"D\"\nscale = \"dorian\"\nvoicing = \"seventh\"\n+++";
        let text = render(source, &Style::parse(header).unwrap());
        let melody = text.lines().find(|l| l.contains("the melody")).unwrap();
        let chords: Vec<&str> = melody
            .split('"')
            .nth(1)
            .unwrap()
            .split(|c: char| c.is_whitespace() || c == '<' || c == '>')
            // A bar's `@` weight is how long it lasts, not a chord.
            .map(|t| t.split('@').next().unwrap_or(t))
            .map(|t| t.trim_start_matches('[').trim_end_matches(']'))
            .filter(|t| !t.is_empty() && *t != "~")
            .collect();
        assert!(chords.contains(&"62,65,69,72"), "{melody}");
        assert!(chords.contains(&"64,67,71,74"), "{melody}");
        assert!(chords.iter().all(|c| c.split(',').count() == 4), "{melody}");
        // The level is the chord's: four tones at 1/√4 of the melody level, 0.75 × 0.5.
        assert!(melody.contains(".postgain(0.375)"), "{melody}");
        // The bass is never voiced: "rot" is OW, mid, so the third degree of dorian (3
        // semitones) from 36 + 2 = 41, one note.
        let bass = text.lines().find(|l| l.contains("the bass")).unwrap();
        assert!(!bass.split('"').nth(1).unwrap().contains(','), "{bass}");
        assert!(bass.contains("41"), "{bass}");
        assert!(bass.contains(".postgain(0.320)"), "{bass}");
        assert!(
            text.contains("as seventh chords"),
            "named in the header: {text}"
        );

        // Note voicing is the same piece with each chord's lowest tone alone.
        let single = render(
            source,
            &Style::parse("+++\nroot = \"D\"\nscale = \"dorian\"\n+++").unwrap(),
        );
        let melody_single = single.lines().find(|l| l.contains("the melody")).unwrap();
        assert!(melody_single.contains("62") && !melody_single.contains("65,"));
    }

    #[test]
    fn every_kit_asks_every_bank_for_a_sample_the_bank_has() {
        // A sample a bank lacks is not an error Strudel raises: it logs "sound not found" on every
        // hit and the layer goes silent. So every pair a header can write must name a real one,
        // and every bank a header can name must be in the table checked against the sample map.
        for (bank, kit, fallback) in BANKS.iter().flat_map(|bank| {
            KITS.iter()
                .flat_map(move |kit| RIDE_FALLBACKS.iter().map(move |f| (bank, kit, f)))
        }) {
            {
                let style = Style::parse(&format!(
                    "+++\nkit = \"{kit}\"\nbank = \"{bank}\"\nride_fallback = \"{fallback}\"\n+++"
                ))
                .unwrap();
                let strudel = strudel_bank(style.bank());
                assert!(
                    BANK_SAMPLES.iter().any(|(name, _)| *name == strudel),
                    "{bank} → {strudel} is not in the sample table"
                );
                for layer in [Layer::Foundation, Layer::Flourish] {
                    let sample = sample_for(&style, layer);
                    assert!(
                        bank_has(strudel, sample),
                        "{kit} on {bank} ({fallback}) asks {strudel} for `{sample}` ({layer:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn a_bank_without_a_ride_keeps_time_on_its_open_hat_and_says_so() {
        // tidal-drum-machines.json has no RolandTR808_rd and no RolandTR707_rd; both have `oh`.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        for (bank, strudel) in [("tr808", "RolandTR808"), ("tr707", "RolandTR707")] {
            let header = format!("+++\npreset = \"jazz\"\nbank = \"{bank}\"\n+++");
            let text = render(source, &Style::parse(&header).unwrap());
            let drums: Vec<String> = text
                .lines()
                .filter(|l| l.starts_with("  s("))
                .map(|l| argument(l, "s"))
                .collect();
            assert!(drums.iter().any(|d| d.contains("oh")), "{text}");
            assert!(drums.iter().any(|d| d.contains("sd")), "{text}");
            assert!(!drums.iter().any(|d| d.contains("rd")), "{text}");
            assert!(
                text.contains(&format!(
                    "open hat in place of the ride, which {strudel} does not have,"
                )),
                "the stand-in is visible: {text}"
            );
        }
        // A bank that has a ride plays it, and the header claims no stand-in.
        let text = render(
            source,
            &Style::parse("+++\npreset = \"jazz\"\nbank = \"linn\"\n+++").unwrap(),
        );
        assert!(text.contains("(ride on the foundation"), "{text}");
        assert!(!text.contains("in place of the ride"), "{text}");
    }

    #[test]
    fn a_bank_without_a_ride_can_keep_time_on_its_crash_instead() {
        // tidal-drum-machines.json has RolandTR808_cr and RolandTR707_cr.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let crash = "+++\npreset = \"jazz\"\nbank = \"tr808\"\nride_fallback = \"crash\"\n+++";
        let text = render(source, &Style::parse(crash).unwrap());
        let drums: Vec<String> = text
            .lines()
            .filter(|l| l.starts_with("  s("))
            .map(|l| argument(l, "s"))
            .collect();
        assert!(drums.iter().any(|d| d.contains("cr")), "{text}");
        assert!(drums.iter().any(|d| d.contains("sd")), "{text}");
        assert!(
            !drums.iter().any(|d| d.contains("oh") || d.contains("rd")),
            "{text}"
        );
        assert!(
            text.contains("(crash in place of the ride, which RolandTR808 does not have, on"),
            "the stand-in is named on the plays line: {text}"
        );
        assert!(text.contains("//   ride_fallback = crash\n"), "{text}");
        // The same piece but for that one sample name: the stand-in strikes where the hat did.
        let hat = render(
            source,
            &Style::parse("+++\npreset = \"jazz\"\nbank = \"tr808\"\n+++").unwrap(),
        );
        let music = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| !l.starts_with("//"))
                .map(|l| {
                    l.replace("\"cr", "\"oh")
                        .replace(" cr", " oh")
                        .replace("[cr", "[oh")
                })
                .collect()
        };
        assert_eq!(music(&text), music(&hat));
        // A bank that has a ride plays it whatever the fallback, and the header names no stand-in.
        let linn = render(
            source,
            &Style::parse(
                "+++\npreset = \"jazz\"\nbank = \"linn\"\nride_fallback = \"crash\"\n+++",
            )
            .unwrap(),
        );
        assert!(linn.contains("(ride on the foundation"), "{linn}");
        assert!(!linn.contains("in place of the ride"), "{linn}");
    }

    #[test]
    fn each_role_plays_in_its_palettes_octave() {
        // Hand-worked, C minor, as in `the_bass_and_the_melody_are_separate_instruments`: the
        // rhyme class is degree 0 and "rot" is the third degree (3 semitones). Melody octave 5
        // starts at MIDI 72, so 72; bass octave 1 at 24, so 24 + 3 = 27. And octave 3 for both,
        // where the ranges meet: 48 and 51.
        let source = "rot bAn\nrot bAn\n";
        let notes = |text: &str, role: &str| -> Vec<String> {
            let line = text.lines().find(|l| l.contains(role)).unwrap();
            argument(line, "note")
                .replace(['<', '>', '[', ']'], " ")
                .split_whitespace()
                // A bar's `@` weight is how long it lasts, not a value.
                .map(|t| t.split('@').next().unwrap_or(t))
                .filter(|t| !t.is_empty())
                .filter(|t| *t != "~")
                .map(ToOwned::to_owned)
                .collect()
        };
        for (header, melody, bass) in [
            ("bass_octave = 1\nmelody_octave = 5", "72", "27"),
            ("bass_octave = 3\nmelody_octave = 3", "48", "51"),
            ("", "60", "39"),
        ] {
            let text = render(
                source,
                &Style::parse(&format!("+++\n{header}\n+++")).unwrap(),
            );
            let (m, b) = (notes(&text, "the melody"), notes(&text, "the bass"));
            assert!(
                !m.is_empty() && m.iter().all(|n| n == melody),
                "{header}: {m:?}"
            );
            assert!(
                !b.is_empty() && b.iter().all(|n| n == bass),
                "{header}: {b:?}"
            );
        }
        // Listed in the header when set, and only then — so a pattern that does not use them
        // reads exactly as it did before the keys existed.
        let moved = render(
            source,
            &Style::parse("+++\nbass_octave = 1\nmelody_octave = 5\n+++").unwrap(),
        );
        assert!(moved.contains("//   bass_octave = 1\n"), "{moved}");
        assert!(moved.contains("//   melody_octave = 5\n"), "{moved}");
        let plain = render(source, &Style::default());
        for key in [
            "bass_octave",
            "melody_octave",
            "density_level",
            "ride_fallback",
        ] {
            assert!(!plain.contains(key), "{key} listed at its default: {plain}");
        }
    }

    #[test]
    fn levels_follow_density_only_in_bars_of_several_lines() {
        // The arrangement of `a_stanza_keeps_playing_under_the_next`: stanza 1 stated alone (two
        // slots of one line), then stanza 2 with stanza 1 looping under it, then the outro, which
        // repeats stanza 2 over the same loop — so one line, one line, then two lines in each of
        // the last four slots.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let score = score_of(source);
        assert_eq!(super::lines_sounding(&score), vec![1, 1, 2, 2, 2, 2]);

        let at = |level: &str| {
            render(
                source,
                &Style::parse(&format!("+++\ndensity_level = {level}\n+++")).unwrap(),
            )
        };
        // Zero is today's piece, byte for byte, header included.
        assert_eq!(at("0"), render(source, &Style::default()));

        // Voice 0 is stanza 1's first line, `bA`: one stressed syllable at full velocity, sounding
        // in slots 0, 2 and 4 (`<[hh ~ ~] ~ hh ~ hh ~>`). Hand-worked levels: slot 0 has one line
        // and keeps 1; slots 2 and 4 have two, so 2^-1 = 0.5 at density 1 and 2^-0.5 = 0.707 at
        // density 0.5.
        let voice_0 = |text: &str| values_on(text, "  s(\"<[hh", "gain");
        assert_eq!(voice_0(&at("0")), vec![1.0, 1.0, 1.0]);
        assert_eq!(voice_0(&at("1")), vec![1.0, 0.5, 0.5]);
        assert_eq!(voice_0(&at("0.5")), vec![1.0, 0.707, 0.707]);

        // Only gain values change: every other line, and every other parameter of every layer, is
        // byte for byte the same — the arrangement, the samples, the notes, the filters.
        let without_gain = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|l| !l.starts_with("//"))
                .map(|l| {
                    let mut kept = String::new();
                    let mut rest = l;
                    while let Some(at) = rest.find(".gain(\"") {
                        kept.push_str(&rest[..at]);
                        let after = &rest[at + 7..];
                        rest = &after[after.find('"').unwrap() + 2..];
                    }
                    kept.push_str(rest);
                    kept
                })
                .collect()
        };
        assert_eq!(without_gain(&at("1")), without_gain(&at("0")));
        assert_ne!(at("1"), at("0"));
        // ...and the header says what it did.
        assert!(at("1").contains("//   density_level = 1.00\n"));
        assert!(
            at("0.5").contains(
                "//   density: every gain in a bar divided by n^0.500, n = the lines sounding in \
                 it (1: ×1.000, 2: ×0.707)"
            ),
            "{}",
            at("0.5")
        );
    }

    #[test]
    fn a_seventh_voicing_outside_a_seven_note_scale_is_named_for_what_it_stacks() {
        // C pentatonic minor: C Eb F G Bb = 0 3 5 7 10. Hand-worked:
        //   degree 0 + {0, 2, 4, 6} = degrees 0 2 4 6 = C F Bb Eb' = 60 65 70 75 (stacked fourths)
        //   degree 1 + {0, 2, 4, 6} = degrees 1 3 5 7 = Eb G C' F' = 63 67 72 77
        let source = "rot bAn\nrot bAn\nrot dos\nrot dos\n";
        let header = "+++\nscale = \"pentatonic_minor\"\nvoicing = \"seventh\"\n+++";
        let text = render(source, &Style::parse(header).unwrap());
        let melody = text.lines().find(|l| l.contains("the melody")).unwrap();
        assert!(melody.contains("[60,65,70,75]"), "{melody}");
        assert!(melody.contains("[63,67,72,77]"), "{melody}");
        assert!(!text.contains("as seventh chords"), "{text}");
        assert!(
            text.contains(
                "as four-tone chords on every other degree of pentatonic_minor (not sevenths: \
                 the scale has 5 notes, not 7)"
            ),
            "{text}"
        );
    }

    #[test]
    fn the_ride_kit_keeps_time_on_the_ride_and_comps_on_the_snare() {
        // The same poem as `the_layer_chooses_the_instrument`: a regular stanza and a ragged one.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let style = Style::parse("+++\nkit = \"ride\"\nbank = \"r8\"\n+++").unwrap();
        let text = render(source, &style);
        let drums: Vec<&str> = text.lines().filter(|l| l.starts_with("  s(")).collect();
        assert!(
            drums.iter().any(|l| l.contains("rd")),
            "the foundation: {text}"
        );
        assert!(
            drums.iter().any(|l| l.contains("sd")),
            "the flourish: {text}"
        );
        assert!(
            !drums.iter().any(|l| l.contains("bd") || l.contains("hh")),
            "{text}"
        );
        assert!(
            drums.iter().all(|l| l.contains(".bank(\"RolandR8\")")),
            "{text}"
        );
        // Same layers, same steps: only the sound's name moved.
        let machine = render(source, &Style::default());
        let skeleton = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|l| l.starts_with("  s("))
                .map(|l| argument(l, "s").replace("rd", "bd").replace("sd", "hh"))
                .collect()
        };
        assert_eq!(skeleton(&text), skeleton(&machine));
    }

    #[test]
    fn a_synth_name_becomes_the_nearest_recorded_instrument() {
        // SPEC.md §5 names models Strudel lacks. Each takes the instrument nearest its model, per
        // role; the plain waveforms stay oscillators.
        let source = "rot bAn\nrot bAn\n";
        let sound = |synth: &str, role: &str| {
            let text = render(
                source,
                &Style::parse(&format!("+++\nsynth = \"{synth}\"\n+++")).unwrap(),
            );
            let line = text.lines().find(|l| l.contains(role)).unwrap().to_owned();
            line.split(".sound(\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_eq!(sound("karplus", "the melody"), "gm_acoustic_guitar_nylon");
        assert_eq!(sound("karplus", "the bass"), "gm_acoustic_bass");
        assert_eq!(sound("fm", "the melody"), "gm_epiano1");
        assert_eq!(sound("fm", "the bass"), "gm_synth_bass_2");
        assert_eq!(sound("physical", "the melody"), "gm_marimba");
        assert_eq!(sound("triangle", "the melody"), "triangle");
        assert_eq!(sound("subtractive", "the bass"), "sawtooth");
    }

    #[test]
    fn the_high_cut_scales_the_brightness_range_rather_than_clipping_it() {
        // §4.4's mapping is 400 Hz + sharpness × (high cut − 400). Hand-worked for the same word
        // under two cuts: the excess over 400 Hz scales by (2400 − 400) / (4400 − 400) = 1/2.
        let source = "bA bAnAnAnA\n";
        let bright = values_on(&render(source, &Style::default()), "  s(", "lpf");
        let dark = values_on(
            &render(source, &Style::parse("+++\nhigh_cut = 2400\n+++").unwrap()),
            "  s(",
            "lpf",
        );
        assert_eq!(bright.len(), dark.len());
        for (b, d) in bright.iter().zip(&dark) {
            assert!(((d - 400.0) - (b - 400.0) / 2.0).abs() < 0.002, "{b} {d}");
        }
        // ...so the order between words survives: the striking word is still the brighter one.
        assert!(dark[1] > dark[0], "{dark:?}");
    }

    #[test]
    fn the_mix_reaches_every_layer() {
        let style = Style::parse(
            "+++\nreverb = 0.4\nroom_size = 3\ndelay = 0.5\ncompress = 1\ndrive = 0.2\n\
             drums_gain = 0.5\nbass_gain = 1.5\nmelody_gain = 0.75\ntempo_range = [120, 120]\n+++",
        )
        .unwrap();
        let text = render("rot bAn\nrot bAn\n", &style);
        let line = |needle: &str| {
            text.lines()
                .find(|l| l.contains(needle))
                .unwrap()
                .to_owned()
        };
        let (drums, melody, bass) = (line("voice 0"), line("the melody"), line("the bass"));
        for layer in [&drums, &melody, &bass] {
            // One room for the whole piece: Strudel's reverb is per orbit.
            assert!(layer.contains(".roomsize(3.000)"), "{layer}");
            // compress = 1: −24 dB at 7:1.
            assert!(
                layer.contains(".compressor(-24.000).compressorRatio(7.000)"),
                "{layer}"
            );
        }
        assert!(drums.contains(".postgain(0.500)"), "{drums}");
        assert!(bass.contains(".postgain(1.500)"), "{bass}");
        assert!(melody.contains(".postgain(0.750)"), "{melody}");
        // The echo is the melody's alone: a dotted eighth at 120 bpm is 0.75 × 0.5 s.
        assert!(
            melody.contains(".delay(0.500).delaytime(0.375)"),
            "{melody}"
        );
        assert!(!drums.contains(".delay(") && !bass.contains(".delay("));
        // Drive is saturation on the drums and bass, at half its amount.
        assert!(drums.contains(".shape(0.100)") && bass.contains(".shape(0.100)"));
        assert!(!melody.contains(".shape("));
        // The melody sends the whole reverb amount, the bass three tenths of it.
        assert!(melody.contains(".room(0.400)") && bass.contains(".room(0.120)"));
        // The drums' send is half the reverb plus §4.4's rise: never below 0.2 here.
        assert!(
            values_on(&text, "  s(", "room")
                .iter()
                .all(|r| *r >= 0.2 - 1e-9)
        );

        // ...and the raw preset has none of it.
        let raw = render("rot bAn\nrot bAn\n", &Style::preset("raw").unwrap());
        for absent in [
            ".compressor(",
            ".delay(",
            ".shape(",
            ".nudge(",
            ".velocity(",
        ] {
            assert!(!raw.contains(absent), "{absent} in raw:\n{raw}");
        }
    }

    #[test]
    fn humanise_zero_is_a_machine() {
        let text = render("teA bAn\nteA rAn\n", &humanised("0"));
        assert!(
            !text.contains(".nudge(") && !text.contains(".velocity("),
            "{text}"
        );
        assert!(!text.contains("humanise:"), "{text}");
    }

    #[test]
    fn humanise_strays_late_and_soft_within_its_amount_and_in_proportion_to_it() {
        // Hand-worked bounds: at humanise h, each event is 0..=0.030·h s late and multiplied by
        // 1 − 0..0.30·h. The draws are the line's, so doubling h doubles every stray exactly.
        let source = "teA bAn rot\nteA rAn sot\n\nbo do\nro so\n";
        let half = render(source, &humanised("0.5"));
        let full = render(source, &humanised("1"));
        for prefix in ["  s(", "  note("] {
            let (late_half, late_full) = (
                values_on(&half, prefix, "nudge"),
                values_on(&full, prefix, "nudge"),
            );
            let (vel_half, vel_full) = (
                values_on(&half, prefix, "velocity"),
                values_on(&full, prefix, "velocity"),
            );
            assert!(!late_full.is_empty(), "{full}");
            assert_eq!(late_half.len(), late_full.len());
            for (h, f) in late_half.iter().zip(&late_full) {
                assert!((0.0..=0.030).contains(f), "{f}");
                assert!((f - 2.0 * h).abs() <= 0.0015, "{h} {f}");
            }
            for (h, f) in vel_half.iter().zip(&vel_full) {
                assert!((0.70..=1.0).contains(f), "{f}");
                assert!(((1.0 - f) - 2.0 * (1.0 - h)).abs() <= 0.0015, "{h} {f}");
            }
            // Not a constant: a stray that is the same everywhere is a delay, not a feel.
            assert!(
                late_full.iter().any(|v| (v - late_full[0]).abs() > 0.001),
                "{late_full:?}"
            );
        }
        assert!(full.contains("humanise: each syllable up to 0.030 s late and 0.300 softer"));
    }

    #[test]
    fn a_note_strays_with_its_own_drum_hit() {
        // A pitched event is the same event as its drum hit, so they must stray together or the
        // note would drift off its syllable. "bAn" is the second word: its drum stray is the
        // second value of the line's nudge, and its note's is the melody's first.
        let text = render("te bAn\nte bAn\n", &humanised("1"));
        let drum = values_on(&text, "  s(", "nudge");
        let note = values_on(&text, "  note(", "nudge");
        assert!((drum[1] - note[0]).abs() < 1e-9, "{drum:?} {note:?}");
    }

    /// Each line's strays as `(line text, strays in order)`, for the lines of the score's voices.
    fn strays_by_line(source: &str) -> std::collections::BTreeMap<String, Vec<(u64, u64)>> {
        let score = score_of(source);
        let strays = super::strays(&score);
        let mut out = std::collections::BTreeMap::new();
        for voice in score.voices() {
            let first_bar = score
                .events_of(voice.id())
                .map(|e| e.bar().get())
                .min()
                .unwrap();
            let line = source[voice.span().start()..voice.span().end()].to_owned();
            let mut events: Vec<_> = score
                .events_of(voice.id())
                .filter(|e| e.bar().get() == first_bar)
                .collect();
            events.sort_by_key(|e| super::key(e));
            let values = events
                .iter()
                .map(|e| {
                    let stray = strays[&super::key(e)];
                    (stray.late.to_bits(), stray.soft.to_bits())
                })
                .collect();
            out.insert(line, values);
        }
        out
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        /// I1 for the stray: the same poem and palette render the same bytes, humanised or not.
        #[test]
        fn humanised_rendering_is_reproducible(
            lines in proptest::collection::vec("(te|bA|rot|sAn|do|ni) (bAn|dot|rAs|tes)", 1..5),
            amount in 0_u8..=100,
        ) {
            let source = lines.join("\n") + "\n";
            let style = humanised(&format!("{}", f64::from(amount) / 100.0));
            let score = score_of(&source);
            let (Ok(a), Ok(b)) = (Strudel::new().render(&score, &style), Strudel::new().render(&score_of(&source), &style)) else {
                return Ok(());
            };
            proptest::prop_assert_eq!(a, b);
        }

        /// I2 for the stray: editing the first stanza leaves every line of the second straying
        /// exactly as before, bit for bit.
        #[test]
        fn an_edit_in_one_stanza_does_not_move_anothers_strays(
            first in proptest::collection::vec("(te|bA|rot|sAn) (bAn|dot|rAs)", 1..4),
            edited in proptest::collection::vec("(do|ni|bo|A) (tes|nAn|sot)", 1..4),
            second in proptest::collection::vec("(bAd|ro|tAn) (sAt|bon|der)", 1..4),
        ) {
            let before = format!("{}\n\n{}\n", first.join("\n"), second.join("\n"));
            let after = format!("{}\n\n{}\n", edited.join("\n"), second.join("\n"));
            let (was, now) = (strays_by_line(&before), strays_by_line(&after));
            for line in &second {
                proptest::prop_assert!(was.contains_key(line.as_str()), "{} not in {:?}", line, was);
                // A line of the second stanza that the first also contains is one voice with it,
                // and its seed is still its own text — so it compares equal either way.
                proptest::prop_assert_eq!(was.get(line.as_str()), now.get(line.as_str()), "{}", line);
            }
        }
    }

    /// A poem lowered with the harmony's `variant` chosen.
    fn harmonised(source: &str, variant: prosody_core::Variant) -> Score {
        harmonised_with(source, variant, false)
    }

    /// [`harmonised`], under the elastic pulse when `elastic`.
    fn harmonised_with(source: &str, variant: prosody_core::Variant, elastic: bool) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &ByLength);
        let roles = role::assign(&meter, &repetition).unwrap();
        let mut mappings = prosody_core::Mappings::all()
            .with(prosody_core::Mapping::Harmony.choose(variant).unwrap());
        if elastic {
            mappings = mappings.with(
                prosody_core::Mapping::Pulse
                    .choose(prosody_core::Variant::Elastic)
                    .unwrap(),
            );
        }
        lower::run_with(
            Analyses {
                poem: &poem,
                meter: &meter,
                rhyme: &rhyme,
                repetition: &repetition,
                sound: &sound,
                surprisal: &surprisal,
                roles: &roles,
            },
            &mappings,
        )
        .unwrap()
    }

    /// ABAB of stressed one-syllable words: A (AA) is degree 0, B (OW) degree 1. One foundation
    /// stanza, so the piece is four bars stated and the outro's four again.
    const ABAB: &str = "dA bA\nro bo\nsA tA\nno do\n";

    /// The chord lane's line of the pattern.
    fn chord_line(text: &str) -> &str {
        text.lines()
            .find(|line| line.contains("// the chords"))
            .unwrap_or_else(|| panic!("no chord lane in:\n{text}"))
    }

    #[test]
    fn without_the_harmony_there_is_no_chord_lane_and_no_chord_keys() {
        let text = render(ABAB, &Style::default());
        assert!(!text.contains("the chords"), "{text}");
        assert!(
            !text.contains("chord_sound") && !text.contains("chord_gain"),
            "{text}"
        );
        assert!(!text.contains("chords"), "{text}");
    }

    #[test]
    fn rhyme_plays_abab_as_alternating_triads() {
        // Default palette: C minor, the melody at octave 4 (MIDI 60), so chords from 48. `note`
        // voicing stacks a triad: degree 0 is C Eb G (48 51 55), degree 1 is D F Ab (50 53 56).
        let text = Strudel::new()
            .render(
                &harmonised(ABAB, prosody_core::Variant::Rhyme),
                &Style::default(),
            )
            .unwrap();
        let line = chord_line(&text);
        // Each line is one beat (a spondee), so under the steady pulse each chord lasts `@1`.
        let one = "[48,51,55]@1 [50,53,56]@1";
        assert!(
            line.starts_with(&format!(
                "  note(\"<{one} {one} {one} {one}>\").sound(\"gm_pad_warm\")"
            )),
            "{line}"
        );
        // Every slot has a chord at the bar's own level; the lane's level 0.40 is shared by the
        // three tones: 0.4 / √3.
        assert!(
            line.contains(
                ".gain(\"<1.000@1 1.000@1 1.000@1 1.000@1 1.000@1 1.000@1 1.000@1 1.000@1>\")"
            ),
            "{line}"
        );
        assert!(line.contains(".postgain(0.231)"), "{line}");
        assert!(text.contains("//   chord_sound = gm_pad_warm\n"), "{text}");
        assert!(text.contains("//   chord_gain = 0.40\n"), "{text}");
        assert!(
            text.contains("//   chords by line: 1:I/0 2:I/1 3:I/0 4:I/1\n"),
            "{text}"
        );
        assert!(text.contains("as triads in minor"), "{text}");
    }

    #[test]
    fn cadence_voiced_as_sevenths_folds_its_roots_into_one_octave() {
        // Cadence on ABAB: V/4, V/5, I/0, and the last bar I/1 then the turnaround V/8 — degree 8
        // is degree 1 folded into the octave, so here the turnaround restrikes the bar's own
        // chord. Sevenths in C minor from 48: degree 4 is G Bb D F (55 58 62 65), 5 is Ab C Eb G
        // (56 60 63 67), 0 is C Eb G Bb (48 51 55 58), 1 is D F Ab C (50 53 56 60).
        let style = Style::parse("+++\nvoicing = \"seventh\"\n+++").unwrap();
        let text = Strudel::new()
            .render(&harmonised(ABAB, prosody_core::Variant::Cadence), &style)
            .unwrap();
        let line = chord_line(&text);
        let one = "[55,58,62,65]@1 [56,60,63,67]@1 [48,51,55,58]@1 \
                   [[50,53,56,60] [50,53,56,60]]@1";
        assert!(
            line.starts_with(&format!("  note(\"<{one} {one}>\")")),
            "{line}"
        );
        // Four tones share the level: 0.4 / 2.
        assert!(line.contains(".postgain(0.200)"), "{line}");
        assert!(
            text.contains("//   chords by line: 1:V/4 2:V/5 3:I/0 4:I/1+V/8\n"),
            "{text}"
        );
        assert!(text.contains("as seventh chords in minor"), "{text}");
    }

    #[test]
    fn a_bar_without_a_chord_rests_in_the_chord_lane() {
        // `ro bAn` rhymes with nothing: under rhyme its bar rests, and the header says `-`.
        let text = Strudel::new()
            .render(
                &harmonised("dA bA\nro bAn\nsA tA\n", prosody_core::Variant::Rhyme),
                &Style::default(),
            )
            .unwrap();
        let line = chord_line(&text);
        assert!(
            line.starts_with(
                "  note(\"<[48,51,55]@1 ~@1 [48,51,55]@1 [48,51,55]@1 ~@1 [48,51,55]@1>\")"
            ),
            "{line}"
        );
        assert!(
            line.contains(".gain(\"<1.000@1 ~@1 1.000@1 1.000@1 ~@1 1.000@1>\")"),
            "{line}"
        );
        assert!(
            text.contains("//   chords by line: 1:I/0 2:- 3:I/0\n"),
            "{text}"
        );
    }

    /// ABBA in four-iamb lines: A (AA) is degree 0, B (OW) degree 1.
    const ABBA: &str = "de dA de bA de sA de tA\nde do de bo de so de to\n\
                        de bA de sA de dA de no\nde bo de so de to de bA\n";

    #[test]
    fn the_last_bar_plays_its_own_chord_then_the_turnaround_from_its_half() {
        // Cadence on ABBA: A opens (V/4: G Bb D, 55 58 62), B opens (V/5: Ab C Eb, 56 60 63), B
        // closes (I/1: D F Ab, 50 53 56), and A closes the stanza on I/0 (C Eb G, 48 51 55) — then
        // from the bar's half V of the first chord's root, 4 + 4 = 8, which folds to degree 1
        // (50 53 56). Four beats a bar: under the steady pulse `@4`, the split `[a b]` inside it,
        // so the turnaround begins two beats in. Stated, then the outro's pass.
        let score = harmonised(ABBA, prosody_core::Variant::Cadence);
        let text = Strudel::new().render(&score, &Style::default()).unwrap();
        let line = chord_line(&text);
        let pass = "[55,58,62]@4 [56,60,63]@4 [50,53,56]@4 [[48,51,55] [50,53,56]]@4";
        assert!(
            line.starts_with(&format!("  note(\"<{pass} {pass}>\")")),
            "{line}"
        );
        // On the music's clock: the lane's bar weights are the slots' beats, as every other
        // pattern's (`weights`, the one walk's), so a chord lasts its bar and not one beat.
        let lane_weights: Vec<u64> = argument(line, "note")
            .split('@')
            .skip(1)
            .map(|rest| {
                rest.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap()
            })
            .collect();
        assert_eq!(lane_weights, super::weights(&score));
        assert_eq!(lane_weights, vec![4; 8]);
        // The levels split with the chords: both halves of a turning bar carry the bar's level.
        let gains = argument(line, "gain");
        let steps: Vec<&str> = gains.trim_matches(['<', '>', '"']).split("@4").collect();
        assert!(steps[3].trim().starts_with('[') && steps[3].matches(' ').count() == 2);
        assert!(
            text.contains("//   chords by line: 1:V/4 2:V/5 3:I/1 4:I/0+V/8\n"),
            "{text}"
        );
        assert!(
            text.contains("//   chords: a stanza's last bar is split — its own chord, then"),
            "{text}"
        );
        // Under the elastic pulse a bar is one cycle, and the turnaround still begins at its
        // half.
        let elastic = Strudel::new()
            .render(
                &harmonised_with(ABBA, prosody_core::Variant::Cadence, true),
                &Style::default(),
            )
            .unwrap();
        let pass = "[55,58,62] [56,60,63] [50,53,56] [[48,51,55] [50,53,56]]";
        assert!(
            chord_line(&elastic).starts_with(&format!("  note(\"<{pass} {pass}>\")")),
            "{}",
            chord_line(&elastic)
        );
        // Rhyme (H1) has no turnaround, so no split and no split line in the header.
        let rhyme = Strudel::new()
            .render(
                &harmonised(ABBA, prosody_core::Variant::Rhyme),
                &Style::default(),
            )
            .unwrap();
        assert!(!rhyme.contains("is split"), "{rhyme}");
        assert!(!chord_line(&rhyme).contains("[["), "{rhyme}");
    }

    #[test]
    fn a_turning_bar_with_no_chord_of_its_own_rests_then_turns_round() {
        // Under `both`, `ro bAn` (AA N) rhymes with nothing, so its first half has no chord; from
        // its half it turns round onto the stanza's first chord: A opens on degree 0, V/4, so V/8,
        // degree 1 folded (50 53 56).
        let text = Strudel::new()
            .render(
                &harmonised("dA bA\nsA tA\nro bAn\n", prosody_core::Variant::Both),
                &Style::default(),
            )
            .unwrap();
        let line = chord_line(&text);
        let pass = "[55,58,62]@1 [48,51,55]@1 [~ [50,53,56]]@1";
        assert!(
            line.starts_with(&format!("  note(\"<{pass} {pass}>\")")),
            "{line}"
        );
        assert!(
            line.contains(".gain(\"<1.000@1 1.000@1 [~ 1.000]@1"),
            "{line}"
        );
        assert!(
            text.contains("//   chords by line: 1:V/4 2:I/0 3:-+V/8\n"),
            "{text}"
        );
    }

    #[test]
    fn the_chord_lane_follows_the_palette_and_never_its_structure() {
        // I3: another palette changes the chord lane's sound, register and level, and not which
        // slots have a chord.
        let score = harmonised(ABAB, prosody_core::Variant::Both);
        let plain = Strudel::new().render(&score, &Style::default()).unwrap();
        let other = Style::parse(
            "+++\npreset = \"jazz\"\nchord_sound = \"gm_vibraphone\"\nchord_gain = 1.2\n\
             melody_octave = 5\nroot = \"E\"\n+++",
        )
        .unwrap();
        let coloured = Strudel::new().render(&score, &other).unwrap();
        let rests = |text: &str| -> Vec<bool> {
            argument(chord_line(text), "note")
                .trim_matches(['<', '>'])
                .split(' ')
                .map(|step| step.split('@').next() == Some("~"))
                .collect()
        };
        assert_eq!(rests(&plain), rests(&coloured));
        assert_ne!(
            argument(chord_line(&plain), "note"),
            argument(chord_line(&coloured), "note")
        );
        assert!(chord_line(&coloured).contains(".sound(\"gm_vibraphone\")"));
    }

    #[test]
    fn swing_slices_the_bar_so_it_comes_before_the_tempo() {
        // `.cpm` is `_fast(cpm / 60)`: after it a cycle is the scheduler's second, not a bar, so
        // a `.swingBy` chained after it sliced the wrong span and moved, dropped and doubled
        // onsets. The swing must be applied to the stack itself, and the tempo last.
        let score = harmonised(ABAB, prosody_core::Variant::Both);
        let swung = Style::parse(
            "+++
swing = 0.33
+++",
        )
        .unwrap();
        let text = Strudel::new().render(&score, &swung).unwrap();
        let tail: Vec<&str> = text.lines().rev().take(3).collect();
        assert_eq!(tail[2], ")", "{text}");
        assert!(tail[1].starts_with("  .swingBy(0.330, "), "{text}");
        assert!(tail[0].starts_with("  .cpm("), "{text}");
    }

    #[test]
    fn outside_a_seven_note_scale_the_header_says_the_numerals_are_not_functions() {
        let score = harmonised(ABAB, prosody_core::Variant::Cadence);
        let seven = Strudel::new().render(&score, &Style::default()).unwrap();
        assert!(seven.contains(" as triads in "), "{seven}");
        assert!(!seven.contains("not functions"), "{seven}");
        let five = Style::parse("+++\nscale = \"pentatonic_minor\"\n+++").unwrap();
        let text = Strudel::new().render(&score, &five).unwrap();
        assert!(!text.contains(" as triads in "), "{text}");
        assert!(
            text.contains(" as three-tone chords on every other degree in pentatonic_minor "),
            "{text}"
        );
        assert!(
            text.contains("pentatonic_minor has 5 notes, not 7, so the numerals below count steps"),
            "{text}"
        );
    }

    /// A score under the chosen bass and harmony variants.
    fn bassed(source: &str, bass: prosody_core::Variant, harmony: prosody_core::Variant) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let mappings = prosody_core::Mappings::all()
            .with(prosody_core::Mapping::Bass.choose(bass).unwrap())
            .with(prosody_core::Mapping::Harmony.choose(harmony).unwrap());
        lower::run_with(
            Analyses {
                poem: &poem,
                meter: &meter,
                rhyme: &rhyme::analyse(&poem, &Spelled),
                repetition: &repetition,
                sound: &sound::analyse(&poem, &Spelled),
                surprisal: &surprisal::analyse(&poem, &Spelled, &ByLength),
                roles: &roles,
            },
            &mappings,
        )
        .unwrap()
    }

    /// The bass layer's line of a pattern.
    fn bass_line(text: &str) -> &str {
        text.lines()
            .find(|line| line.contains("// the bass"))
            .unwrap_or_else(|| panic!("no bass in:\n{text}"))
    }

    /// The `index`-th bar of a `note("<…>")` pattern, without its brackets.
    fn bar_of(line: &str, index: usize) -> String {
        let pattern = argument(line, "note");
        let inner = pattern.trim_start_matches('<').trim_end_matches('>');
        // Bars are top-level elements: split at depth zero.
        let mut bars = Vec::new();
        let (mut depth, mut current) = (0_i32, String::new());
        for character in inner.chars() {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                ' ' if depth == 0 => {
                    bars.push(std::mem::take(&mut current));
                    continue;
                }
                _ => {}
            }
            current.push(character);
        }
        bars.push(current);
        // A bar's `@` weight is how long it lasts under the steady pulse, not part of the bar.
        let bar = bars[index].clone();
        match bar.rfind('@') {
            Some(at) if bar[at + 1..].chars().all(|c| c.is_ascii_digit()) => bar[..at].to_owned(),
            _ => bar,
        }
    }

    #[test]
    fn the_syllables_walk_sounds_its_passing_and_approach_tones() {
        // W1 in C minor (0 2 3 5 7 8 10) from the bass octave, MIDI 36. `bo de de bA`: OW is mid,
        // the third — step 2, Eb, 39. The passing tone steps down towards AA's root: step 1, D,
        // 38. The approach is a semitone below the root: 35, the B below — a chromatic note C
        // minor does not have. Then AA, the root, 36. One note per syllable, in the drum's grid.
        let source = "bo de de bA\nso te te sA\n";
        let text = Strudel::new()
            .render(
                &bassed(
                    source,
                    prosody_core::Variant::Syllables,
                    prosody_core::Variant::Off,
                ),
                &Style::default(),
            )
            .unwrap();
        let bass = bass_line(&text);
        let first: Vec<String> = bar_of(bass, 0)
            .replace(['[', ']'], " ")
            .split_whitespace()
            .map(ToOwned::to_owned)
            .collect();
        assert_eq!(first, vec!["39", "38", "35", "36"], "{bass}");
        assert!(bass.contains("(mapping bass = syllables)"), "{bass}");
        assert!(
            text.contains("//   walking bass (mapping bass = syllables): a note on every"),
            "{text}"
        );
        // The rhyme word sounds both: its melody note and its bass note.
        let melody = text.lines().find(|l| l.contains("the melody")).unwrap();
        assert!(melody.contains("60"), "{melody}");
        // The height bass, the default, says nothing new in the header.
        let plain = render(source, &Style::default());
        assert!(!plain.contains("walking bass"), "{plain}");
        assert!(
            bass_line(&plain)
                .ends_with("the bass — every stressed foundation vowel, by its height"),
            "{plain}"
        );
    }

    /// Four iambs a line, ABAB: `bA` lines (A, degree 0) and `bo` lines (O, degree 1).
    const WALK: &str = "de bA de bo de bo de bA\n\
                        de bo de bA de bA de bo\n\
                        se sA se so se so se sA\n\
                        se so se sA se sA se so\n";

    #[test]
    fn the_feet_walk_holds_each_beat_without_harmony() {
        // W2, harmony off, C minor from 36. Bar 0's stressed vowels: AA OW OW AA. The root (36),
        // the third (step 2, 39), OW again — the next foot's AA is lower, so a step down (step 1,
        // 38) — then AA's root (36). Each note fills its beat: no subdivision, so `[36 39 38 36]`.
        // Bar 1: OW AA AA OW — 39, 36, AA again with OW higher next, so up (38), then 39.
        let score = bassed(
            WALK,
            prosody_core::Variant::Feet,
            prosody_core::Variant::Off,
        );
        let text = Strudel::new().render(&score, &Style::default()).unwrap();
        let bass = bass_line(&text);
        assert_eq!(bar_of(bass, 0), "[36 39 38 36]", "{bass}");
        assert_eq!(bar_of(bass, 1), "[39 36 38 39]", "{bass}");
        // Every note is struck with its foot's stressed syllable's accent, full force — not the
        // unstressed `de` it rides on.
        let gains = argument(bass, "gain");
        assert!(gains.starts_with("<[1.000 1.000 1.000 1.000]"), "{gains}");
        assert!(
            text.contains("root, third, fifth of the scale: no harmony"),
            "{text}"
        );
        assert!(bass.contains("(mapping bass = feet)"), "{bass}");
    }

    #[test]
    fn one_walk_a_bar_a_held_line_keeps_its_drums_but_not_its_walk() {
        // ABAB, so the A line is held under the B line in slot 1 (`prosody_core::arrange`). Under
        // the height bass the held line's stressed `bA` sounds in a second bass lane, as it always
        // has — a chord tone under a chord tone. A walk under a walk clashed by a semitone on the
        // page, so a walking bass sounds only the bar's foundation line (`Score::sounds_bass`).
        let source = "de bA de bA\nde bo de bo\nse sA se sA\nse so se so\n";
        let off = prosody_core::Variant::Off;
        let height = Strudel::new()
            .render(
                &bassed(source, prosody_core::Variant::Height, off),
                &Style::default(),
            )
            .unwrap();
        assert!(height.contains("by its height (lane 2)"), "{height}");

        let text = Strudel::new()
            .render(
                &bassed(source, prosody_core::Variant::Feet, off),
                &Style::default(),
            )
            .unwrap();
        assert!(
            !text
                .lines()
                .any(|l| l.contains("(mapping bass = feet) (lane 2)")),
            "one walk a bar:\n{text}"
        );
        // The held line still sounds its drums under the B line.
        assert!(text.contains("(lane 2)"), "{text}");
        // C minor from 36. Slot 1 is the B line alone: `de bo` OW mid, the third (39); `de bo`
        // again would repeat it and no foot follows, so a step down (38). Slot 0, the A line: AA's
        // root (36), then the step below the root, Bb (34).
        assert!(
            text.contains("one walk a bar, the bar's foundation line's\n"),
            "the header says so:\n{text}"
        );
        let bass = bass_line(&text);
        assert_eq!(bar_of(bass, 0), "[36 34]", "{bass}");
        assert_eq!(bar_of(bass, 1), "[39 38]", "{bass}");
    }

    #[test]
    fn the_feet_walk_follows_the_chord_under_harmony_rhyme() {
        // H1: bar 0 is I on degree 0, bar 1 I on degree 1 (D). Bar 0 walks as without harmony.
        // Bar 1 walks the same steps as above, 2 0 1 2, from degree 1: degrees 3 1 2 3 of C
        // minor, F D Eb F — 41 38 39 41. The walk moves with the chord.
        let score = bassed(
            WALK,
            prosody_core::Variant::Feet,
            prosody_core::Variant::Rhyme,
        );
        let text = Strudel::new().render(&score, &Style::default()).unwrap();
        let bass = bass_line(&text);
        assert_eq!(bar_of(bass, 0), "[36 39 38 36]", "{bass}");
        assert_eq!(bar_of(bass, 1), "[41 38 39 41]", "{bass}");
        assert!(text.contains("of the bar's chord"), "{text}");
    }

    /// Two stanzas of one layer — three iambs a line, then four — so the arrangement mixes bars of
    /// three and four beats. No line rhymes within its stanza, so nothing is held.
    const THREE_THEN_FOUR: &str = "deA deA deo\nteA teA teA\n\nbeA beA beA beo\nreA reA reA reA\n";

    /// `source` lowered with the pulse set to `pulse`, every other mapping at its default.
    fn pulsed(source: &str, pulse: prosody_core::Variant) -> (Poem, Score) {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let mappings =
            prosody_core::Mappings::all().with(prosody_core::Mapping::Pulse.choose(pulse).unwrap());
        let score = lower::run_with(
            Analyses {
                poem: &poem,
                meter: &meter,
                rhyme: &rhyme::analyse(&poem, &Spelled),
                repetition: &repetition,
                sound: &sound::analyse(&poem, &Spelled),
                surprisal: &surprisal::analyse(&poem, &Spelled, &ByLength),
                roles: &roles,
            },
            &mappings,
        )
        .unwrap();
        (poem, score)
    }

    /// The `@` weights of a sequence's top-level elements, in order: how many cycles each bar
    /// lasts. Empty when it has none, as under the elastic pulse.
    fn weights_in(sequence: &str) -> Vec<u64> {
        let inner = sequence.trim_start_matches('<').trim_end_matches('>');
        let mut depth = 0_i32;
        let mut out = Vec::new();
        let mut current = String::new();
        for character in inner.chars().chain(std::iter::once(' ')) {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                ' ' if depth == 0 => {
                    if let Some((_, weight)) = current.rsplit_once('@') {
                        out.push(weight.parse().unwrap());
                    }
                    current.clear();
                    continue;
                }
                _ => {}
            }
            current.push(character);
        }
        out
    }

    fn at_120() -> Style {
        Style::parse("+++\ntempo_range = [120, 120]\n+++").unwrap()
    }

    #[test]
    fn under_the_steady_pulse_each_bar_lasts_its_beats() {
        // Hand-worked from `prosody_core::arrange`, a slot taking its host's stanza's period:
        // stanza 1 stated (slots 0, 1: three beats each); stanza 2 transforms its loop a line a
        // pass — slot 2 hosts its first line arriving (four), slot 3 the old loop's second line
        // yielding (three), slot 4 the new first line looping, slot 5 the second arriving (four
        // each); the outro plays stanza 2 again (four, four). A cycle is a beat, so the bars are
        // weighted 3 3 4 3 4 4 4 4 and begin on beats 0 3 6 10 13 17 21 25; the piece is 29.
        let (_, score) = pulsed(THREE_THEN_FOUR, prosody_core::Variant::Steady);
        assert_eq!(super::weights(&score), vec![3, 3, 4, 3, 4, 4, 4, 4]);
        assert_eq!(super::starts(&score), vec![0, 3, 6, 10, 13, 17, 21, 25, 29]);
        let text = Strudel::new().render(&score, &at_120()).unwrap();
        // Every pattern of the piece carries the same weights — the drums, and every parameter.
        for line in text
            .lines()
            .filter(|l| l.starts_with("  s(") || l.starts_with("  note("))
        {
            let sequence = line.split('"').nth(1).unwrap();
            assert_eq!(weights_in(sequence), vec![3, 3, 4, 3, 4, 4, 4, 4], "{line}");
            for parameter in line.split(").").skip(1) {
                if let Some(inner) = parameter.split('"').nth(1).filter(|p| p.starts_with('<')) {
                    assert_eq!(
                        weights_in(inner),
                        vec![3, 3, 4, 3, 4, 4, 4, 4],
                        "{parameter}"
                    );
                }
            }
        }
        // ...at the tempo itself: a cycle is a beat, so one beat lasts 60 / 120 = 0.5 s, a bar of
        // three 1.5 s and one of four 2 s.
        assert!(text.ends_with("  .cpm(120.000)\n"), "{text}");
        assert!(
            text.contains(
                "//   pulse = steady: a cycle is a beat at 120 bpm, and every bar lasts its beats \
                 (bars of 3, 4 beats)"
            ),
            "{text}"
        );
        assert_eq!(
            fmt_param(super::cycles_per_minute(&score, &at_120())),
            "120.000"
        );
    }

    #[test]
    fn under_the_elastic_pulse_every_bar_lasts_one_cycle() {
        // The same piece as the pattern was: 29 beats over 8 slots, integer mean 3, so one cycle a
        // bar at 120 / 3 = 40 a minute — 1.5 s a bar whatever its length. A four-beat bar plays
        // its beats in 1.5 / 4 = 0.375 s, which is 160 bpm; a three-beat bar at 120.
        let (_, score) = pulsed(THREE_THEN_FOUR, prosody_core::Variant::Elastic);
        assert_eq!(super::weights(&score), vec![1; 8]);
        assert_eq!(super::starts(&score), (0..=8).collect::<Vec<u64>>());
        let text = Strudel::new().render(&score, &at_120()).unwrap();
        let drums = text.lines().find(|l| l.starts_with("  s(")).unwrap();
        assert!(!drums.contains('@'), "{drums}");
        assert!(text.ends_with("  .cpm(40.000)\n"), "{text}");
        assert!(
            text.contains("so the beat follows the bar's length: 3 beats at 120.000 bpm, 4 beats at 160.000 bpm"),
            "{text}"
        );
        // Both pulses play the same bars, beats and subdivisions: only how long a bar lasts
        // differs.
        let steady = Strudel::new()
            .render(
                &pulsed(THREE_THEN_FOUR, prosody_core::Variant::Steady).1,
                &at_120(),
            )
            .unwrap();
        let unweighted = |text: &str| -> String {
            text.lines()
                .filter(|l| l.starts_with("  ") && !l.starts_with("  .cpm"))
                .map(|l| {
                    let mut out = String::new();
                    let mut skipping = false;
                    for c in l.chars() {
                        if c == '@' {
                            skipping = true;
                        } else if skipping && c.is_ascii_digit() {
                        } else {
                            skipping = false;
                            out.push(c);
                        }
                    }
                    out
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(unweighted(&text), unweighted(&steady));
    }

    #[test]
    fn swing_slices_every_beat_under_either_pulse() {
        // `swingBy(x, n)` slices a *cycle* into n. Steady: a cycle is a beat, so n = 1 swings
        // every beat of a three- and a four-beat bar alike. Elastic: a cycle is a bar, sliced by
        // the integer mean, 3 — which is a beat only in a bar of three.
        let swung = Style::parse("+++\ntempo_range = [120, 120]\nswing = 0.33\n+++").unwrap();
        let steady = Strudel::new()
            .render(
                &pulsed(THREE_THEN_FOUR, prosody_core::Variant::Steady).1,
                &swung,
            )
            .unwrap();
        assert!(
            steady.ends_with("  .swingBy(0.330, 1)\n  .cpm(120.000)\n"),
            "{steady}"
        );
        let elastic = Strudel::new()
            .render(
                &pulsed(THREE_THEN_FOUR, prosody_core::Variant::Elastic).1,
                &swung,
            )
            .unwrap();
        assert!(
            elastic.ends_with("  .swingBy(0.330, 3)\n  .cpm(40.000)\n"),
            "{elastic}"
        );
    }

    #[test]
    fn the_trace_and_the_reading_land_on_the_musics_bars_under_either_pulse() {
        for (pulse, weights, cpm) in [
            (
                prosody_core::Variant::Steady,
                vec![3, 3, 4, 3, 4, 4, 4, 4],
                "120.000",
            ),
            (prosody_core::Variant::Elastic, vec![1; 8], "40.000"),
        ] {
            let (poem, score) = pulsed(THREE_THEN_FOUR, pulse);
            let music = Strudel::new().render(&score, &at_120()).unwrap();
            // The trace: the music's bar starts, the music's clock, and the music's weights on
            // every lane.
            let trace = crate::trace::plan(&score, &at_120());
            assert_eq!(trace.bars(), super::starts(&score).as_slice(), "{pulse:?}");
            assert_eq!(fmt_param(trace.cycles_per_minute()), cpm);
            assert_eq!(
                trace.pattern().lines().last(),
                music.lines().last(),
                "{pulse:?}"
            );
            for lane in trace.pattern().lines().filter(|l| l.starts_with("  n(")) {
                let sequence = lane.split('"').nth(1).unwrap();
                let found = weights_in(sequence);
                if pulse == prosody_core::Variant::Steady {
                    assert_eq!(found, weights, "{lane}");
                } else {
                    assert!(found.is_empty(), "{lane}");
                }
            }
            // The reading: each cue in the slot where its line arrives, weighted as the music's
            // bars, at the music's cycles a minute — so it fires on that bar's first beat.
            let reading =
                crate::reading::plan(&poem, &score, &at_120(), &prosody_core::Mappings::all())
                    .unwrap();
            let slots: Vec<usize> = reading.cues().iter().map(crate::Cue::slot).collect();
            assert_eq!(slots, vec![0, 1, 2, 5], "{pulse:?}");
            let cue_pattern = reading.pattern();
            let last = cue_pattern.lines().last().unwrap();
            assert!(last.ends_with(&format!(".cpm({cpm})")), "{last}");
            let sequence = last.split('"').nth(1).unwrap();
            if pulse == prosody_core::Variant::Steady {
                assert_eq!(sequence, "<0@3 1@3 2@4 ~@3 ~@4 3@4 ~@4 ~@4>");
            } else {
                assert_eq!(sequence, "<0 1 2 ~ ~ 3 ~ ~>");
            }
            // The rate is fitted to the bar the line is spoken in. Steady: six syllables in a
            // three-beat bar, 1.5 s, and eight in a four-beat bar, 2 s — 1.5 / (0.85 × 1.5) and
            // 2 / (0.85 × 2), both 1.176. Elastic: every bar 1.5 s, so the eight-syllable line
            // is hurried, 2 / (0.85 × 1.5) = 1.569.
            let rates: Vec<String> = reading.cues().iter().map(|c| fmt_param(c.rate())).collect();
            if pulse == prosody_core::Variant::Steady {
                assert_eq!(rates, vec!["1.176", "1.176", "1.176", "1.176"]);
            } else {
                assert_eq!(rates, vec!["1.176", "1.176", "1.569", "1.569"]);
            }
        }
    }

    /// ABAB, the first A line opening on an anapaest — `dee-e-A deA`, `u u S | u S` — and the
    /// others two iambs, so the held A line sounds three against two under the B line in slot 1.
    const HELD_ANAPAEST: &str = "deeA deA\nbeo beo\nteA teA\nreo reo\n";

    /// `source` lowered with the stack set to `stack`, every other mapping at its default.
    fn stacked(source: &str, stack: prosody_core::Variant) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let mappings =
            prosody_core::Mappings::all().with(prosody_core::Mapping::Stack.choose(stack).unwrap());
        lower::run_with(
            Analyses {
                poem: &poem,
                meter: &meter,
                rhyme: &rhyme::analyse(&poem, &Spelled),
                repetition: &repetition,
                sound: &sound::analyse(&poem, &Spelled),
                surprisal: &surprisal::analyse(&poem, &Spelled, &ByLength),
                roles: &roles,
            },
            &mappings,
        )
        .unwrap()
    }

    /// The `index`-th bar of a `<…>` sequence, its `@` weight removed, with every value replaced
    /// by `x` — the rhythm alone, so the music and the trace can be compared.
    fn shape_of(sequence: &str, index: usize) -> String {
        let inner = sequence.trim_start_matches('<').trim_end_matches('>');
        let mut bars = Vec::new();
        let (mut depth, mut current) = (0_i32, String::new());
        for character in inner.chars() {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                ' ' if depth == 0 => {
                    bars.push(std::mem::take(&mut current));
                    continue;
                }
                _ => {}
            }
            current.push(character);
        }
        bars.push(current);
        let bar = bars[index].clone();
        let bar = match bar.rfind('@') {
            Some(at) if !bar[at..].contains(']') => bar[..at].to_owned(),
            _ => bar,
        };
        // Each step's value becomes `x`, its weight kept: `[bd@2 bd bd]` → `[x@2 x x]`.
        let mut out = String::new();
        let mut in_value = false;
        let mut in_weight = false;
        for character in bar.chars() {
            match character {
                '[' | ']' | ' ' => {
                    in_value = false;
                    in_weight = false;
                    out.push(character);
                }
                '@' => {
                    in_weight = true;
                    out.push('@');
                }
                '~' if !in_value => out.push('~'),
                _ if in_weight => out.push(character),
                _ if !in_value => {
                    in_value = true;
                    out.push('x');
                }
                _ => {}
            }
        }
        out
    }

    /// The `index`-th sequence written as `call("…")` in a rendered pattern or trace, in order.
    fn nth_call(text: &str, call: &str, index: usize) -> String {
        let needle = format!("{call}(\"");
        let lines: Vec<&str> = text.lines().filter(|l| l.contains(&needle)).collect();
        let line = lines
            .get(index)
            .unwrap_or_else(|| panic!("no {call} {index} in:\n{text}"));
        let start = line.find(&needle).unwrap() + needle.len();
        let rest = &line[start..];
        rest[..rest.find('"').unwrap()].to_owned()
    }

    #[test]
    fn straight_fits_a_held_triplet_to_its_host_in_the_pattern_and_the_trace() {
        let crossing = stacked(HELD_ANAPAEST, prosody_core::Variant::Crossing);
        let straight = stacked(HELD_ANAPAEST, prosody_core::Variant::Straight);
        let style = at_120();
        let (cross_text, straight_text) = (
            Strudel::new().render(&crossing, &style).unwrap(),
            Strudel::new().render(&straight, &style).unwrap(),
        );
        // Every line is its own voice, so the A line's drums are the first `s(…)`. Slot 1: the B
        // line hosts, `[x x] [x x]`, and the A line is held under it. Crossing, its anapaest
        // divides beat 0 in three. Straight, by hand: the first syllable takes the host's first
        // half (`@2` of four parts), the other two share the second — onsets 0, ½, ¾.
        assert_eq!(
            shape_of(&nth_call(&cross_text, "s", 0), 1),
            "[[x x x] [x x]]"
        );
        assert_eq!(
            shape_of(&nth_call(&straight_text, "s", 0), 1),
            "[[x@2 x x] [x x]]"
        );
        assert_eq!(
            shape_of(&nth_call(&straight_text, "s", 1), 1),
            "[[x x] [x x]]"
        );
        // Where the A line hosts — slot 0, and slot 4 in the outro — nothing is under it, and
        // it keeps its triplet.
        assert_eq!(
            shape_of(&nth_call(&straight_text, "s", 0), 0),
            "[[x x x] [x x]]"
        );
        assert_eq!(
            shape_of(&nth_call(&straight_text, "s", 0), 4),
            "[[x x x] [x x]]"
        );
        // Every parameter of the lane is written on the same grid, so each meets one event.
        let lane = straight_text.lines().find(|l| l.contains("s(\"")).unwrap();
        for parameter in lane.split(").").skip(1) {
            if let Some(inner) = parameter.split('"').nth(1).filter(|p| p.starts_with('<')) {
                assert_eq!(shape_of(inner, 1), "[[x@2 x x] [x x]]", "{parameter}");
            }
        }
        // Only the straight header names the stack, and counts the one beat it fitted.
        assert!(!cross_text.contains("stack ="), "{cross_text}");
        assert!(
            straight_text.contains("//   stack = straight: a line stacked under a bar's line is fitted to that line's subdivision wherever the two would cross, in 1 beat(s)"),
            "{straight_text}"
        );
        // The trace is the music's walk: each of its lanes has the music's rhythm, slot by slot.
        let trace = crate::trace::plan(&straight, &style);
        for voice in 0..4 {
            let traced = nth_call(trace.pattern(), "n", voice);
            let music = nth_call(&straight_text, "s", voice);
            for slot in 0..straight.arrangement().slots().len() {
                assert_eq!(
                    shape_of(&traced, slot),
                    shape_of(&music, slot),
                    "{voice} {slot}"
                );
            }
        }
        // Nothing else changed: besides the header, the A line's drums and the bass's second
        // lane — the held line's bass note on its stressed third syllable, now at ¾ of the beat
        // (`[~ ~ ~ 36]`, the long first syllable's rest written as its two parts).
        let differing: Vec<&str> = cross_text
            .lines()
            .zip(straight_text.lines().filter(|l| !l.contains("stack =")))
            .filter(|(a, b)| a != b)
            .map(|(_, b)| b)
            .collect();
        assert_eq!(differing.len(), 2, "{differing:?}");
        assert!(differing[0].starts_with("  s(\"<[[bd bd bd] [bd bd]]@2 [[bd@2 bd bd]"));
        assert!(
            differing[1].starts_with("  note(\"<~@2 [[~ ~ ~ 36] ~]@2"),
            "{}",
            differing[1]
        );
    }

    #[test]
    fn the_osc_schedule_sounds_the_fitted_onsets() {
        // Every slot lasts two beats; slot 1 starts on beat 2. The held A line's first beat there
        // is 2, 2⅓, 2⅔ crossing and 2, 2½, 2¾ straight; its second beat 3 and 3½ either way.
        let onsets = |stack| {
            let schedule = crate::Backend::render(
                &crate::Osc::new(),
                &stacked(HELD_ANAPAEST, stack),
                &at_120(),
            )
            .unwrap();
            let mut at: Vec<String> = schedule
                .messages()
                .iter()
                .map(|m| fmt_param(m.at().as_beats()))
                .filter(|at| at.starts_with("2.") || at.starts_with("3."))
                .collect();
            at.dedup();
            (at, schedule)
        };
        let (crossing, crossing_schedule) = onsets(prosody_core::Variant::Crossing);
        let (straight, straight_schedule) = onsets(prosody_core::Variant::Straight);
        // The host's iambs sound at 2, 2½, 3, 3½ under both.
        assert_eq!(
            crossing,
            vec!["2.000", "2.333", "2.500", "2.667", "3.000", "3.500"]
        );
        assert_eq!(straight, vec!["2.000", "2.500", "2.750", "3.000", "3.500"]);
        assert_eq!(crossing_schedule.straightened(), None);
        assert_eq!(straight_schedule.straightened(), Some(1));
        assert!(straight_schedule.to_text().contains(
            "# stack = straight: a stacked line fitted to its bar's line's subdivision in 1 beat(s)"
        ));
        assert_eq!(
            crossing_schedule.messages().len(),
            straight_schedule.messages().len()
        );
    }

    #[test]
    fn the_osc_schedule_counts_the_steady_bars_and_says_it_is_not_elastic() {
        // OSC is in beats, so its bars are the steady pulse's whichever is chosen: every bar
        // begins with its host line's first syllable, on beats 0 3 6 10 13 17 21 25.
        for pulse in [
            prosody_core::Variant::Steady,
            prosody_core::Variant::Elastic,
        ] {
            let (_, score) = pulsed(THREE_THEN_FOUR, pulse);
            let schedule = crate::Backend::render(&crate::Osc::new(), &score, &at_120()).unwrap();
            let onsets: Vec<f64> = schedule
                .messages()
                .iter()
                .map(|m| m.at().as_beats())
                .collect();
            for start in [0.0, 3.0, 6.0, 10.0, 13.0, 17.0, 21.0, 25.0] {
                assert!(
                    onsets.iter().any(|at| (at - start).abs() < 1e-9),
                    "no onset on beat {start}"
                );
            }
            let steady_starts: Vec<f64> =
                super::starts(&pulsed(THREE_THEN_FOUR, prosody_core::Variant::Steady).1)
                    .into_iter()
                    .map(|beats| f64::from(u32::try_from(beats).unwrap()))
                    .collect();
            assert!(onsets.iter().all(|at| *at < steady_starts[8]), "{pulse:?}");
            let elastic = pulse == prosody_core::Variant::Elastic;
            assert_eq!(schedule.elastic_unapplied(), elastic);
            assert_eq!(
                schedule
                    .to_text()
                    .contains("# not applied here: pulse = elastic"),
                elastic
            );
        }
    }
}
