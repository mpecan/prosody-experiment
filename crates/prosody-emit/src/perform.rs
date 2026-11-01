//! The Performance backend: every note of the piece, resolved (`docs/player.md` §3).
//!
//! Where [`Strudel`](crate::Strudel) writes a pattern for Strudel's runtime to resolve while it
//! plays, this resolves it now: each note's onset and length in seconds, after swing and nudge, its
//! sound, its pitch, every parameter, and the token [`Span`] it came from (I4). Nothing is left to
//! decide at play time, which is what lets a player render ahead of the playhead rather than ask a
//! busy page for its notes just in time (`docs/player.md` §1).
//!
//! # One source of the numbers
//!
//! The two backends must mean the same piece, note for note. So every value an event is given —
//! how loud, how bright, which drum, which pitch, how wet — is worked out once, by a function
//! here, and the Strudel emitter prints what that function says. Two backends that each worked out
//! "how loud is this note" would drift; one function both call cannot. The same holds for *where*
//! a note falls: the Strudel emitter writes the steps of each line (`strudel::lines`) and this
//! times the very same steps (`strudel::Line::timed`), with the same clock helpers (`weights`, `starts`,
//! `cycles_per_minute`).
//!
//! What the text prints at three decimals ([`fmt_param`]) the Performance carries at the same three
//! decimals (`quantised`): the text is the contract, and a Performance more precise than the text
//! would be a different piece by a thousandth of a decibel — and would fail the parity test
//! (`web/test/parity.test.ts`), which plays both and compares them.
//!
//! # Time
//!
//! Exact until the end. A note's place in the pattern is an exact fraction of a cycle
//! ([`Cycles`]); swing moves it by an exact fraction; the tempo the text prints (`.cpm`, at three
//! decimals) is an exact number of thousandths; and the nudge the text prints is an exact number of
//! milliseconds. So every onset is an exact number of seconds ([`ExactSeconds`]), notes are sorted
//! without a float in sight (`CLAUDE.md` §4), and each is turned into an `f64` once, when it is
//! read. Cycles and seconds are different types (`CLAUDE.md` §5): the piece's clock is the one
//! place that turns one into the other, so a place in the pattern cannot be read as a time.
//!
//! **Swing** follows Strudel's documented `swingBy(x, n)`: each cycle is cut into `n` slices, and
//! an event in the second half of a slice is delayed by `x` of a half-slice. The text writes that
//! meaning itself rather than calling `swingBy`, whose output departs from it — see [`Swing`].

use std::collections::BTreeMap;

use prosody_core::mapping::Variant;
use prosody_core::role::Layer;
use prosody_core::voice::{Anchor, Event, Score};
use prosody_core::{BassNote, Chord, Chromatic, Drum, Harmony, Span, VoiceId};
use rand_core::Rng as _;

use crate::reading::{self, Reading};
use crate::strudel;
use crate::style::{Amount, BANKS, Kit, Named, RideFallback, Style, Voicing};
use crate::{Backend, fmt_param};

// ---------------------------------------------------------------------------------------------
// The values: what each event is given. Both backends call these.
// ---------------------------------------------------------------------------------------------

/// A value as the text prints it, at three decimals: the number Strudel will read back.
///
/// The Performance carries this rather than the unrounded value, so it means exactly what the
/// pattern says (see the module docs). Parsing the printed form is deliberate: it is the only way
/// to round *exactly* as [`fmt_param`] does, ties and all.
#[must_use]
pub(crate) fn quantised(value: f64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    // a thousandths count of a printed parameter, far below 2^53
    let thousandths = thousandths(value) as f64;
    thousandths / 1000.0
}

/// A value as the text prints it, in whole thousandths: `0.350` is 350.
fn thousandths(value: f64) -> i64 {
    let printed = fmt_param(value);
    let digits: String = printed.chars().filter(|c| *c != '.').collect();
    digits.parse().unwrap_or(0)
}

/// The colour/role a note is drawn in — its lane on the page's roll and its colour in the lit poem,
/// so a syllable lighting up and the note it plays read as one thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    /// §4.1's pulse.
    Foundation,
    /// §4.1's ornament.
    Flourish,
    /// The bass (§4.6).
    Bass,
    /// The rhyme melody (§4.5).
    Melody,
    /// The harmony layer's chords (§4.6).
    Chords,
}

impl Lane {
    /// Every lane, in the order the page's roll lays them out.
    pub(crate) const ALL: [Self; 5] = [
        Self::Foundation,
        Self::Flourish,
        Self::Bass,
        Self::Melody,
        Self::Chords,
    ];

    /// The lane's name on the page: `SPEC.md` §4's words for it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Foundation => "pulse",
            Self::Flourish => "ornament",
            Self::Bass => "bass",
            Self::Melody => "rhyme",
            Self::Chords => "chords",
        }
    }

    /// Its colour, as CSS takes it.
    #[must_use]
    pub const fn hex(self) -> &'static str {
        match self {
            Self::Foundation => "#c9a6ff",
            Self::Flourish => "#6fd6c5",
            Self::Bass => "#ffb86b",
            Self::Melody => "#ff7aa8",
            Self::Chords => "#8fb3ff",
        }
    }

    /// The lane a voice's drums are drawn in.
    const fn of(layer: Layer) -> Self {
        match layer {
            Layer::Foundation => Self::Foundation,
            Layer::Flourish => Self::Flourish,
        }
    }
}

/// Which pitched role a layer plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pitched {
    /// The rhyme words (`SPEC.md` §4.5).
    Melody,
    /// The bass notes (§4.6): stressed foundation vowels, or a walking bass.
    Bass,
}

impl Pitched {
    /// Whether this role sounds `event`: the melody a rhyme note, the bass a bass note.
    pub(crate) const fn plays(self, event: &Event) -> bool {
        match self {
            Self::Melody => event.pitch().is_some(),
            Self::Bass => event.bass().is_some(),
        }
    }
}

/// What makes a note's sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// A drum sample: `drums/<bank>/<piece>.flac` beside the page, Strudel's `<bank>_<piece>`.
    Drum {
        /// The kit, one of [`BANKS`].
        bank: &'static str,
        /// The piece, one of [`DRUM_SAMPLES`].
        piece: &'static str,
    },
    /// A General MIDI soundfont, in the licensed variant `style::SOUNDFONT_VARIANTS` names.
    Font {
        /// The instrument, one of `style::SOUNDFONTS`.
        name: &'static str,
        /// Which of Strudel's variants of it: what follows the `:` in the pattern.
        variant: u8,
        /// The file that variant loads, without `.js`.
        file: &'static str,
    },
    /// A plain oscillator.
    Wave(Wave),
}

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wave {
    /// A sine.
    Sine,
    /// A sawtooth.
    Sawtooth,
    /// A square.
    Square,
    /// A triangle.
    Triangle,
}

impl Wave {
    /// Its name, as Strudel and the Performance's JSON say it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Sawtooth => "sawtooth",
            Self::Square => "square",
            Self::Triangle => "triangle",
        }
    }
}

impl Sound {
    /// What the pattern asks Strudel for: a drum's piece (its bank is a call of its own), a
    /// soundfont's `name:variant`, an oscillator's name.
    #[must_use]
    pub fn strudel(self) -> &'static str {
        match self {
            Self::Drum { piece, .. } => piece,
            Self::Font { name, .. } => {
                crate::style::soundfont_variant(name).map_or(name, |variant| variant.sound)
            }
            Self::Wave(wave) => wave.name(),
        }
    }
}

/// The Strudel sample bank a palette name plays from.
///
/// The page registers every kit under its own name ([`DRUM_SAMPLES`]), so the poem's word is
/// Strudel's: `.bank("uzu")` with `s("bd")` plays `uzu_bd`. It stays a step of its own because the
/// backend owns the palette-to-sound translation — what `sample_for`'s doc means by "which sounds
/// those are is palette" — and the drum machines this replaced did need one (`tr909` was
/// `RolandTR909`; passing the poem's word through unchanged played silence).
pub(crate) const fn strudel_bank(bank: Named) -> &'static str {
    bank.get()
}

/// The sound a palette instrument plays in a role.
///
/// A soundfont name (`gm_…`) is Strudel's own and passes through. `SPEC.md` §5's four synth names
/// are models Strudel does not have, so each takes the recorded instrument nearest the model,
/// per role: Karplus–Strong is a plucked string, so a plucked bass and a nylon guitar; FM's
/// signature sound is the DX7 electric piano and its synth bass; physical modelling's classic
/// case is a struck bar. An earlier version mapped all three to bare oscillators, which is why
/// every example poem sounded like a test tone. The plain waveforms stay what they say.
///
/// A soundfont plays its licensed variant (`style::SOUNDFONT_VARIANTS`), never Strudel's first,
/// unlicensed one.
pub(crate) fn instrument(instrument: Named, role: Pitched) -> Sound {
    let name = nearest_instrument(instrument.get(), role);
    if let Some(variant) = crate::style::soundfont_variant(name) {
        let index = variant
            .sound
            .split_once(':')
            .and_then(|(_, index)| index.parse().ok())
            .unwrap_or(0);
        return Sound::Font {
            name: variant.name,
            variant: index,
            file: variant.file,
        };
    }
    Sound::Wave(match name {
        "sine" => Wave::Sine,
        "sawtooth" => Wave::Sawtooth,
        "square" => Wave::Square,
        _ => Wave::Triangle,
    })
}

/// The instrument a palette name plays in a role, by name: a soundfont as named, a synth model as
/// its nearest recording (see [`instrument`]).
fn nearest_instrument(name: &'static str, role: Pitched) -> &'static str {
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

/// The sound the chord lane plays: a soundfont as named, a synth name as the melody would play it
/// — a chord is keys, not a bass.
pub(crate) fn chord_instrument(style: &Style) -> Sound {
    instrument(style.chord_sound(), Pitched::Melody)
}

/// Which sample a [`Drum`] plays under `style`'s [`Kit`] and [`RideFallback`].
///
/// `SPEC.md` §4.1 turns the layer into instrumentation: "low variance → periodic → foundation
/// layer (drums, bass, ostinato)" and "high variance → aperiodic → flourish layer (fills, stabs,
/// one-shots, ornament)". Which *sounds* those are is palette; that a foundation is the pulse and
/// a flourish ornaments it is structure, and comes from the score.
///
/// A bank without a ride would play the palette's [`RideFallback`] instead — its open hat by
/// default, or its crash — and the pattern's header says so ([`ride_stand_in`]): asking for a ride
/// it does not have would make Strudel log "sound not found" on every hit and the whole pulse
/// layer go silent, so a palette choice would in effect remove events (I3) and leave sound nobody
/// can trace (I4). Every bank in [`DRUM_SAMPLES`] has a ride, so today that never happens.
pub(crate) fn sample_for_drum(style: &Style, drum: Drum) -> &'static str {
    match (style.kit(), drum) {
        (Kit::Machine, Drum::Pulse) => "bd",
        (Kit::Ride, Drum::Pulse) => {
            if bank_has(strudel_bank(style.bank()), RIDE) {
                RIDE
            } else {
                stand_in(style.ride_fallback())
            }
        }
        (Kit::Machine, Drum::Ornament) | (_, Drum::Subdivision) => "hh",
        (Kit::Ride, Drum::Ornament) | (_, Drum::Backbeat) => "sd",
        (_, Drum::HighTom) => "ht",
        (_, Drum::MidTom) => "mt",
        (_, Drum::LowTom) => "lt",
    }
}

/// The ride cymbal's sample name in the drum-machine sample map.
const RIDE: &str = "rd";

/// What a bank without a ride plays in its place — the palette's [`RideFallback`]: the open hat or
/// the crash, both in [`DRUM_SAMPLES`].
const fn stand_in(fallback: RideFallback) -> &'static str {
    match fallback {
        RideFallback::OpenHat => "oh",
        RideFallback::Crash => "cr",
    }
}

