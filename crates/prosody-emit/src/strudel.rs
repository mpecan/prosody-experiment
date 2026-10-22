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
//! | a rhyme note's chord tones | the same degree, stacked in the style's scale ([`Voicing`]) | §5 |
//! | `.lpf`, `.attack`, `.room` | `Event::sharpness` | §4.4 |
//! | `.vowel` | `Event::bank` — the alliterating phoneme | §4.3, alliteration |
//! | `.crush` depth | `Event::family` — distance from the family's root | §4.3, morphology |
//! | which sample, which instrument, tempo, swing | [`Style`] | §5 |
//! | `.room` floor, `.roomsize`, `.delay`, `.shape`, `.compressor`, `.postgain` | [`Style::mix`] | §5, §9 "tuning the ranges" |
//! | `.nudge`, `.velocity` — the human stray | each line's own seed, scaled by [`Style::mix`] | §6 I2 |
//!
//! # Three pitched and percussive roles
//!
//! Every line's voice is a drum (§4.1's foundation and flourish). The pitched events split by what
//! pitched them — [`PitchSource`] — into the **melody** (a rhyme word, §4.5) and the **bass** (a
//! stressed foundation vowel). They are separate layers only so that each can have its own
//! instrument and level; which events exist and where they fall is the score's, and splitting them
//! moves nothing.
//!
//! `Event::anchor` (§4.3's anaphora and epistrophe rows) is **not** rendered: Strudel places a
//! step by its position in the cycle, so "downbeat-anchored" has nowhere to go that the position
//! has not already decided. It is carried in the IR for the OSC backend, which schedules
//! absolutely. Recorded in `PLAN.md` rather than left to be rediscovered.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use prosody_core::VoiceId;
use prosody_core::role::Layer;
use prosody_core::voice::{Event, PitchSource, Score, Voice};
use rand_core::Rng as _;