/// `Some(bank)` when the style asks for a ride its bank does not have, so the header can say which
/// drum stands in — `CLAUDE.md` §6: a fallback must be visible.
pub(crate) fn ride_stand_in(style: &Style) -> Option<&'static str> {
    let bank = strudel_bank(style.bank());
    (style.kit() == Kit::Ride && !bank_has(bank, RIDE)).then_some(bank)
}

/// The drums every bank in [`BANKS`] has: one file each, `drums/<bank>/<drum>.flac` beside the
/// page, which the page registers as Strudel's `<bank>_<drum>`. Exactly what a kit can ask for
/// (`sample_for_drum`), and its ride stand-ins.
pub const DRUM_SAMPLES: &[&str] = &["bd", "sd", "hh", "oh", "rd", "cr", "lt", "mt", "ht"];

/// Whether a Strudel bank has a sample of that name.
pub(crate) fn bank_has(bank: &str, sample: &str) -> bool {
    BANKS.contains(&bank) && DRUM_SAMPLES.contains(&sample)
}

/// The key that identifies one event within the score, for looking up its stray.
pub(crate) type EventKey = (VoiceId, usize, usize, usize);

/// An event's [`EventKey`].
pub(crate) const fn key(event: &Event) -> EventKey {
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
pub(crate) struct Stray {
    pub(crate) late: f64,
    pub(crate) soft: f64,
}

/// The latest an event strays at `humanise = 1`, in seconds. Thirty milliseconds is a loose
/// human player; past it a stray stops sounding like feel and starts sounding like a mistake.
pub(crate) const MAX_LATE: f64 = 0.030;

/// The most an event is softened at `humanise = 1`. Only ever softer, never louder, so the stray
/// cannot push a mix that fits into clipping.
pub(crate) const MAX_SOFTEN: f64 = 0.30;

/// Every event's stray, drawn from its own line's seed.
///
/// `CLAUDE.md` §4 and invariant I2: the randomness is the voice's seed — derived from the line's
/// own text — under a child named for this decision, and the draws restart for every bar the voice
/// sounds in. So a line's strays depend on that line and nothing else: an edit in another stanza
/// cannot move them, and a refrain line, which shares its voice and its text, strays identically
/// every time it returns — §4.3's "literal loop". Only late, never early: Strudel's sampler starts
/// a nudged sample after its envelope opens, and an early start would sound before it, as a click.
pub(crate) fn strays(score: &Score) -> BTreeMap<EventKey, Stray> {
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

/// An event's stray, or none for an event the walk did not draw for.
fn stray_of(strays: &BTreeMap<EventKey, Stray>, event: &Event) -> Stray {
    strays.get(&key(event)).copied().unwrap_or(Stray {
        late: 0.0,
        soft: 0.0,
    })
}

/// How late an event is struck, in seconds — `.nudge` — or `None` when `humanise` is zero.
pub(crate) fn nudge(
    strays: &BTreeMap<EventKey, Stray>,
    event: &Event,
    style: &Style,
) -> Option<f64> {
    let amount = style.mix().humanise();
    (!amount.is_off()).then(|| stray_of(strays, event).late * amount.as_f64() * MAX_LATE)
}

/// How much softer an event is struck — `.velocity`, a factor on its gain — or `None` when
/// `humanise` is zero.
pub(crate) fn velocity(
    strays: &BTreeMap<EventKey, Stray>,
    event: &Event,
    style: &Style,
) -> Option<f64> {
    let amount = style.mix().humanise();
    (!amount.is_off()).then(|| 1.0 - stray_of(strays, event).soft * amount.as_f64() * MAX_SOFTEN)
}

/// How many lines sound in each slot of the arrangement, in slot order: every placement that plays
/// at least one event — a stated line, a stanza looping under the next, a held line, a call.
pub(crate) fn lines_sounding(score: &Score) -> Vec<usize> {
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
pub(crate) fn level_for(lines: usize, exponent: f64) -> f64 {
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
pub(crate) fn density(score: &Score, style: &Style) -> Vec<f64> {
    let exponent = style.mix().density_level().as_f64();
    if style.mix().density_level().is_off() {
        return vec![1.0; score.arrangement().slots().len()];
    }
    lines_sounding(score)
        .into_iter()
        .map(|lines| level_for(lines, exponent))
        .collect()
}

/// `SPEC.md` §4.2: stress becomes accent — times the bar's level under `density_level`, which is 1
/// unless the palette asks levels to follow density ([`density`]). The stress is `velocity`: the
/// event's own, or for the bass its note's, whose W2 note takes its foot's stressed syllable's
/// accent rather than the syllable it rides on.
///
/// §4.3's anaphora row under the anchor mapping: a downbeat-anchored ostinato is accented a
/// quarter harder.
pub(crate) fn gain(
    score: &Score,
    event: &Event,
    velocity: prosody_core::Velocity,
    level: f64,
) -> f64 {
    let vel = velocity.as_f64() * level;
    if score.anchor() == Variant::On && event.anchor() == Some(Anchor::Downbeat) {
        (vel * 1.25).min(1.0)
    } else {
        vel
    }
}

/// `SPEC.md` §4.4: distinctiveness becomes brightness.
///
/// 400 Hz up to the palette's high cut (4400 Hz by default) spans dull to bright without ever
/// closing the filter completely — a silent event would be an event that cannot be traced to its
/// word. The high cut scales the range rather than clipping it, so under a darker palette a
/// striking word is still brighter than a plain one (§9: tuning the ranges is palette work).
pub(crate) fn cutoff(score: &Score, event: &Event, style: &Style) -> f64 {
    let span = f64::from(style.mix().high_cut()) - 400.0;
    let base = 400.0 + event.sharpness().as_f64() * span;
    if score.anchor() == Variant::On && event.anchor() == Some(Anchor::Downbeat) {
        (base * 1.5).min(f64::from(style.mix().high_cut()))
    } else {
        base
    }
}

/// `SPEC.md` §4.4: distinctiveness becomes attack time, inverted — a sharper word strikes faster.
pub(crate) fn attack(score: &Score, event: &Event) -> f64 {
    if score.anchor() == Variant::On {
        match event.anchor() {
            Some(Anchor::Downbeat) => 0.005,
            Some(Anchor::Cadence) => 0.035,
            None => 0.04 * (1.0 - event.sharpness().as_f64()),
        }
    } else {
        0.04 * (1.0 - event.sharpness().as_f64())
    }
}

/// `SPEC.md` §4.4: distinctiveness becomes dry/wet — a striking word rings in more space.
///
/// The palette's reverb raises the floor the mapping stands on, at half strength for the drums so
/// a kick does not wash out; the rise with distinctiveness is the same under every palette.
pub(crate) fn drum_room(score: &Score, event: &Event, style: &Style) -> f64 {
    let floor = style.mix().reverb().as_f64() * 0.5;
    let base = floor + event.sharpness().as_f64() * 0.4;
    if score.anchor() == Variant::On && event.anchor() == Some(Anchor::Cadence) {
        (base + 0.35).min(0.95)
    } else {
        base
    }
}

/// `SPEC.md` §4.3's morphological row: "same voice, varied parameters".
///
/// Distance from the family's root becomes bit-crush depth, so "burn", "burnt" and "burning" are
/// audibly one instrument at three degrees of erosion. 16 bits is untouched; each phoneme of
/// difference takes two away, to a floor of 4 — still the same instrument, never a different one.
pub(crate) fn crush(event: &Event) -> u8 {
    let depth = event
        .family()
        .map_or(0, |family| family.depth().get())
        .min(6);
    // At most 6, so this is 4..=16 and always fits.
    16 - u8::try_from(depth).unwrap_or(6) * 2
}

/// A formant filter's vowel (`.vowel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vowel {
    /// /a/.
    A,
    /// /e/.
    E,
    /// /i/.
    I,
    /// /o/.
    O,
    /// /u/.
    U,
}

impl Vowel {
    /// As Strudel and the Performance's JSON write it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::E => "e",
            Self::I => "i",
            Self::O => "o",
            Self::U => "u",
        }
    }
}

/// Whether a voice's drums take a vowel at all: only when some word in it alliterates.
///
/// `SPEC.md` §4.3's alliteration row: "timbral continuity — same sample bank". When no word in the
/// voice alliterates the filter is not applied at all rather than applied neutrally — a poem with
/// no sound figures should sound untouched by them.
pub(crate) fn takes_vowels(events: &[&Event]) -> bool {
    events.iter().any(|event| event.bank().is_some())
}

/// The vowel colour an event of a voice that [`takes_vowels`] is filtered through: its
/// alliterating phoneme's, or `a` for a word outside the figure.
///
/// The mapping from phoneme to colour is arbitrary and deliberately so: what matters structurally
/// is that words sharing a phoneme share a colour, and *which* colour is palette.
pub(crate) fn vowel(event: &Event) -> Vowel {
    event.bank().map_or(Vowel::A, |phoneme| {
        match phoneme.symbol().as_bytes().first().copied().unwrap_or(b'A') % 5 {
            0 => Vowel::A,
            1 => Vowel::E,
            2 => Vowel::I,
            3 => Vowel::O,
            _ => Vowel::U,
        }
    })
}

/// The waveshaper's amount (`.shape`) on the drums and the bass, or `None` with no drive: soft
/// saturation at most half of Strudel's `shape`, past which it is distortion, not warmth, and gets
/// loud.
pub(crate) fn drive(style: &Style) -> Option<f64> {
    let drive = style.mix().drive();
    (!drive.is_off()).then(|| drive.as_f64() * 0.5)
}

/// The reverb's size (`.roomsize`), the same on every layer: reverb and echo are per orbit in
/// Strudel, not per event, so `roomsize` must be the same everywhere or each event would rebuild
/// the room.
pub(crate) fn room_size(style: &Style) -> f64 {
    f64::from(style.mix().room_size_tenths()) / 10.0
}

/// The per-layer compressor, `(threshold in dB, ratio)`, or `None` with compression off: from a
/// gentle −4 dB at 2:1 to −24 dB at 7:1. Strudel compresses each event's own chain; a fixed master
/// stage on the page (compressor, then a soft clipper) catches the sum.
pub(crate) fn compressor(style: &Style) -> Option<(f64, f64)> {
    let compress = style.mix().compress();
    (!compress.is_off()).then(|| {
        let amount = compress.as_f64();
        (-4.0 - 20.0 * amount, 2.0 + 5.0 * amount)
    })
}

/// A layer's level after its chain (`.postgain`): the palette's level for the role, shared between
/// the tones of a chord ([`chord_share`]).
pub(crate) fn postgain(level: Amount, share: f64) -> f64 {
    level.as_f64() * share
}

/// The level each tone of a chord takes, so a role's level is the level of the chord.
///
/// One over the square root of the tone count: soundfont tones at different pitches are
/// uncorrelated, so their powers add, and four tones at half level carry the power of one at full.
/// The melody level a palette sets therefore means the same thing under either voicing.
pub(crate) fn chord_share(voicing: Voicing) -> f64 {
    tone_share(voicing.degrees().len())
}

/// One over the square root of `tones`, at least one tone.
fn tone_share(tones: usize) -> f64 {
    let tones = u32::try_from(tones).unwrap_or(1).max(1);
    1.0 / f64::from(tones).sqrt()
}

/// How this piece's rhyme melody is voiced: the style's [`Voicing`], unless the score has a chord
/// lane to play over, and then one note a rhyme word whatever the voicing says.
///
/// The user decided it (`SPEC.md` §4.6, *the melody over the harmony*): under the jazz palette the
/// rhyme melody and the chord lane both played four-tone chords, and two lanes of sevenths were
/// thick. So whenever the harmony mapping is on — any variant but off — the melody plays single
/// notes on top of the chords, and the voicing shapes only the chord lane (`chord_tones`). With
/// the harmony off the voicing works as it always did.
///
/// It reads the **score**, not a style: whether the harmony mapping ran is a structural fact the
/// score records ([`Score::harmony`] is empty exactly when it is off). The style still decides
/// only how an event sounds, so I3 holds — no event is added, moved or removed, and every tone the
/// melody drops was one of its own event's chord tones, never another event's. It asks whether the
/// mapping is on rather than whether any bar has a chord because the second is a fact about the
/// whole poem: an edit to one stanza that gave the poem its first chord would revoice the melody of
/// every other stanza, which I2 forbids. Whether the mapping is on is the same for every stanza.
#[must_use]
pub fn melody_voicing(score: &Score, style: &Style) -> Voicing {
    if under_harmony(score) {
        Voicing::Note
    } else {
        style.voicing()
    }
}

/// Whether the harmony mapping is on in this score (any variant but off) — what moves the rhyme
/// melody to single notes ([`melody_voicing`]).
pub(crate) fn under_harmony(score: &Score) -> bool {
    !score.harmony().is_empty()
}

/// The MIDI note a role's octave starts on: palette (`SPEC.md` §5, `bass_octave` and
/// `melody_octave`), moving every note of the role by whole octaves and no event. By default the
/// bass is in the second (MIDI 36), where a bass instrument has its body, and the melody two
/// octaves above (MIDI 60), where keys and mallets sing. Before the roles were split both sat at
/// 36, so the rhyme melody was in the bass.
pub(crate) fn octave_base(style: &Style, role: Pitched) -> i32 {
    match role {
        Pitched::Melody => style.melody_octave().midi_c(),
        Pitched::Bass => style.bass_octave().midi_c(),
    }
}

/// One tone of a melody note: on the scale, or detuned off it by a slant rhyme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Tone {
    /// A MIDI note on the style's scale: a perfect rhyme.
    Exact(i32),
    /// A MIDI note and a fraction: a slant rhyme's detune (§4.5).
    Detuned(f64),
}

impl Tone {
    /// As the pattern writes it: an integer, or three decimals.
    pub(crate) fn written(self) -> String {
        match self {
            Self::Exact(note) => format!("{note}"),
            Self::Detuned(note) => fmt_param(note),
        }
    }

    /// As the Performance carries it: what Strudel reads back from [`Tone::written`].
    fn midi(self) -> Midi {
        match self {
            Self::Exact(note) => Midi(f64::from(note)),
            Self::Detuned(note) => Midi(quantised(note)),
        }
    }
}

/// One melody event's tones — one, or a chord under the seventh voicing — or `None` for an event
/// with no rhyme note: its rhyme's degree through the style's scale from `base`, detuned by how
/// far the rhyme is off.
pub(crate) fn melody_tones(
    event: &Event,
    style: &Style,
    base: i32,
    voicing: Voicing,
) -> Option<Vec<Tone>> {
    let pitch = event.pitch()?;
    Some(
        voicing
            .degrees()
            .iter()
            .map(|above| {
                // Degree through the style's scale, from the style's root. Which note that is, is
                // palette; that two lines take the *same* note is the poem's doing. A chord tone is
                // the same degree plus every other step above it, in the same scale — nothing from
                // any other event.
                let semitone = base
                    + i32::from(style.root().semitones())
                    + style.scale().degree(pitch.degree().get() + above);
                // §4.5: "perfect rhyme → exact pitch; slant rhyme → detune or neighbour tone." The
                // mapping interpolates between the two: a whole step off at a total mismatch, so a
                // half-match lands exactly on the neighbour. A chord is detuned as a whole, so a
                // slant rhyme's chord is off as one.
                if pitch.detune().is_exact() {
                    Tone::Exact(semitone)
                } else {
                    Tone::Detuned(f64::from(semitone) + pitch.detune().as_f64() * 2.0)
                }
            })
            .collect(),
    )
}

/// One bass note's MIDI number: its steps above its root through the style's scale, from the
/// style's root note in the bass octave, a semitone lower for W1's approach tone.
///
/// The root is a chord's under W2 and is folded into one octave of the scale, as the chord lane
/// folds it (`chord_tones`), so the walk stays in the bass's register while the harmony moves;
/// a step below the root sounds below it ([`crate::style::Scale::step`]). Register is palette.
pub(crate) fn bass_note(note: BassNote, style: &Style) -> i32 {
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
pub(crate) const fn chord_degrees(voicing: Voicing) -> &'static [usize] {
    match voicing {
        Voicing::Note => &[0, 2, 4],
        Voicing::Seventh => &[0, 2, 4, 6],
    }
}

/// One chord's MIDI tones: its root folded into one octave of the style's scale, then
/// [`chord_degrees`] above it, an octave below the melody's.
///
/// Folding is register, so palette: the core counts V of degree 5 as degree 9, which in a
/// seven-note scale is degree 2 an octave up; the chord lane sounds it in the same octave as every
/// other chord, so the harmony moves by its roots rather than climbing with the degree numbers.
pub(crate) fn chord_tones(chord: Chord, style: &Style) -> Vec<i32> {
    let scale = style.scale();
    let root = chord.root().get() % scale.len().max(1);
    let base = style.melody_octave().midi_c() - 12 + i32::from(style.root().semitones());
    chord_degrees(style.voicing())
        .iter()
        .map(|above| base + scale.degree(root + above))
        .collect()
}

/// The share of the chord lane's level each tone takes ([`chord_share`] for its tone count).
pub(crate) fn chord_lane_share(style: &Style) -> f64 {
    tone_share(chord_degrees(style.voicing()).len())
}

/// A pitched note's release: a struck note rings past its syllable rather than stopping dead at the
/// next one — a soundfont's own release is a hundredth of a second, which made every note a click.
pub(crate) const fn release(role: Pitched) -> f64 {
    match role {
        Pitched::Melody => 0.4,
        Pitched::Bass => 0.15,
    }
}

/// A pitched note's reverb send: the palette's reverb for the melody; the bass sits nearly dry, so
/// the low end stays clear.
pub(crate) fn pitched_room(role: Pitched, style: &Style) -> f64 {
    match role {
        Pitched::Melody => style.mix().reverb().as_f64(),
        Pitched::Bass => style.mix().reverb().as_f64() * 0.3,
    }
}

/// The melody's echo, `(level, time in seconds, feedback)`, or `None` with no delay: a dotted
/// eighth behind — three quarters of a beat at this piece's tempo, so it falls between the
/// syllables rather than on them.
pub(crate) fn echo(style: &Style, tempo: u16) -> Option<(f64, f64, f64)> {
    let delay = style.mix().delay();
    (!delay.is_off()).then(|| {
        (
            delay.as_f64(),
            0.75 * 60.0 / f64::from(tempo.max(1)),
            ECHO_FEEDBACK,
        )
    })
}

/// How much of each echo returns: printed as a constant in the pattern.
pub(crate) const ECHO_FEEDBACK: f64 = 0.35;

/// The chord lane's attack: a soft envelope, so a held chord swells in under the syllables rather
/// than striking with them.
pub(crate) const CHORD_ATTACK: f64 = 0.05;

/// The chord lane's release.
pub(crate) const CHORD_RELEASE: f64 = 0.6;

/// The piece's tempo in beats a minute, at its density.
pub(crate) fn tempo(score: &Score, style: &Style) -> u16 {
    style
        .tempo_range()
        .at_density(score.events().len(), score.bars())
        .get()
}

// ---------------------------------------------------------------------------------------------
// Time.
// ---------------------------------------------------------------------------------------------

/// A non-negative exact fraction: the arithmetic under [`Cycles`] and [`ExactSeconds`], which say
/// what it counts. It never crosses the module's public surface bare (`CLAUDE.md` §5).
///
/// Kept in lowest terms. Every operation saturates rather than overflowing: the denominators are a
/// bar's steps times a beat's parts times the printed tempo, small enough that saturation is never
/// reached by a poem, and a panic in a live-reload loop would kill the session (`CLAUDE.md` §3).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rational {
    numerator: u128,
    denominator: u128,
}