use crate::style::{Kit, Named, Style, Voicing};
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

        // `cpm` is *cycles* per minute and a cycle is a bar, so the tempo in beats has to be
        // divided by how many beats a bar holds. Emitting the BPM directly ran every poem at
        // roughly five times its stated tempo, which made `SPEC.md` §5's window decorative.
        let tempo = style
            .tempo_range()
            .at_density(score.events().len(), score.bars());

        let mut out = String::new();
        header(&mut out, score, style);

        let strays = strays(score);
        let mut layers: Vec<(String, String)> = Vec::new();
        for voice in score.voices() {
            layers.extend(rhythm_layer(score, style, &strays, *voice));
        }
        for role in [Pitched::Melody, Pitched::Bass] {
            layers.extend(pitched_layer(score, style, &strays, role, tempo.get()));
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

        let _ = writeln!(
            out,
            "  .cpm({})",
            fmt_param(cycles_per_minute(score, style))
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
    /// The stressed foundation vowels.
    Bass,
}

impl Pitched {
    /// Whether this role sounds an event pitched from `source`.
    const fn plays(self, source: PitchSource) -> bool {
        matches!(
            (self, source),
            (Self::Melody, PitchSource::Rhyme(_)) | (Self::Bass, PitchSource::Vowel(_))
        )
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

/// How many cycles — slots of the arrangement — the piece plays a minute.
///
/// One number for every consumer that has to land on the same clock: the pattern's own `.cpm`,
/// and the reading ([`crate::reading`]), whose cues must fall on the bars they name.
pub(crate) fn cycles_per_minute(score: &Score, style: &Style) -> f64 {
    let tempo = style
        .tempo_range()
        .at_density(score.events().len(), score.bars());
    f64::from(tempo.get()) / f64::from(mean_beats(score).max(1))
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
    score
        .arrangement()
        .slots()
        .iter()
        .flat_map(prosody_core::Slot::placements)
        .map(|placement| score.placed(*placement).events().len())
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
    // The translation from the palette's words to Strudel's sounds, so `karplus` in a header is
    // not a mystery in the output.
    let ride = ride_stand_in(style).map_or_else(
        || "ride".to_owned(),
        |bank| format!("open hat in place of the ride, which {bank} does not have,"),
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
    let humanise = style.mix().humanise();
    if !humanise.is_off() {
        let _ = writeln!(
            out,
            "//   humanise: each syllable up to {} s late and {} softer, drawn from its own line's seed",
            fmt_param(humanise.as_f64() * MAX_LATE),
            fmt_param(humanise.as_f64() * MAX_SOFTEN)
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

/// One voice: every syllable it sounds, and what §4.3 and §4.4 do to each.
fn rhythm_layer(
    score: &Score,
    style: &Style,
    strays: &BTreeMap<EventKey, Stray>,
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
    for lane in Lane::all(score, &events) {
        let mut layer = format!("  s(\"{}\")", grid(lane, &|_| sample.to_owned(), "~"));
        // §4.2: stress becomes accent.
        let _ = write!(layer, ".gain(\"{}\")", gains(lane));
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
/// sound nobody can trace (I4). Such a bank plays its open hat instead, the drum machine's usual
/// stand-in for a ride, and the pattern's header says so ([`ride_stand_in`]).
fn sample_for(style: &Style, layer: Layer) -> &'static str {
    match (style.kit(), layer) {
        (Kit::Machine, Layer::Foundation) => "bd",
        (Kit::Machine, Layer::Flourish) => "hh",
        (Kit::Ride, Layer::Foundation) => {
            if bank_has(strudel_bank(style.bank()), RIDE) {
                RIDE
            } else {
                RIDE_STAND_IN
            }
        }
        (Kit::Ride, Layer::Flourish) => "sd",
    }
}

/// The ride cymbal's sample name in the drum-machine sample map.
const RIDE: &str = "rd";

/// What a bank without a ride plays in its place: the open hat, which every bank has.
const RIDE_STAND_IN: &str = "oh";

/// `Some(bank)` when the style asks for a ride its bank does not have, so the header can say the
/// open hat stands in — `CLAUDE.md` §6: a fallback must be visible.
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
struct Lane<'a> {
    score: &'a Score,
    events: &'a [&'a Event],
    index: usize,
}

impl<'a> Lane<'a> {
    /// Every lane a layer of these events needs: as many as the most lines it sounds in one bar.
    fn all(score: &'a Score, events: &'a [&'a Event]) -> Vec<Self> {
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
                        bars.contains(&placement.bar().get())
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
            })
            .collect()
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
    // loops or yields (`prosody_core::arrange`). A cycle is a slot; a line divides it into the
    // slot's beats, or its own when it has more — which is the bar-locked fitting. Lines sounding
    // together are *not* stacked with `,` inside the pattern: see [`Lane`]. This pattern carries
    // the `lane.index`-th of them, and rests where a bar has fewer.
    let mut bars = Vec::new();
    for slot in score.arrangement().slots() {
        let lines: Vec<String> = slot
            .placements()
            .iter()
            .filter_map(|placement| {
                // A whole line, or a call lifted out of one: its own events, its beats counted
                // from where it starts. A line's width comes from the score, not from the events
                // in *this* pattern, so the melody and the rhythm divide it the same way and a
                // note lands on its own syllable.
                let placed = score.placed(*placement);
                let present: BTreeMap<usize, Vec<&Event>> = by_bar
                    .get(&placement.bar().get())?
                    .iter()
                    .filter_map(|(beat, syllables)| {
                        let covered: Vec<&Event> = syllables
                            .iter()
                            .copied()
                            .filter(|event| placement.part().covers(event.span()))
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
                (!present.is_empty()).then(|| line(&present, steps, value, absent))
            })
            .collect();
        bars.push(
            lines
                .into_iter()
                .nth(lane.index)
                .unwrap_or_else(|| "~".to_owned()),
        );
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
fn gains(lane: Lane<'_>) -> String {
    grid(lane, &|event| fmt_param(event.velocity().as_f64()), "~")
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
    role: Pitched,
    tempo: u16,
) -> Vec<(String, String)> {
    let pitched: Vec<&Event> = score
        .events()
        .iter()
        .filter(|event| {
            event
                .pitch()
                .is_some_and(|pitch| role.plays(pitch.source()))
        })
        .collect();
    if pitched.is_empty() {
        return Vec::new();
    }
    // Which octave a role sits in is fixed here, in the backend, not read from the palette: the
    // bass at the second (MIDI 36), where a bass instrument has its body, and the melody two
    // octaves above (MIDI 60), where keys and mallets sing. It moves no event, so it could become a
    // `Style` field; no header key sets it yet (SPEC.md §5). Before the roles were split both sat
    // at 36, so the rhyme melody was in the bass.
    let base = match role {
        Pitched::Melody => 60,
        Pitched::Bass => 36,
    };

    // The melody may be voiced as chords (`Voicing`, palette — its doc argues it against I3); the
    // bass is always one note, the root a chord would stand on.
    let voicing = match role {
        Pitched::Melody => style.voicing(),
        Pitched::Bass => Voicing::Note,
    };
    let note = |event: &Event| {
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
        Pitched::Bass => "the bass — every stressed foundation vowel, by its height",
    };

    let mut layers = Vec::new();
    for lane in Lane::all(score, &pitched) {
        let mut layer = format!("  note(\"{}\")", grid(lane, &note, "~"));
        let _ = write!(layer, ".sound(\"{instrument}\")");
        let _ = write!(layer, ".gain(\"{}\")", gains(lane));
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
        layers.push((layer, lane.label(name)));
    }
    layers
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
    use crate::style::{BANKS, KITS, Style};

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
        // two syllables; two of them is two steps. One cycle for the line, one for the outro.
        let text = render("eA eA\n", &Style::default());
        assert_eq!(
            argument(&text, "s"),
            "<[[bd bd] [bd bd]] [[bd bd] [bd bd]]>"
        );
    }

    #[test]
    fn stress_becomes_gain_and_all_three_levels_survive() {
        // Hand-worked: `e` unstressed is a third, `i` secondary two thirds, `A` primary full.
        // Twice: the line, then the outro.
        let text = render("eiA\n", &Style::default());
        assert_eq!(
            argument(&text, "gain"),
            "<[[0.333 0.667 1.000]] [[0.333 0.667 1.000]]>"
        );
    }

    #[test]
    fn a_bar_a_voice_does_not_play_is_a_rest_not_a_shift() {
        // A voice must not have its later bars pulled forward into the gap, or every line after a
        // silence plays at the wrong time.
        // Voice 0 plays bar 0 and rests in bar 1 — in the statement and again in the outro.
        let text = render("bA\ntA\n", &Style::default());
        assert_eq!(argument(&text, "s"), "<bd ~ bd ~>", "{text}");
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
        assert!(
            first.starts_with("  s(\"<[hh ~ ~] ~ hh ~ hh ~>\")"),
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
                "  s(\"<[[bd bd] [bd bd]] [[bd bd] ~] [[bd bd] [bd bd]] [[bd bd] ~]>\")"
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
        for bank in BANKS {
            for kit in KITS {
                let style =
                    Style::parse(&format!("+++\nkit = \"{kit}\"\nbank = \"{bank}\"\n+++")).unwrap();
                let strudel = strudel_bank(style.bank());
                assert!(
                    BANK_SAMPLES.iter().any(|(name, _)| *name == strudel),
                    "{bank} → {strudel} is not in the sample table"
                );
                for layer in [Layer::Foundation, Layer::Flourish] {
                    let sample = sample_for(&style, layer);
                    assert!(
                        bank_has(strudel, sample),
                        "{kit} on {bank} asks {strudel} for `{sample}` ({layer:?})"
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
}