impl Rational {
    /// `numerator / denominator`, in lowest terms; a zero denominator counts as one.
    pub(crate) const fn new(numerator: u128, denominator: u128) -> Self {
        let denominator = if denominator == 0 { 1 } else { denominator };
        let divisor = gcd(numerator, denominator);
        let divisor = if divisor == 0 { 1 } else { divisor };
        Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        }
    }

    /// A whole number.
    pub(crate) const fn whole(value: u64) -> Self {
        Self::new(value as u128, 1)
    }

    /// The sum.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self::new(
            self.numerator
                .saturating_mul(other.denominator)
                .saturating_add(other.numerator.saturating_mul(self.denominator)),
            self.denominator.saturating_mul(other.denominator),
        )
    }

    /// The difference, or zero where `other` is the larger: a value never goes below zero.
    pub(crate) const fn less(self, other: Self) -> Self {
        Self::new(
            (self.numerator.saturating_mul(other.denominator))
                .saturating_sub(other.numerator.saturating_mul(self.denominator)),
            self.denominator.saturating_mul(other.denominator),
        )
    }

    /// The product.
    pub(crate) const fn mul(self, other: Self) -> Self {
        Self::new(
            self.numerator.saturating_mul(other.numerator),
            self.denominator.saturating_mul(other.denominator),
        )
    }

    /// Times a whole number.
    pub(crate) const fn times(self, factor: u64) -> Self {
        self.mul(Self::whole(factor))
    }

    /// Over a whole number (zero counts as one).
    pub(crate) const fn over(self, divisor: u64) -> Self {
        self.mul(Self::new(1, divisor as u128))
    }

    /// The fractional part.
    const fn fraction(self) -> Self {
        Self::new(self.numerator % self.denominator, self.denominator)
    }

    /// The exact fraction, `(numerator, denominator)`.
    #[must_use]
    pub(crate) const fn parts(self) -> (u128, u128) {
        (self.numerator, self.denominator)
    }

    /// The value as a float, for a consumer that must have a number. Ordering and equality use the
    /// exact fraction.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a value only: the exact fraction is what is compared
    pub(crate) fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl PartialEq for Rational {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Rational {}

impl Ord for Rational {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Both in lowest terms, so equal values have equal parts; the cross products order the rest.
        self.numerator
            .saturating_mul(other.denominator)
            .cmp(&other.numerator.saturating_mul(self.denominator))
    }
}

impl PartialOrd for Rational {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a
}

/// A place in the pattern, in cycles: exact.
///
/// What the pattern's structure is measured in — where a slot begins, where a note falls before
/// swing — and what no palette moves (I3). Only the piece's clock turns it into time, at the
/// piece's tempo; there is deliberately no way to read it as seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cycles(Rational);

impl Cycles {
    /// `numerator / denominator` cycles.
    pub(crate) const fn new(numerator: u128, denominator: u128) -> Self {
        Self(Rational::new(numerator, denominator))
    }

    /// A whole number of cycles.
    pub(crate) const fn whole(value: u64) -> Self {
        Self(Rational::whole(value))
    }

    /// The sum.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self(self.0.plus(other.0))
    }

    /// The difference, or zero cycles where `other` is the larger.
    pub(crate) const fn less(self, other: Self) -> Self {
        Self(self.0.less(other.0))
    }

    /// Times a whole number.
    pub(crate) const fn times(self, factor: u64) -> Self {
        Self(self.0.times(factor))
    }

    /// Over a whole number (zero counts as one).
    pub(crate) const fn over(self, divisor: u64) -> Self {
        Self(self.0.over(divisor))
    }

    /// The exact number of cycles, `(numerator, denominator)`, in lowest terms.
    #[must_use]
    pub const fn parts(self) -> (u128, u128) {
        self.0.parts()
    }

    /// The number of cycles as a float: a value only. Ordering and equality use the exact fraction.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.0.as_f64()
    }
}

/// A time in seconds, exact: for ordering notes and comparing times without a float
/// (`CLAUDE.md` §4). [`Seconds`] is the same time as a number to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExactSeconds(Rational);

impl ExactSeconds {
    /// `numerator / denominator` seconds.
    pub(crate) const fn new(numerator: u128, denominator: u128) -> Self {
        Self(Rational::new(numerator, denominator))
    }

    /// The sum.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self(self.0.plus(other.0))
    }

    /// The exact number of seconds, `(numerator, denominator)`, in lowest terms.
    #[must_use]
    pub const fn parts(self) -> (u128, u128) {
        self.0.parts()
    }

    /// The time as a float, to play.
    #[must_use]
    pub fn seconds(self) -> Seconds {
        Seconds(self.0.as_f64())
    }
}

/// A time in seconds.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Seconds(f64);

impl Seconds {
    /// The number of seconds.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A pitch as a MIDI note number, fractional for a slant rhyme's detune.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Midi(f64);

impl Midi {
    /// The note number.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A frequency in hertz.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Hertz(f64);

impl Hertz {
    /// The frequency.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A level in decibels: a compressor's threshold.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Decibels(f64);

impl Decibels {
    /// The number of decibels.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A linear level: a gain, a send, a waveshaper's amount.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Level(f64);

impl Level {
    /// The level.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// How swing moves a note (the text's `.layer(…)` before `.cpm`; see `strudel::swing`).
///
/// Each cycle is cut into `n` slices, and an event that begins in the second half of a slice is
/// delayed by `x` of a half-slice, `x / 2n` cycles. Every event sounds exactly once: swing is
/// palette, and moves notes without adding or removing any (I3).
///
/// That is what Strudel documents `swingBy(x, n)` as doing, but `@strudel/core` 1.2.6's `swingBy`
/// measurably does not (its haps queried, its source not read): an event in a slice's first half
/// whose delayed copy lands in the second half sounds twice, and one in the second half whose
/// delayed copy crosses into the next slice never sounds — `s("a b c d").swingBy(0.5, 1)` plays
/// a, b, b, c. So the text does not call `swingBy`. It writes this meaning with Strudel's public
/// `layer`, `mask` and `late`, which Strudel plays exactly as this places each note; the parity
/// test (`web/test/parity.test.ts`) requires that, note for note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swing {
    /// `x` in hundredths, as the palette sets it.
    hundredths: u16,
    /// `n`: the slices a cycle is cut into.
    slices: u16,
}

impl Swing {
    /// The piece's swing — the palette's amount, cut into the pulse's slices — or `None` when the
    /// palette does not swing. The Strudel text prints these same two numbers.
    pub(crate) fn of(score: &Score, style: &Style) -> Option<Self> {
        let hundredths = u16::from(style.swing().hundredths());
        (hundredths > 0).then(|| Self {
            hundredths,
            slices: strudel::swing_slices(score),
        })
    }

    /// `x`: the share of a half-slice an event in a slice's second half is delayed by.
    #[must_use]
    pub fn amount(self) -> f64 {
        f64::from(self.hundredths) / 100.0
    }

    /// `n`: how many slices a cycle is cut into.
    #[must_use]
    pub const fn slices(self) -> u16 {
        self.slices
    }

    /// Where an event that begins at `cycle` sounds: delayed by `x / 2n` cycles when it begins in
    /// a slice's second half.
    fn place(self, cycle: Cycles) -> Cycles {
        let slices = u64::from(self.slices.max(1));
        if cycle.0.times(slices).fraction() >= Rational::new(1, 2) {
            cycle.plus(self.delay())
        } else {
            cycle
        }
    }

    /// How far an event in a slice's second half is delayed: `x / 2n` cycles.
    #[must_use]
    pub fn delay(self) -> Cycles {
        Cycles::new(
            u128::from(self.hundredths),
            200 * u128::from(self.slices.max(1)),
        )
    }
}

/// The piece's clock: where each slot begins and how long it lasts, in the pattern's cycles, how
/// long a cycle lasts, and the swing. The same numbers the Strudel text prints.
#[derive(Debug, Clone)]
struct Clock {
    starts: Vec<u64>,
    weights: Vec<u64>,
    /// The `.cpm` the text prints, in thousandths: a cycle lasts `60000 / cpm` seconds on a
    /// scheduler at one cycle a second (`reading::SCHEDULER_CPS`).
    cpm: u128,
    swing: Option<Swing>,
}

impl Clock {
    fn of(score: &Score, style: &Style) -> Self {
        let cpm = u128::try_from(thousandths(strudel::cycles_per_minute(score, style)))
            .unwrap_or(1)
            .max(1);
        Self {
            starts: strudel::starts(score),
            weights: strudel::weights(score),
            cpm,
            swing: Swing::of(score, style),
        }
    }

    /// Where slot `slot` begins and how long it lasts, in cycles.
    fn slot(&self, slot: usize) -> (Cycles, Cycles) {
        (
            Cycles::whole(self.starts.get(slot).copied().unwrap_or(0)),
            Cycles::whole(self.weights.get(slot).copied().unwrap_or(1)),
        )
    }

    /// Cycles of the pattern, as seconds: the one place one becomes the other.
    const fn seconds(&self, cycles: Cycles) -> ExactSeconds {
        ExactSeconds(cycles.0.mul(Rational::new(60_000, self.cpm)))
    }

    /// Where a note at `grid` sounds, after swing, in seconds — before any nudge — always within
    /// one time round the piece.
    ///
    /// Swing can carry a note on the last half-slice past the piece's end. Strudel's pattern is
    /// cyclic, so its `late` puts that note at the start of the next time round — the first round
    /// too, as the last note of the round before it (measured: `docs/player.md` §9) — and so does
    /// this: such a note's onset is counted from the start of the piece, and it says so in
    /// [`Round::Next`]. Its place in the pattern (`grid`, its slot) does not move (I3, I4).
    fn onset(&self, grid: Cycles) -> (ExactSeconds, Round) {
        let swung = self.swing.map_or(grid, |swing| swing.place(grid));
        let end = self.cycles();
        if end > Cycles::whole(0) && swung >= end {
            (self.seconds(swung.less(end)), Round::Next)
        } else {
            (self.seconds(swung), Round::Same)
        }
    }

    /// The piece's length in cycles: the cycle it loops round on.
    fn cycles(&self) -> Cycles {
        Cycles::whole(self.starts.last().copied().unwrap_or(0))
    }
}

/// Which time round the piece a note sounds in: the round its bar is in, or — for a note swing
/// carries past the piece's end — the next, from whose start its onset is counted
/// ([`Note::round`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Round {
    /// The round its bar is in.
    Same,
    /// The next: swing carried it past the piece's end, so it sounds at the start of the next time
    /// round, as Strudel's looping pattern plays it — in the first round as well, as the last note
    /// of the round before.
    Next,
}

/// A nudge as the text prints it, in exact seconds.
fn nudged(onset: ExactSeconds, nudge: Option<f64>) -> ExactSeconds {
    nudge.map_or(onset, |nudge| {
        onset.plus(ExactSeconds::new(
            u128::try_from(thousandths(nudge)).unwrap_or(0),
            1000,
        ))
    })
}

// ---------------------------------------------------------------------------------------------
// The Performance.
// ---------------------------------------------------------------------------------------------

/// Which stem a note plays on: what mute and solo silence together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Both drum lanes.
    Drums,
    /// The bass.
    Bass,
    /// The rhyme melody.
    Melody,
    /// The chord lane.
    Chords,
}

impl Channel {
    /// Its name in the Performance's JSON.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Drums => "drums",
            Self::Bass => "bass",
            Self::Melody => "melody",
            Self::Chords => "chords",
        }
    }
}

/// How a note's voice is shaped: what the text sets on it, and nothing it does not — `None` where
/// the text leaves a parameter to the player's own default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceParams {
    attack: Option<Seconds>,
    release: Option<Seconds>,
    lpf: Hertz,
    crush: Option<u8>,
    vowel: Option<Vowel>,
    shape: Option<Level>,
}

impl VoiceParams {
    /// The attack (`.attack`): set on the drums and the chords.
    #[must_use]
    pub const fn attack(&self) -> Option<Seconds> {
        self.attack
    }

    /// The release (`.release`): set on the pitched layers and the chords.
    #[must_use]
    pub const fn release(&self) -> Option<Seconds> {
        self.release
    }

    /// The low-pass cutoff (`.lpf`).
    #[must_use]
    pub const fn lpf(&self) -> Hertz {
        self.lpf
    }

    /// The bit depth (`.crush`): set on the drums.
    #[must_use]
    pub const fn crush(&self) -> Option<u8> {
        self.crush
    }

    /// The formant vowel (`.vowel`): on a voice whose words alliterate.
    #[must_use]
    pub const fn vowel(&self) -> Option<Vowel> {
        self.vowel
    }

    /// The waveshaper (`.shape`): on the drums and the bass under drive.
    #[must_use]
    pub const fn shape(&self) -> Option<Level> {
        self.shape
    }
}

/// An echo (`.delay`, `.delaytime`, `.delayfeedback`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Echo {
    level: Level,
    time: Seconds,
    feedback: Level,
}

impl Echo {
    /// How much is sent to the echo.
    #[must_use]
    pub const fn level(&self) -> Level {
        self.level
    }

    /// How far behind it repeats.
    #[must_use]
    pub const fn time(&self) -> Seconds {
        self.time
    }

    /// How much of each repeat returns.
    #[must_use]
    pub const fn feedback(&self) -> Level {
        self.feedback
    }
}

/// What a note sends to its channel's buses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sends {
    room: Level,
    room_size: f64,
    delay: Option<Echo>,
}

impl Sends {
    /// The reverb send (`.room`).
    #[must_use]
    pub const fn room(&self) -> Level {
        self.room
    }

    /// The reverb's size (`.roomsize`), as Strudel takes it; the same on every note.
    #[must_use]
    pub const fn room_size(&self) -> f64 {
        self.room_size
    }

    /// The echo, on the melody when the palette has one.
    #[must_use]
    pub const fn delay(&self) -> Option<Echo> {
        self.delay
    }
}

/// A layer's compressor (`.compressor`, `.compressorRatio`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compressor {
    threshold: Decibels,
    ratio: f64,
}

impl Compressor {
    /// The threshold.
    #[must_use]
    pub const fn threshold(&self) -> Decibels {
        self.threshold
    }

    /// The ratio, `n` for `n:1`.
    #[must_use]
    pub const fn ratio(&self) -> f64 {
        self.ratio
    }
}

/// One note: what sounds, when, and the syllable that made it.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    span: Span,
    channel: Channel,
    lane: Lane,
    slot: usize,
    grid: Cycles,
    onset: ExactSeconds,
    round: Round,
    length: ExactSeconds,
    sound: Sound,
    pitch: Option<Midi>,
    gain: Level,
    voice: VoiceParams,
    sends: Sends,
    postgain: Level,
    compressor: Option<Compressor>,
}

impl Note {
    /// The token that made it (I4): a syllable's word, or the rhyme word a chord stands on.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The stem it plays on.
    #[must_use]
    pub const fn channel(&self) -> Channel {
        self.channel
    }

    /// The lane it is drawn in.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    /// The slot of the arrangement — the bar of the piece — it sounds in, from 0.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }

    /// Where it falls in the pattern, in cycles, before swing: the score's structure, which no
    /// palette moves (I3).
    #[must_use]
    pub const fn grid(&self) -> Cycles {
        self.grid
    }

    /// When it sounds, after swing and nudge, counted from the start of the time round it sounds
    /// in ([`Note::round`]): never past the piece's end but for its nudge.
    #[must_use]
    pub fn onset(&self) -> Seconds {
        self.onset.seconds()
    }

    /// Which time round it sounds in: [`Round::Next`] when swing carried it past the piece's end,
    /// so its onset is counted from the start of the next round. A piece played round and round
    /// needs nothing else; one played once (`prosody render`) may place it after the end instead.
    #[must_use]
    pub const fn round(&self) -> Round {
        self.round
    }

    /// The exact onset, for ordering.
    #[must_use]
    pub const fn exact_onset(&self) -> ExactSeconds {
        self.onset
    }

    /// How long it is held: its step of the pattern.
    #[must_use]
    pub fn length(&self) -> Seconds {
        self.length.seconds()
    }

    /// What makes its sound.
    #[must_use]
    pub const fn sound(&self) -> Sound {
        self.sound
    }

    /// Its pitch, for a pitched note.
    #[must_use]
    pub const fn pitch(&self) -> Option<Midi> {
        self.pitch
    }

    /// Its level: `.gain` times `.velocity`.
    #[must_use]
    pub const fn gain(&self) -> Level {
        self.gain
    }

    /// How its voice is shaped.
    #[must_use]
    pub const fn voice(&self) -> VoiceParams {
        self.voice
    }

    /// What it sends.
    #[must_use]
    pub const fn sends(&self) -> Sends {
        self.sends
    }

    /// Its level after its chain (`.postgain`): the palette's level for its role.
    #[must_use]
    pub const fn postgain(&self) -> Level {
        self.postgain
    }

    /// Its layer's compressor, when the palette compresses.
    #[must_use]
    pub const fn compressor(&self) -> Option<Compressor> {
        self.compressor
    }
}

/// Where a bar of the piece begins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarMark {
    slot: usize,
    cycle: Cycles,
    at: Seconds,
}

impl BarMark {
    /// The slot of the arrangement, from 0.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }

    /// The cycle of the pattern it begins on: where the grid of its notes ([`Note::grid`]) starts.
    #[must_use]
    pub const fn cycle(&self) -> Cycles {
        self.cycle
    }

    /// When it begins.
    #[must_use]
    pub const fn at(&self) -> Seconds {
        self.at
    }
}

/// One syllable lighting up: what the page's trace lit, on the same clock as the notes.
///
/// One for every drum note — every syllable that sounds is an event of exactly one voice, and the
/// pitched layers sound a subset of those same events — at the note's onset before its nudge, as
/// the trace pattern (`crate::trace`) placed it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    span: Span,
    lane: Lane,
    slot: usize,
    onset: ExactSeconds,
    round: Round,
    length: ExactSeconds,
    fresh: bool,
    melody: bool,
    bass: bool,
}

impl Mark {
    /// The syllable's word in the source (I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Its lane, whose colour it lights in.
    #[must_use]
    pub const fn lane(&self) -> Lane {
        self.lane
    }

    /// The slot it lights in.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }

    /// When it lights, counted from the start of the time round it lights in ([`Mark::round`]).
    #[must_use]
    pub fn onset(&self) -> Seconds {
        self.onset.seconds()
    }

    /// Which time round it lights in, as its note's [`Note::round`].
    #[must_use]
    pub const fn round(&self) -> Round {
        self.round
    }

    /// When it lights, exactly.
    #[must_use]
    pub const fn exact_onset(&self) -> ExactSeconds {
        self.onset
    }

    /// How long it stays lit.
    #[must_use]
    pub fn length(&self) -> Seconds {
        self.length.seconds()
    }

    /// Whether its line is heard anew here rather than looping (`crate::trace::Mark::fresh`).
    #[must_use]
    pub const fn fresh(&self) -> bool {
        self.fresh
    }

    /// Whether the syllable also plays a rhyme note.
    #[must_use]
    pub const fn melody(&self) -> bool {
        self.melody
    }

    /// Whether it also plays a bass note.
    #[must_use]
    pub const fn bass(&self) -> bool {
        self.bass
    }
}

/// A line of the reading at the fitted pace: spoken in its slot, fitted to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    slot: usize,
    at: Seconds,
    rate: f64,
}

impl Fitted {
    /// The slot it is spoken in.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }

    /// When that slot begins.
    #[must_use]
    pub const fn at(&self) -> Seconds {
        self.at
    }

    /// The speaking rate it is fitted at.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }
}

/// A line of the reading at the natural pace: when it may begin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paced {
    onset: reading::Onset,
    at: Option<Seconds>,
}

impl Paced {
    /// What it waits for.
    #[must_use]
    pub const fn onset(&self) -> reading::Onset {
        self.onset
    }

    /// When the slot it waits for begins; `None` for a line that only follows the one before.
    #[must_use]
    pub const fn at(&self) -> Option<Seconds> {
        self.at
    }
}

/// One line of the reading, at both paces (`SPEC.md` §4.7).
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    line: usize,
    stanza: usize,
    span: Span,
    pace: f64,
    pause: Seconds,
    fitted: Option<Fitted>,
    natural: Option<Paced>,
}

impl Cue {
    /// Which line of the poem, in reading order from 0.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Which stanza it is in, from 0.
    #[must_use]
    pub const fn stanza(&self) -> usize {
        self.stanza
    }

    /// The line's span in the source (I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The speed the natural pace asks a voice for.
    #[must_use]
    pub const fn pace(&self) -> f64 {
        self.pace
    }

    /// The pause after it at the natural pace.
    #[must_use]
    pub const fn pause(&self) -> Seconds {
        self.pause
    }

    /// Where it is spoken at the fitted pace, if it is.
    #[must_use]
    pub const fn fitted(&self) -> Option<Fitted> {
        self.fitted
    }

    /// When it may begin at the natural pace, if it is read there.
    #[must_use]
    pub const fn natural(&self) -> Option<Paced> {
        self.natural
    }
}

/// A whole piece, resolved: every note, every bar, every syllable's light, and every line of the
/// reading — a cue for each line it speaks, and the reason for each line it leaves unread.
#[derive(Debug, Clone, PartialEq)]
pub struct Performance {
    cycles: Cycles,
    seconds: ExactSeconds,
    cpm: f64,
    swing: Option<Swing>,
    bars: Vec<BarMark>,
    notes: Vec<Note>,
    cues: Vec<Cue>,
    unread: Vec<reading::Unread>,
    natural_unread: Vec<reading::Unread>,
    marks: Vec<Mark>,
}

impl Performance {
    /// How long the piece lasts, tails excluded: where it loops round.
    #[must_use]
    pub fn seconds(&self) -> Seconds {
        self.seconds.seconds()
    }

    /// How long the piece lasts in cycles of the pattern: where its last bar ends.
    #[must_use]
    pub const fn cycles(&self) -> Cycles {
        self.cycles
    }

    /// The `.cpm` the text prints: the pattern's cycles a minute.
    #[must_use]
    pub const fn cycles_per_minute(&self) -> f64 {
        self.cpm
    }

    /// The swing, when the palette swings.
    #[must_use]
    pub const fn swing(&self) -> Option<Swing> {
        self.swing
    }

    /// Where each bar begins, in slot order.
    #[must_use]
    pub fn bars(&self) -> &[BarMark] {
        &self.bars
    }

    /// Every note, sorted by onset, then by where its token is in the source, then by lane.
    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Every line the reading speaks at either pace, in reading order: empty until
    /// [`Performance::read`]. A line it speaks at neither is not here but in
    /// [`Performance::unread`] and [`Performance::natural_unread`], with why.
    #[must_use]
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// The lines the fitted pace leaves unread although the reading is on, and why
    /// ([`Reading::unread`]) — so a player that reads from the Performance can still say what it
    /// loses (`CLAUDE.md` §6). Empty until [`Performance::read`].
    #[must_use]
    pub fn unread(&self) -> &[reading::Unread] {
        &self.unread
    }

    /// The lines the natural pace has no onset for, and why ([`Reading::natural_unread`]). Empty
    /// until [`Performance::read`].
    #[must_use]
    pub fn natural_unread(&self) -> &[reading::Unread] {
        &self.natural_unread
    }

    /// Every syllable's light, sorted as the notes are.
    #[must_use]
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// The same piece, with its reading's cues on the piece's clock, and the lines it leaves
    /// unread with their reasons.
    ///
    /// Separate from [`Backend::render`] because a reading needs the poem's lines and the chosen
    /// mappings, which a backend is not given; `reading` must be planned from the same score and
    /// style (`reading::plan`).
    #[must_use]
    pub fn read(mut self, reading: &Reading) -> Self {
        let per_cycle = Rational::new(60_000, self.cpm_thousandths());
        let at_slot = |slot: usize| {
            let start = reading.bars().get(slot).copied().unwrap_or(0);
            ExactSeconds(Rational::whole(start).mul(per_cycle)).seconds()
        };
        let blank = |spoken: &reading::Spoken| Cue {
            line: spoken.line(),
            stanza: spoken.stanza(),
            span: spoken.span(),
            pace: spoken.pace().speed(),
            pause: Seconds(spoken.pause().seconds()),
            fitted: None,
            natural: None,
        };
        let mut cues: BTreeMap<usize, Cue> = BTreeMap::new();
        for cue in reading.cues() {
            let line = cues
                .entry(cue.line())
                .or_insert_with(|| blank(cue.spoken()));
            line.fitted = Some(Fitted {
                slot: cue.slot(),
                at: at_slot(cue.slot()),
                rate: cue.rate(),
            });
        }
        for natural in reading.natural() {
            let line = cues
                .entry(natural.spoken().line())
                .or_insert_with(|| blank(natural.spoken()));
            line.natural = Some(Paced {
                onset: natural.onset(),
                at: natural.onset().slot().map(at_slot),
            });
        }
        self.cues = cues.into_values().collect();
        self.unread = reading.unread().to_vec();
        self.natural_unread = reading.natural_unread().to_vec();
        self
    }

    /// The tempo in thousandths, as the clock had it.
    fn cpm_thousandths(&self) -> u128 {
        u128::try_from(thousandths(self.cpm)).unwrap_or(1).max(1)
    }
}

/// Resolves a score into a [`Performance`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Perform;

impl Perform {
    /// A new backend.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Why a score could not be performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PerformError {
    /// The score has no events, so there is nothing to play — as for [`crate::StrudelError`].
    #[error("nothing to play: the score has no events")]
    Silent,
}

impl Backend for Perform {
    type Output = Performance;
    type Error = PerformError;

    fn name(&self) -> &'static str {
        "performance"
    }

    fn render(&self, score: &Score, style: &Style) -> Result<Performance, PerformError> {
        if score.events().is_empty() {
            return Err(PerformError::Silent);
        }
        let clock = Clock::of(score, style);
        let context = Context {
            score,
            style,
            clock: &clock,
            strays: strays(score),
            density: density(score, style),
        };
        let mut notes = Vec::new();
        let mut marks = Vec::new();
        for voice in score.voices() {
            context.drums(*voice, &mut notes, &mut marks);
        }
        for role in [Pitched::Melody, Pitched::Bass] {
            context.pitched(role, &mut notes);
        }
        context.chords(&mut notes);
        // One total order before anything is played (`CLAUDE.md` §4): exact onsets, ties by span,
        // then lane. A stable sort, so the tones of one chord keep the order they were voiced in.
        notes.sort_by(|a, b| {
            a.onset
                .cmp(&b.onset)
                .then(a.span.start().cmp(&b.span.start()))
                .then(a.lane.cmp(&b.lane))
        });
        marks.sort_by(|a, b| {
            a.onset
                .cmp(&b.onset)
                .then(a.span.start().cmp(&b.span.start()))
                .then(a.lane.cmp(&b.lane))
        });
        let bars = (0..score.arrangement().slots().len())
            .map(|slot| {
                let cycle = clock.slot(slot).0;
                BarMark {
                    slot,
                    cycle,
                    at: clock.seconds(cycle).seconds(),
                }
            })
            .collect();
        Ok(Performance {
            cycles: clock.cycles(),
            seconds: clock.seconds(clock.cycles()),
            cpm: quantised(strudel::cycles_per_minute(score, style)),
            swing: clock.swing,
            bars,
            notes,
            cues: Vec::new(),
            unread: Vec::new(),
            natural_unread: Vec::new(),
            marks,
        })
    }
}

/// Everything a layer's walk reads.
struct Context<'a> {
    score: &'a Score,
    style: &'a Style,
    clock: &'a Clock,
    strays: BTreeMap<EventKey, Stray>,
    density: Vec<f64>,
}

/// One timed event of a lane: the slot it sounds in, where it falls and how long it lasts.
struct Timed<'a> {
    event: &'a Event,
    slot: usize,
    grid: Cycles,
    length: Cycles,
}

impl Context<'_> {
    /// Every event of a lane, timed, slot by slot.
    fn timed<'a>(&self, walk: strudel::Lane<'a>) -> Vec<Timed<'a>> {
        let mut out = Vec::new();
        for (slot, line) in strudel::lines(walk).iter().enumerate() {
            let Some(line) = line else { continue };
            let (begin, length) = self.clock.slot(slot);
            out.extend(
                line.timed(begin, length)
                    .into_iter()
                    .map(|(event, grid, length)| Timed {
                        event,
                        slot,
                        grid,
                        length,
                    }),
            );
        }
        out
    }

    /// The bus a layer's notes share: its compressor and level.
    fn bus(&self, level: Amount, share: f64) -> (Level, Option<Compressor>) {
        (
            Level(quantised(postgain(level, share))),
            compressor(self.style).map(|(threshold, ratio)| Compressor {
                threshold: Decibels(quantised(threshold)),
                ratio: quantised(ratio),
            }),
        )
    }

    /// A note's level: its gain times its velocity, each as the text prints it.
    fn level(&self, gain: f64, event: &Event) -> Level {
        let velocity = velocity(&self.strays, event, self.style).map_or(1.0, quantised);
        Level(quantised(gain) * velocity)
    }

    /// A note's onset, after swing and its nudge, and the round it sounds in.
    fn onset(&self, grid: Cycles, event: &Event) -> (ExactSeconds, Round) {
        let (onset, round) = self.clock.onset(grid);
        (nudged(onset, nudge(&self.strays, event, self.style)), round)
    }

    /// One voice's drums: every syllable it sounds, and what §4.3 and §4.4 do to each (the
    /// Strudel emitter's `rhythm_layer`), with a light for each.
    fn drums(
        &self,
        voice: prosody_core::voice::Voice,
        notes: &mut Vec<Note>,
        marks: &mut Vec<Mark>,
    ) {
        let (score, style) = (self.score, self.style);
        let events: Vec<&Event> = score.events_of(voice.id()).collect();
        let vowels = takes_vowels(&events);
        let lane = Lane::of(voice.layer());
        let (postgain, compressor) = self.bus(style.mix().drums_gain(), 1.0);
        for walk in strudel::Lane::all(score, &events, false) {
            for timed in self.timed(walk) {
                let event = timed.event;
                let level = self.density.get(timed.slot).copied().unwrap_or(1.0);
                let (onset, round) = self.onset(timed.grid, event);
                notes.push(Note {
                    span: event.span(),
                    channel: Channel::Drums,
                    lane,
                    slot: timed.slot,
                    grid: timed.grid,
                    onset,
                    round,
                    length: self.clock.seconds(timed.length),
                    sound: Sound::Drum {
                        bank: strudel_bank(style.bank()),
                        piece: sample_for_drum(style, event.drum()),
                    },
                    pitch: None,
                    gain: self.level(gain(score, event, event.velocity(), level), event),
                    voice: VoiceParams {
                        attack: Some(Seconds(quantised(attack(score, event)))),
                        release: None,
                        lpf: Hertz(quantised(cutoff(score, event, style))),
                        crush: Some(crush(event)),
                        vowel: vowels.then(|| vowel(event)),
                        shape: drive(style).map(|shape| Level(quantised(shape))),
                    },
                    sends: Sends {
                        room: Level(quantised(drum_room(score, event, style))),
                        room_size: quantised(room_size(style)),
                        delay: None,
                    },
                    postgain,
                    compressor,
                });
                let (onset, round) = self.clock.onset(timed.grid);
                marks.push(Mark {
                    span: event.span(),
                    lane,
                    slot: timed.slot,
                    onset,
                    round,
                    length: self.clock.seconds(timed.length),
                    fresh: crate::trace::fresh(score, timed.slot, event),
                    melody: event.pitch().is_some(),
                    bass: event.bass().is_some(),
                });
            }
        }
    }

    /// A pitched layer: the melody or the bass (the Strudel emitter's `pitched_layer`).
    fn pitched(&self, role: Pitched, notes: &mut Vec<Note>) {
        let (score, style) = (self.score, self.style);
        let pitched: Vec<&Event> = score
            .events()
            .iter()
            .filter(|event| role.plays(event))
            .collect();
        let base = octave_base(style, role);
        let voicing = match role {
            Pitched::Melody => melody_voicing(score, style),
            Pitched::Bass => Voicing::Note,
        };
        let (channel, lane, named, level) = match role {
            Pitched::Melody => (
                Channel::Melody,
                Lane::Melody,
                style.melody(),
                style.mix().melody_gain(),
            ),
            Pitched::Bass => (
                Channel::Bass,
                Lane::Bass,
                style.bass(),
                style.mix().bass_gain(),
            ),
        };
        let sound = instrument(named, role);
        let (postgain, compressor) = self.bus(level, chord_share(voicing));
        let delay = match role {
            Pitched::Melody => {
                echo(style, tempo(score, style)).map(|(level, time, feedback)| Echo {
                    level: Level(quantised(level)),
                    time: Seconds(quantised(time)),
                    feedback: Level(quantised(feedback)),
                })
            }
            Pitched::Bass => None,
        };
        // The bass is driven; the melody is not.
        let shape = drive(style)
            .filter(|_| role == Pitched::Bass)
            .map(|shape| Level(quantised(shape)));
        for walk in strudel::Lane::all(score, &pitched, role == Pitched::Bass) {
            // W2's note is its foot: it holds the beat (`prosody_core::bass`).
            let walk = if role == Pitched::Bass && score.bass() == Variant::Feet {
                walk.whole_beats()
            } else {
                walk
            };
            for timed in self.timed(walk) {
                let event = timed.event;
                let (tones, velocity) = match role {
                    Pitched::Melody => (
                        melody_tones(event, style, base, voicing)
                            .unwrap_or_default()
                            .into_iter()
                            .map(Tone::midi)
                            .collect::<Vec<_>>(),
                        event.velocity(),
                    ),
                    Pitched::Bass => match event.bass() {
                        Some(note) => (
                            vec![Midi(f64::from(bass_note(note, style)))],
                            note.velocity(),
                        ),
                        None => (Vec::new(), event.velocity()),
                    },
                };
                let level = self.density.get(timed.slot).copied().unwrap_or(1.0);
                let gain = self.level(gain(score, event, velocity, level), event);
                let (onset, round) = self.onset(timed.grid, event);
                for pitch in tones {
                    notes.push(Note {
                        span: event.span(),
                        channel,
                        lane,
                        slot: timed.slot,
                        grid: timed.grid,
                        onset,
                        round,
                        length: self.clock.seconds(timed.length),
                        sound,
                        pitch: Some(pitch),
                        gain,
                        voice: VoiceParams {
                            attack: None,
                            release: Some(Seconds(quantised(release(role)))),
                            lpf: Hertz(quantised(cutoff(score, event, style))),
                            crush: None,
                            vowel: None,
                            shape,
                        },
                        sends: Sends {
                            room: Level(quantised(pitched_room(role, style))),
                            room_size: quantised(room_size(style)),
                            delay,
                        },
                        postgain,
                        compressor,
                    });
                }
            }
        }
    }

    /// The chord lane: one chord a slot where the harmony gives one, a stanza's last bar under the
    /// cadence split in halves (the Strudel emitter's `chord_layer`).
    fn chords(&self, notes: &mut Vec<Note>) {
        let (score, style) = (self.score, self.style);
        let sound = chord_instrument(style);
        let (postgain, compressor) = self.bus(style.mix().chord_gain(), chord_lane_share(style));
        for (slot, harmony) in score
            .arrangement()
            .slots()
            .iter()
            .map(|slot| score.harmony_in(slot))
            .enumerate()
        {
            let (begin, length) = self.clock.slot(slot);
            let half = length.over(2);
            let held: Vec<(Chord, Cycles, Cycles)> = match harmony {
                None => Vec::new(),
                Some(Harmony::Turning { own, turn }) => own
                    .map(|own| (own, begin, half))
                    .into_iter()
                    .chain(std::iter::once((turn, begin.plus(half), half)))
                    .collect(),
                Some(harmony) => harmony
                    .chord()
                    .map(|chord| (chord, begin, length))
                    .into_iter()
                    .collect(),
            };
            let level = Level(quantised(self.density.get(slot).copied().unwrap_or(1.0)));
            for (chord, grid, length) in held {
                let (onset, round) = self.clock.onset(grid);
                for tone in chord_tones(chord, style) {
                    notes.push(Note {
                        span: chord.span(),
                        channel: Channel::Chords,
                        lane: Lane::Chords,
                        slot,
                        grid,
                        onset,
                        round,
                        length: self.clock.seconds(length),
                        sound,
                        pitch: Some(Midi(f64::from(tone))),
                        gain: level,
                        voice: VoiceParams {
                            attack: Some(Seconds(CHORD_ATTACK)),
                            release: Some(Seconds(CHORD_RELEASE)),
                            lpf: Hertz(f64::from(style.mix().high_cut())),
                            crush: None,
                            vowel: None,
                            shape: None,
                        },
                        sends: Sends {
                            room: Level(quantised(style.mix().reverb().as_f64())),
                            room_size: quantised(room_size(style)),
                            delay: None,
                        },
                        postgain,
                        compressor,
                    });
                }
            }
        }
    }
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

    use super::{Channel, Cycles, Perform, Rational, Round, Swing, quantised, thousandths};
    use crate::{Backend as _, Strudel, Style};

    /// Spelling is pronunciation: `A` and `o` are stressed vowels, `e` unstressed.
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

    struct Flat;

    impl Lexicon for Flat {
        fn is_function_word(&self, _: &str) -> bool {
            false
        }

        fn rarity(&self, _: &str) -> Rarity {
            Rarity::band(3)
        }
    }

    fn score_of(source: &str) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &Flat);
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

    fn style(header: &str) -> Style {
        Style::parse(&format!("+++\ntempo_range = [120, 120]\n{header}+++")).unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn fractions_stay_exact_and_in_lowest_terms() {
        let sum = Rational::new(1, 2).plus(Rational::new(1, 3));
        assert_eq!(sum.parts(), (5, 6));
        assert_eq!(Rational::new(6, 8).parts(), (3, 4));
        assert_eq!(Rational::new(2, 4), Rational::new(1, 2));
        assert!(Rational::new(1, 3) < Rational::new(1, 2));
        assert_eq!(Rational::whole(3).over(4).times(2).parts(), (3, 2));
        assert_eq!(Rational::new(7, 2).fraction().parts(), (1, 2));
        // A zero denominator is one, not a division by zero.
        assert_eq!(Rational::new(3, 0).parts(), (3, 1));
    }

    #[test]
    fn a_value_is_carried_at_the_three_decimals_the_text_prints() {
        assert!(close(quantised(1.0 / 3.0), 0.333));
        assert!(close(quantised(0.4999), 0.5));
        assert!(close(quantised(-24.0), -24.0));
        assert_eq!(thousandths(0.35), 350);
        assert_eq!(thousandths(40.333_333), 40_333);
    }

    #[test]
    fn swing_delays_only_what_begins_in_a_slices_second_half() {
        // Strudel's documented `swingBy(0.5, 1)` on `a b c d`: a cycle is one slice; `c` and `d`
        // begin in its second half and move by half of a half-slice, a quarter cycle. (Strudel's
        // own `swingBy` plays `b` twice and `d` never — see `Swing` — so the text does not use it.)
        let swing = Swing {
            hundredths: 50,
            slices: 1,
        };
        let placed: Vec<(u128, u128)> = [(0, 1), (1, 4), (1, 2), (3, 4)]
            .iter()
            .map(|(n, d)| swing.place(Cycles::new(*n, *d)).parts())
            .collect();
        assert_eq!(placed, vec![(0, 1), (1, 4), (3, 4), (1, 1)]);
        // Two slices a cycle: the second half of the first slice is [1/4, 1/2), and the delay is
        // a third of a half-slice, 1/12 of a cycle.
        let swing = Swing {
            hundredths: 33,
            slices: 2,
        };
        assert_eq!(swing.place(Cycles::new(1, 8)).parts(), (1, 8));
        assert_eq!(
            swing.place(Cycles::new(1, 4)).parts(),
            Rational::new(1, 4).plus(Rational::new(33, 400)).parts()
        );
        assert_eq!(swing.delay().parts(), (33, 400));
        assert_eq!(swing.place(Cycles::new(1, 2)).parts(), (1, 2));
    }

    #[test]
    fn a_line_lands_where_its_pattern_says() {
        // "teA bAn" is two feet: an unstressed and a stressed syllable, then one stressed. The
        // pattern says so — a two-beat bar, `[[bd bd] bd]@2`, played twice (the outro repeats it)
        // at `.cpm(120)` — so a cycle, a beat, lasts half a second, the first beat's syllables
        // fall at 0 and a quarter second, and the second beat's at half a second, held for it.
        // Swing 0.5 over one slice a beat moves what begins in a beat's second half by a quarter
        // of a beat: the second syllable to 0.375 s.
        let score = score_of("teA bAn\n");
        let swung = style("swing = 0.5\nhumanise = 0\n");
        let text = Strudel::new().render(&score, &swung).unwrap();
        assert!(
            text.contains("s(\"<[[bd bd] bd]@2 [[bd bd] bd]@2>\")"),
            "{text}"
        );
        assert!(
            text.contains(".late(0.500 / 2)) // swing: the second half of each beat, late by 0.500 of a half-beat\n  .cpm(120.000)"),
            "{text}"
        );
        let performance = Perform::new().render(&score, &swung).unwrap();
        let drums: Vec<(f64, f64)> = performance
            .notes()
            .iter()
            .filter(|note| note.channel() == Channel::Drums)
            .map(|note| (note.onset().get(), note.length().get()))
            .collect();
        let expected = [
            (0.0, 0.25),
            (0.375, 0.25),
            (0.5, 0.5),
            (1.0, 0.25),
            (1.375, 0.25),
            (1.5, 0.5),
        ];
        assert_eq!(drums.len(), expected.len(), "{drums:?}");
        for ((onset, length), (want_onset, want_length)) in drums.iter().zip(expected) {
            assert!(close(*onset, want_onset), "{drums:?}");
            assert!(close(*length, want_length), "{drums:?}");
        }
        assert!(close(performance.seconds().get(), 2.0));
        let starts: Vec<f64> = performance
            .bars()
            .iter()
            .map(|bar| bar.at().get())
            .collect();
        assert!(close(starts[0], 0.0) && close(starts[1], 1.0), "{starts:?}");
        // The bass sounds the stressed vowels — the second syllable and the third — on the bass
        // octave's C (MIDI 36, the default minor scale on C, degree 0).
        let bass: Vec<(f64, f64)> = performance
            .notes()
            .iter()
            .filter(|note| note.channel() == Channel::Bass)
            .map(|note| (note.onset().get(), note.pitch().unwrap().get()))
            .collect();
        assert_eq!(bass.len(), 4, "{bass:?}");
        assert!(
            close(bass[0].0, 0.375) && close(bass[0].1, 36.0),
            "{bass:?}"
        );
        assert!(close(bass[1].0, 0.5) && close(bass[1].1, 36.0), "{bass:?}");
        // One light for every drum note, before its nudge.
        assert_eq!(performance.marks().len(), drums.len());
    }

    #[test]
    fn a_nudge_moves_the_onset_by_what_the_text_prints() {
        let score = score_of("teA bAn\n");
        let human = style("humanise = 0.5\n");
        let text = Strudel::new().render(&score, &human).unwrap();
        let performance = Perform::new().render(&score, &human).unwrap();
        // The first syllable's nudge and velocity, read off the text by hand: the first value of
        // `.nudge("…")` and of `.velocity("…")`, and its gain the first of `.gain("…")`.
        let first = |call: &str| -> f64 {
            let at = text.find(&format!(".{call}(\"<[[")).unwrap() + call.len() + 6;
            text[at..]
                .split([' ', ']'])
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };
        let note = &performance.notes()[0];
        assert!(close(note.onset().get(), first("nudge")));
        assert!(close(note.gain().get(), first("gain") * first("velocity")));
        // Its light is on the beat: the trace has no nudge.
        assert!(close(performance.marks()[0].onset().get(), 0.0));
    }

    #[test]
    fn a_reading_is_cued_on_the_bars_it_names() {
        let source = "teA bAn\n";
        let poem = Poem::parse(source.to_owned()).unwrap();
        let score = score_of(source);
        let at_120 = style("");
        let reading =
            crate::reading::plan(&poem, &score, &at_120, &prosody_core::Mappings::all()).unwrap();
        let performance = Perform::new()
            .render(&score, &at_120)
            .unwrap()
            .read(&reading);
        assert_eq!(performance.cues().len(), 1);
        let cue = &performance.cues()[0];
        assert_eq!(cue.line(), 0);
        assert_eq!(cue.span().text(source), Some("teA bAn"));
        // The one line is read in the first bar, which begins the piece.
        let fitted = cue.fitted().unwrap();
        assert_eq!(fitted.slot(), 0);
        assert!(close(fitted.at().get(), 0.0));
        assert!(cue.natural().is_some());
    }

    #[test]
    fn a_later_line_is_cued_on_its_own_bars_clock() {
        // Two stanzas of two-beat lines at 100 beats a minute. Under the steady pulse a cycle is a
        // beat, so a cycle lasts 60 / 100 = 0.6 s, and slot `n` begins on cycle 2n: at 1.2·n s.
        // Which slot each line is read in is the reading's to say (`reading::plan`, tested
        // there); this pins how its slots become seconds. 100 rather than 120 so that a slot
        // read as seconds, or a cycle read as half a second, cannot pass.
        let source = "teA bAn\nreA dAn\n\nbo dos rot\nteA rot\n";
        let poem = Poem::parse(source.to_owned()).unwrap();
        let score = score_of(source);
        let at_100 = Style::parse("+++\ntempo_range = [100, 100]\n+++").unwrap();
        let reading =
            crate::reading::plan(&poem, &score, &at_100, &prosody_core::Mappings::all()).unwrap();
        let performance = Perform::new()
            .render(&score, &at_100)
            .unwrap()
            .read(&reading);
        let fitted: Vec<(usize, f64)> = performance
            .cues()
            .iter()
            .map(|cue| {
                let fitted = cue.fitted().unwrap();
                (fitted.slot(), fitted.at().get())
            })
            .collect();
        // Each stanza's lines a bar apart; the second stanza from the bar after its arrival.
        let want = [(0, 0.0), (1, 1.2), (3, 3.6), (4, 4.8)];
        assert_eq!(fitted.len(), want.len(), "{fitted:?}");
        for ((slot, at), (want_slot, want_at)) in fitted.iter().zip(want) {
            assert_eq!(*slot, want_slot, "{fitted:?}");
            assert!(close(*at, want_at), "{fitted:?}");
            // The reading's clock and the piece's are one clock.
            assert!(
                close(*at, performance.bars()[*slot].at().get()),
                "{fitted:?}"
            );
        }
        // The second stanza's first line, at the natural pace, waits for its breath in slot 2,
        // which begins on cycle 4: 2.4 s.
        let natural = performance.cues()[2].natural().unwrap();
        assert_eq!(natural.onset().slot(), Some(2));
        assert!(close(natural.at().unwrap().get(), 2.4));
        assert!(close(performance.bars()[2].at().get(), 2.4));
        // A line that only follows the one before has no slot of its own to wait for.
        assert_eq!(performance.cues()[3].natural().unwrap().at(), None);
    }

    #[test]
    fn a_mark_lights_what_the_trace_lights_on_the_beat() {
        // "teA bAn / teA bAn bAn / reA dAn", humanised. One stanza, so three stated bars and then
        // the outro's three, which loop. "bAn", "bAn" and "dAn" end their lines and rhyme on
        // AA N: the melody's. The bass takes every other stressed vowel (the height bass leaves
        // rhyme words to the melody): the A of teA and reA, and the middle bAn.
        let source = "teA bAn\nteA bAn bAn\nreA dAn\n";
        let score = score_of(source);
        let human = style("humanise = 1\n");
        let performance = Perform::new().render(&score, &human).unwrap();
        let marks: Vec<(&str, usize, f64, bool, bool, bool)> = performance
            .marks()
            .iter()
            .map(|mark| {
                (
                    mark.span().text(source).unwrap(),
                    mark.slot(),
                    mark.onset().get(),
                    mark.fresh(),
                    mark.melody(),
                    mark.bass(),
                )
            })
            .collect();
        // Bars of two beats at 120 beats a minute, a beat half a second: the first bar's two
        // beats split [te A] and [bAn], so its syllables light at 0, 0.25 and 0.5 s — on the
        // beat, though every one of their notes is nudged late.
        let want_first_bar = [
            ("teA", 0, 0.0, true, false, false),
            ("teA", 0, 0.25, true, false, true),
            ("bAn", 0, 0.5, true, true, false),
        ];
        // The second line's second beat splits [bAn bAn]: the first carries the bass, the second
        // — the line's rhyme word — the melody.
        let want_second_bar = [
            ("teA", 1, 1.0, true, false, false),
            ("teA", 1, 1.25, true, false, true),
            ("bAn", 1, 1.5, true, false, true),
            ("bAn", 1, 1.75, true, true, false),
        ];
        for (got, want) in marks
            .iter()
            .zip(want_first_bar.iter().chain(&want_second_bar))
        {
            assert_eq!(got.0, want.0, "{marks:?}");
            assert_eq!(got.1, want.1, "{marks:?}");
            assert!(close(got.2, want.2), "{marks:?}");
            assert_eq!((got.3, got.4, got.5), (want.3, want.4, want.5), "{marks:?}");
        }
        // The second syllable's note is nudged; its light is not.
        let note = performance
            .notes()
            .iter()
            .find(|note| {
                note.slot() == 0
                    && note.channel() == Channel::Drums
                    && note.grid() == Cycles::new(1, 2)
            })
            .unwrap();
        assert!(note.onset().get() > 0.25, "{:?}", note.onset());
        // The outro loops: every mark of its three bars is not fresh, the stated three are.
        assert!(marks.iter().all(|mark| mark.3 == (mark.1 < 3)), "{marks:?}");
        assert_eq!(marks.len(), 20, "{marks:?}");
        // And the Performance lights exactly what the trace pattern lights: the same syllables,
        // in the same colours, fresh, melody and bass alike.
        let trace = crate::trace::plan(&score, &human);
        let traced: std::collections::BTreeSet<_> = trace
            .marks()
            .iter()
            .map(|mark| {
                (
                    mark.span(),
                    mark.colour(),
                    mark.fresh(),
                    mark.melody(),
                    mark.bass(),
                )
            })
            .collect();
        let lit: std::collections::BTreeSet<_> = performance
            .marks()
            .iter()
            .map(|mark| {
                (
                    mark.span(),
                    mark.lane().hex(),
                    mark.fresh(),
                    mark.melody(),
                    mark.bass(),
                )
            })
            .collect();
        assert_eq!(lit, traced);
    }

    #[test]
    fn a_note_swung_past_the_end_sounds_at_the_start_of_the_next_round() {
        // "teA bAn tete" is two beats, `[[bd bd] [bd bd bd]]@2`, played twice (the outro repeats
        // it): four cycles at `.cpm(120)`, half a second each, so the piece loops round at 2 s.
        // Swing 0.9 over one slice a beat delays what begins in a beat's second half by 0.45 of a
        // cycle. The last syllable of each bar begins at 2/3 of its second beat: in the first bar
        // at cycle 5/3, which swing carries to 5/3 + 9/20 = 127/60 — into the second bar, where it
        // stays (127/120 s). In the second bar at cycle 11/3, carried to 247/60: past the end at
        // cycle 4. Strudel's pattern is cyclic, so it plays that note 7/60 of a cycle into the
        // next time round — the first round included, as the last note of the round before
        // (`docs/player.md` §9) — and so does the Performance: at 7/120 s, in `Round::Next`, still
        // in its own bar and at its own place in the pattern.
        let score = score_of("teA bAn tete\n");
        let swung = style("swing = 0.9\nhumanise = 0\n");
        let text = Strudel::new().render(&score, &swung).unwrap();
        assert!(
            text.contains("s(\"<[[bd bd] [bd bd bd]]@2 [[bd bd] [bd bd bd]]@2>\")"),
            "{text}"
        );
        assert!(text.contains(".late(0.900 / 2))"), "{text}");
        let performance = Perform::new().render(&score, &swung).unwrap();
        assert!(close(performance.seconds().get(), 2.0));
        let drums: Vec<_> = performance
            .notes()
            .iter()
            .filter(|note| note.channel() == Channel::Drums)
            .collect();
        let at = |grid: (u128, u128)| {
            drums
                .iter()
                .find(|note| note.grid().parts() == grid)
                .unwrap()
        };
        let stays = at((5, 3));
        assert_eq!(stays.exact_onset().parts(), (127, 120));
        assert_eq!((stays.slot(), stays.round()), (0, Round::Same));
        let wraps = at((11, 3));
        assert_eq!(wraps.exact_onset().parts(), (7, 120));
        assert_eq!((wraps.slot(), wraps.round()), (1, Round::Next));
        // Sorted by onset, it is the piece's second note, after the first syllable at 0.
        assert_eq!(performance.notes()[0].exact_onset().parts(), (0, 1));
        assert_eq!(performance.notes()[1].grid().parts(), (11, 3));
        // Every other note stays in its round; no onset reaches the end.
        assert_eq!(
            performance
                .notes()
                .iter()
                .filter(|note| note.round() == Round::Next)
                .count(),
            1
        );
        assert!(
            performance
                .notes()
                .iter()
                .all(|note| note.onset().get() < performance.seconds().get())
        );
        // Its light comes round with it.
        let mark = performance
            .marks()
            .iter()
            .find(|mark| mark.round() == Round::Next)
            .unwrap();
        assert_eq!(mark.exact_onset().parts(), (7, 120));
        assert_eq!(mark.slot(), 1);
    }
}
