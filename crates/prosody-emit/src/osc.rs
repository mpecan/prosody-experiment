//! The OSC backend: `SPEC.md` §7.8.
//!
//! > Only then: direct synthesis, OSC, live reload.
//!
//! Where the Strudel backend emits *text describing a pattern*, this emits **a schedule**: every
//! event, at an exact time, as an Open Sound Control message a synthesis engine can play.
//! `SPEC.md` §10 names `SuperCollider`; nothing here is specific to it beyond the message names.
//!
//! # Why this backend and not just the text one
//!
//! [`Backend::Output`] is an associated type precisely so this could
//! exist. Two things the text backend structurally cannot do, and both are `SPEC.md` mappings that
//! have been carried through the IR waiting for it:
//!
//! - **`SPEC.md` §4.3's anaphora row — "downbeat-anchored ostinato".** In mini-notation an event's
//!   position in the cycle is the only thing that decides when it sounds, so an anchor has nowhere
//!   to move it to. Here time is a number, so a line-initial repetition can be pulled *onto* its
//!   beat, which is what "anchored" means. `PLAN.md` recorded this as the question M9 would
//!   answer, and this is the answer.
//! - **§4.3's epistrophe row — "cadence figure / turnaround".** A cadence is a gesture of ending,
//!   and the cheapest honest rendering of that is duration: the line-final repetition rings on
//!   where the events around it are clipped.
//!
//! # Time
//!
//! Onsets are exact rationals in **beats from the start of the piece**, never seconds. A tempo is
//! a palette value ([`Style::tempo_range`](crate::Style::tempo_range)), so turning beats into
//! seconds is the last thing that happens and it happens in one place — which also keeps
//! invariant I3 true here: change the tempo and every onset in beats is identical.

use std::fmt::Write as _;

use prosody_core::role::Layer;
use prosody_core::voice::{Anchor, Event, Score};

use crate::style::Style;
use crate::{Backend, fmt_param};

/// Renders a score as a schedule of OSC messages.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Osc;

impl Osc {
    /// A new backend.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Why a score could not be scheduled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OscError {
    /// The score has no events, so there is nothing to schedule.
    #[error("nothing to schedule: the score has no events")]
    Silent,
}

/// A time in beats from the start of the piece, exact.
///
/// A rational rather than a float because onsets are compared and sorted, and `CLAUDE.md` §4 keeps
/// floats out of both. Kept in lowest terms; the denominator is at most the subdivision of the beat
/// times the line's beat count (when a looped line is fitted under a host of another length), so it
/// stays small and exact — a triplet is a third, not 0.333.
#[derive(Debug, Clone, Copy)]
pub struct Onset {
    numerator: u64,
    denominator: u64,
}

impl Onset {
    /// The time in beats, for a consumer that must have a number.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // display only; ordering uses the exact rational
    pub fn as_beats(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }

    /// The exact fraction.
    #[must_use]
    pub const fn parts(self) -> (u64, u64) {
        (self.numerator, self.denominator)
    }
}

impl PartialEq for Onset {
    fn eq(&self, other: &Self) -> bool {
        u128::from(self.numerator) * u128::from(other.denominator)
            == u128::from(other.numerator) * u128::from(self.denominator)
    }
}

impl Eq for Onset {}

impl Ord for Onset {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

impl PartialOrd for Onset {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// One argument of an OSC message.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// A 32-bit integer.
    Int(i32),
    /// A 32-bit float.
    Float(f32),
    /// A string.
    Str(String),
}

impl Arg {
    /// The OSC type tag for this argument.
    const fn tag(&self) -> u8 {
        match self {
            Self::Int(_) => b'i',
            Self::Float(_) => b'f',
            Self::Str(_) => b's',
        }
    }
}

/// One scheduled message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    at: Onset,
    address: String,
    args: Vec<Arg>,
    span: prosody_core::Span,
}

impl Message {
    /// When it sounds, in beats from the start.
    #[must_use]
    pub const fn at(&self) -> Onset {
        self.at
    }

    /// The OSC address.
    #[must_use]
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Its arguments, in order.
    #[must_use]
    pub fn args(&self) -> &[Arg] {
        &self.args
    }

    /// The token that caused it (invariant I4).
    ///
    /// Carried through to the wire format's neighbourhood rather than dropped at the backend: the
    /// Strudel backend loses attribution at emit, and the only reason that is acceptable is that
    /// its output is human-readable text. A binary stream has no such excuse.
    #[must_use]
    pub const fn span(&self) -> prosody_core::Span {
        self.span
    }

    /// The message in OSC 1.0 wire format.
    ///
    /// Hand-written rather than pulled from a crate. `CLAUDE.md` §10 asks what a dependency
    /// replaces and why hand-rolling is worse: here it is not worse. OSC 1.0's encoding for the
    /// three types this uses is about forty lines and *total* — every `Arg` has exactly one
    /// encoding and there is no dialect to get wrong. That is the opposite of the TOML case, where
    /// a partial parser would silently mis-read documents it did not understand.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        pad(&mut out, self.address.as_bytes());

        let mut tags = vec![b','];
        for arg in &self.args {
            tags.push(arg.tag());
        }
        pad(&mut out, &tags);

        for arg in &self.args {
            match arg {
                Arg::Int(value) => out.extend_from_slice(&value.to_be_bytes()),
                Arg::Float(value) => out.extend_from_slice(&value.to_be_bytes()),
                Arg::Str(value) => pad(&mut out, value.as_bytes()),
            }
        }
        out
    }
}

/// Writes a string with its terminating null, padded to a multiple of four bytes.
///
/// OSC's one alignment rule, and the only place this encoder can be got wrong.
fn pad(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(bytes);
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// The address every note is sent to.
pub const NOTE: &str = "/prosody/note";

/// The address the tempo is sent to, once, before the notes.
pub const TEMPO: &str = "/prosody/tempo";

/// A whole piece, scheduled.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    messages: Vec<Message>,
    beats_per_minute: u16,
    unapplied: Vec<(&'static str, String)>,
    unsent_chords: usize,
}

/// The palette keys that shape a number this schedule sends — the pitch and the velocity — but
/// that it does not apply: every pitch is sent from MIDI 36 with no melody/bass split, and every
/// velocity unscaled by density. A header that sets one of them is told so in
/// [`Schedule::to_text`] and on `--send`, rather than having it vanish (`CLAUDE.md` §6: a fallback
/// must be visible). Sample choices (`bank`, `kit`, `ride_fallback`, ...) are not listed: the
/// schedule names no sound at all, so a receiver is choosing those anyway.
const UNAPPLIED: &[&str] = &["bass_octave", "melody_octave", "density_level"];

/// The [`UNAPPLIED`] keys this style sets away from their default, with the values it sets.
fn unapplied(style: &Style) -> Vec<(&'static str, String)> {
    let defaults = Style::default().describe();
    style
        .describe()
        .into_iter()
        .filter(|(key, value)| UNAPPLIED.contains(key) && defaults.get(key) != Some(value))
        .collect()
}

impl Schedule {
    /// Every message, in playing order.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// The tempo the piece plays at.
    ///
    /// The only place beats become seconds, and the only thing here a style decides. A consumer
    /// multiplies an [`Onset`] by `60 / beats_per_minute` to get a time.
    #[must_use]
    pub const fn beats_per_minute(&self) -> u16 {
        self.beats_per_minute
    }

    /// The palette keys the header set that this schedule does not apply, with their values, in
    /// key order. Empty unless a header moves `bass_octave`, `melody_octave` or `density_level`.
    #[must_use]
    pub fn unapplied(&self) -> &[(&'static str, String)] {
        &self.unapplied
    }

    /// How many chords of the harmony layer (`SPEC.md` §4.6) the arrangement plays that this
    /// schedule does not send: it has no chord message yet, and says so rather than dropping them
    /// silently (`CLAUDE.md` §6). Zero unless the harmony mapping is chosen.
    #[must_use]
    pub const fn unsent_chords(&self) -> usize {
        self.unsent_chords
    }

    /// How long the piece lasts, in beats.
    #[must_use]
    pub fn beats(&self) -> f64 {
        self.messages.last().map_or(0.0, |m| m.at.as_beats())
    }

    /// The message that tells a receiver how fast a beat is.
    ///
    /// Sent before the notes. Onsets are in beats — a tempo is palette (`SPEC.md` §5), so it is one
    /// number sent once rather than something baked into every timestamp.
    #[must_use]
    pub fn tempo_message(&self) -> Message {
        Message {
            at: Onset {
                numerator: 0,
                denominator: 1,
            },
            address: TEMPO.to_owned(),
            args: vec![Arg::Float(f32::from(self.beats_per_minute))],
            span: prosody_core::Span::new(0, 0),
        }
    }

    /// The schedule as text, one message a line.
    ///
    /// For `--explain`, for tests, and for anyone who wants to see what would be sent before
    /// sending it.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "# {} bpm, {} messages\n",
            self.beats_per_minute,
            self.messages.len()
        );
        if !self.unapplied.is_empty() {
            let keys: Vec<String> = self
                .unapplied
                .iter()
                .map(|(key, value)| format!("{key} = {value}"))
                .collect();
            let _ = writeln!(
                out,
                "# not applied here: {} (pitch is sent from MIDI 36, velocity unscaled)",
                keys.join(", ")
            );
        }
        if self.unsent_chords > 0 {
            let _ = writeln!(
                out,
                "# not scheduled here: {} chords of the harmony layer (there is no chord message)",
                self.unsent_chords
            );
        }
        for message in &self.messages {
            let args: Vec<String> = message
                .args
                .iter()
                .map(|arg| match arg {
                    Arg::Int(value) => value.to_string(),
                    Arg::Float(value) => fmt_param(f64::from(*value)),
                    Arg::Str(value) => format!("\"{value}\""),
                })
                .collect();
            let _ = writeln!(
                out,
                "{:>9}  {}  {}",
                fmt_param(message.at.as_beats()),
                message.address,
                args.join(" ")
            );
        }
        out
    }
}

impl Backend for Osc {
    type Output = Schedule;
    type Error = OscError;

    fn name(&self) -> &'static str {
        "osc"
    }

    fn render(&self, score: &Score, style: &Style) -> Result<Schedule, OscError> {
        if score.events().is_empty() {
            return Err(OscError::Silent);
        }

        // Walk the arrangement, not the bars: a stanza may sound many times, looping under others
        // or transforming into a rival (`prosody_core::arrange`). Slots differ in length —
        // `SPEC.md` §4.2 insists on it — so their starts are accumulated rather than multiplied,
        // which is the whole reason this backend can honour an odd metre where a fixed-length
        // cycle cannot.
        let mut messages: Vec<Message> = Vec::new();
        let mut slot_start = 0_u64;
        for slot in score.arrangement().slots() {
            let slot_beats = u64::try_from(slot.beats()).unwrap_or(1).max(1);
            for placement in slot.placements() {
                // A whole line, or a call lifted out of one: either way, its own beats.
                let placed = score.placed(*placement);
                let extent = Extent {
                    first: u64::try_from(placed.first_beat()).unwrap_or(0),
                    beats: u64::try_from(placed.beats()).unwrap_or(1).max(1),
                };
                let room = u64::try_from(slot.room_for(placed.beats())).unwrap_or(1);
                for event in placed.events() {
                    let at = fitted(event, slot_start, room, extent);
                    messages.push(message(event, score, style, at));
                }
            }
            slot_start += slot_beats;
        }
        // One total order before anything is sent (`CLAUDE.md` §4). Ties break by span, so two
        // events at the same instant always leave in the same order.
        messages.sort_by(|a, b| a.at.cmp(&b.at).then(a.span.start().cmp(&b.span.start())));

        Ok(Schedule {
            messages,
            beats_per_minute: style
                .tempo_range()
                .at_density(score.events().len(), score.bars())
                .get(),
            unapplied: unapplied(style),
            unsent_chords: score
                .arrangement()
                .slots()
                .iter()
                .filter(|slot| score.chord_in(slot).is_some())
                .count(),
        })
    }
}

/// Which beats of its line a placement covers: from `first`, for `beats`.
#[derive(Debug, Clone, Copy)]
struct Extent {
    first: u64,
    beats: u64,
}

/// When an event sounds: its place within its own line — or within its call, counted from the
/// call's first beat — fitted to the slot it plays in.
///
/// Bar-locked (`prosody_core::arrange`): a line is fitted into its `room` in the slot
/// ([`Slot::room_for`](prosody_core::Slot::room_for)) — its own beats when it fits, so it keeps
/// one beat per foot and rests for the rest of the bar, and the whole bar when it is longer.
fn fitted(event: &Event, slot_start: u64, room: u64, extent: Extent) -> Onset {
    let of = u64::try_from(event.subdivision().of()).unwrap_or(1).max(1);
    let within = u64::try_from(event.subdivision().index()).unwrap_or(0);
    let beat = u64::try_from(event.beat().get())
        .unwrap_or(0)
        .saturating_sub(extent.first);
    let line_beats = extent.beats;

    // §4.3's anaphora row: a line-initial repetition is pulled onto the pulse — its own line's
    // pulse, before fitting, so the pull is the same wherever the line sounds. This is the
    // mapping the text backend could not express, because there a step's position in the cycle
    // already fixes its time and there is nowhere to move it to.
    let (numerator, denominator) = if event.anchor() == Some(Anchor::Downbeat) {
        (beat, 1)
    } else {
        (beat * of + within, of)
    };
    reduced(
        slot_start * denominator * line_beats + numerator * room,
        denominator * line_beats,
    )
}

const fn reduced(numerator: u64, denominator: u64) -> Onset {
    let divisor = gcd(numerator, denominator);
    let divisor = if divisor == 0 { 1 } else { divisor };
    Onset {
        numerator: numerator / divisor,
        denominator: denominator / divisor,
    }
}

const fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a
}

/// One event, scheduled at `at`.
fn message(event: &Event, score: &Score, style: &Style, at: Onset) -> Message {
    // §4.3's epistrophe row: a cadence rings on where its neighbours are clipped.
    let sustain = if event.anchor() == Some(Anchor::Cadence) {
        2.0
    } else {
        1.0
    };

    let layer = score
        .voice(event.voice())
        .map_or(Layer::Flourish, |voice| voice.layer());

    #[allow(clippy::cast_possible_truncation)] // parameters are bounded 0..=1 and printed as f32
    let args = vec![
        // **When it sounds**, in beats from the start of the piece. First, because it is the one
        // argument a receiver cannot do without: an earlier version left it out, so a receiver got
        // every message as fast as UDP would carry it with no idea when any of them belonged. The
        // schedule was computed, printed, and then discarded on the way to the wire.
        Arg::Float(at.as_beats() as f32),
        Arg::Str(match layer {
            Layer::Foundation => "foundation".to_owned(),
            Layer::Flourish => "flourish".to_owned(),
        }),
        Arg::Int(i32::try_from(event.voice().get()).unwrap_or(0)),
        Arg::Float(event.velocity().as_f64() as f32),
        // §4.5: a degree through the style's scale, from its root. Absent for an unpitched event,
        // which is most of them — sent as -1 rather than omitted, so every message has the same
        // shape and a receiver needs no dispatch on argument count.
        Arg::Int(event.pitch().map_or(-1, |pitch| {
            36 + i32::from(style.root().semitones()) + style.scale().degree(pitch.degree().get())
        })),
        Arg::Float(
            event
                .pitch()
                .map_or(0.0, |pitch| pitch.detune().as_f64() * 2.0) as f32,
        ),
        // §4.4.
        Arg::Float(event.sharpness().as_f64() as f32),
        // §4.3's morphological row: depth from the family's root.
        Arg::Float(event.family().map_or(0.0, |family| {
            f64::from(u32::try_from(family.depth().get()).unwrap_or(0)) / 8.0
        }) as f32),
        Arg::Float(sustain),
    ];

    Message {
        at,
        address: NOTE.to_owned(),
        args,
        span: event.span(),
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

    use super::{Arg, Message, Osc, OscError, Schedule, pad};
    use crate::Backend as _;
    use crate::style::Style;

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

    fn schedule(source: &str) -> Schedule {
        Osc::new()
            .render(&score_of(source), &Style::default())
            .unwrap()
    }

    #[test]
    fn a_bar_starts_where_the_previous_one_ended() {
        // SPEC.md §4.2: "iambic pentameter yields five beats […] Don't quantise to 4/4." Bars
        // differ in length, so the second bar of a poem whose first line has three feet starts at
        // beat 3 — not at some fixed cycle boundary. This is what the text backend cannot do:
        // there every cycle is the same span, so the *foot* stretches instead.
        // Two stanzas, so the bars have different periods (§4.6). The outro repeats the last
        // stanza, one beat, so the piece ends two beats after the boundary.
        let source = "bo bA bAn teA rot\n\nteA\n";
        let score = score_of(source);
        let first = score.beats_in(0);
        assert!(
            first > 1,
            "the fixture's first line must be several feet: {first}"
        );
        assert_ne!(
            first,
            score.beats_in(1),
            "and the two bars must differ, or this proves nothing"
        );

        let onsets: Vec<f64> = schedule(source)
            .messages()
            .iter()
            .map(|m| m.at().as_beats())
            .collect();
        assert_eq!(onsets.first().copied(), Some(0.0));
        // The second bar begins exactly where the first ended — accumulated, not multiplied by a
        // fixed cycle length. In mini-notation both bars are one cycle and the *foot* stretches.
        let boundary = f64::from(u32::try_from(first).unwrap_or(0));
        assert!(
            onsets.iter().any(|beat| (beat - boundary).abs() < 1e-9),
            "the second bar starts at beat {boundary}: {onsets:?}"
        );
        assert!(
            onsets.iter().all(|beat| *beat < boundary + 2.0),
            "and nothing sounds past the end of it: {onsets:?}"
        );
    }

    #[test]
    fn a_syllable_lands_inside_its_beat() {
        // §4.2's "syllable → subdivision within the foot", as an exact time rather than a bracket.
        let source = "bo bAn\n";
        let onsets: Vec<(u64, u64)> = schedule(source)
            .messages()
            .iter()
            .map(|m| m.at().parts())
            .collect();
        // Fractions are kept in lowest terms, so the downbeat is 0/1, not 0/2. The outro plays the
        // one-bar poem again from beat 1.
        assert_eq!(
            onsets,
            vec![(0, 1), (1, 2), (1, 1), (3, 2)],
            "two syllables, one foot, twice"
        );
    }

    #[test]
    fn an_anaphora_is_pulled_onto_the_beat() {
        // SPEC.md §4.3: "line-initial n-gram repeats → downbeat-anchored ostinato". This is the
        // mapping `PLAN.md` recorded as the question M9 would answer, and it is answerable only
        // here — in mini-notation a step's position in the cycle already fixes its time.
        let source = "teA bo rAn\nteA bo don\nteA bo ben\n";
        let score = score_of(source);
        let anchored: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.anchor() == Some(super::Anchor::Downbeat))
            .collect();
        assert!(!anchored.is_empty(), "the fixture must contain an anaphora");
        assert!(
            anchored.iter().any(|e| e.subdivision().index() > 0),
            "and at least one anchored syllable must be off the beat to begin with"
        );

        let messages = schedule(source);
        for message in messages.messages() {
            let anchored_here = anchored.iter().any(|e| e.span() == message.span());
            if anchored_here {
                let (numerator, denominator) = message.at().parts();
                assert_eq!(
                    denominator, 1,
                    "an anchored event sounds on the beat: {numerator}/{denominator}"
                );
            }
        }
    }

    #[test]
    fn chords_the_schedule_cannot_send_are_named_not_dropped() {
        // ABAB, one foundation stanza: four bars stated and four in the outro, every one with a
        // chord under harmony = rhyme. None is sent, and the text says how many.
        let source = "dA bA\nro bo\nsA tA\nno do\n";
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let roles = role::assign(&meter, &repetition).unwrap();
        let mappings = prosody_core::Mappings::all().with(
            prosody_core::Mapping::Harmony
                .choose(prosody_core::Variant::Rhyme)
                .unwrap(),
        );
        let score = lower::run_with(
            Analyses {
                poem: &poem,
                meter: &meter,
                rhyme: &rhyme::analyse(&poem, &Spelled),
                repetition: &repetition,
                sound: &sound::analyse(&poem, &Spelled),
                surprisal: &surprisal::analyse(&poem, &Spelled, &Flat),
                roles: &roles,
            },
            &mappings,
        )
        .unwrap();
        let with = Osc::new().render(&score, &Style::default()).unwrap();
        assert_eq!(with.unsent_chords(), 8);
        assert!(
            with.to_text()
                .contains("# not scheduled here: 8 chords of the harmony layer"),
            "{}",
            with.to_text()
        );
        // The messages are the ones the piece without the harmony sends.
        let without = schedule(source);
        assert_eq!(without.unsent_chords(), 0);
        assert_eq!(with.messages(), without.messages());
        assert!(!without.to_text().contains("not scheduled"));
    }

    #[test]
    fn a_cadence_rings_on() {
        // §4.3: "line-final n-gram repeats → cadence figure / turnaround". Duration is the
        // cheapest honest rendering of a gesture of ending.
        let source = "rAn teA bo\ndon teA bo\nben teA bo\n";
        let score = score_of(source);
        assert!(
            score
                .events()
                .iter()
                .any(|e| e.anchor() == Some(super::Anchor::Cadence)),
            "the fixture must contain an epistrophe"
        );
        let sustains: Vec<f32> = schedule(source)
            .messages()
            .iter()
            .filter_map(|m| match m.args().last() {
                Some(Arg::Float(value)) => Some(*value),
                _ => None,
            })
            .collect();
        assert!(sustains.iter().any(|s| *s > 1.5), "{sustains:?}");
        assert!(
            sustains.iter().any(|s| *s < 1.5),
            "and not everything: {sustains:?}"
        );
    }

    #[test]
    fn every_message_carries_when_it_sounds() {
        // The argument a receiver cannot do without. Without it the whole schedule arrives as fast
        // as UDP will carry it and nothing knows what belongs where — which is what happened, and
        // was invisible until the messages were decoded by something other than this crate.
        let source = "bo bAn\nteA rot\n";
        let schedule = schedule(source);
        for message in schedule.messages() {
            let Some(Arg::Float(beats)) = message.args().first() else {
                panic!("the first argument is the onset: {:?}", message.args());
            };
            assert!(
                (f64::from(*beats) - message.at().as_beats()).abs() < 1e-6,
                "the onset on the wire must be the onset in the schedule"
            );
        }
        // ...and the tempo is sent once, separately, because it is palette.
        let tempo = schedule.tempo_message();
        assert_eq!(tempo.address(), super::TEMPO);
        assert_eq!(
            tempo.args(),
            &[Arg::Float(f32::from(schedule.beats_per_minute()))]
        );
        assert!(
            schedule
                .messages()
                .iter()
                .all(|m| m.address() == super::NOTE)
        );
    }

    #[test]
    fn every_message_names_the_token_that_caused_it() {
        // Invariant I4, at the one backend whose output a human cannot read.
        let source = "bo bAn\nteA rot\n";
        let score = score_of(source);
        let spans: std::collections::BTreeSet<_> =
            score.events().iter().map(|e| e.span()).collect();
        for message in schedule(source).messages() {
            assert!(spans.contains(&message.span()), "{:?}", message.span());
            assert!(message.span().text(source).is_some());
        }
    }

    #[test]
    fn messages_leave_in_playing_order() {
        let source = "bo bA bAn\nteA rot\n\nbo don\n";
        let onsets: Vec<_> = schedule(source)
            .messages()
            .iter()
            .map(super::Message::at)
            .collect();
        let mut sorted = onsets.clone();
        sorted.sort_unstable();
        assert_eq!(onsets, sorted);
    }

    #[test]
    fn the_wire_format_is_osc_1_0() {
        // Hand-worked from the specification. `/ab` is three bytes plus a null, padded to four;
        // the type tag `,i` is two bytes plus a null, padded to four; then a big-endian int32.
        let message = Message {
            at: super::Onset {
                numerator: 0,
                denominator: 1,
            },
            address: "/ab".to_owned(),
            args: vec![Arg::Int(1)],
            span: prosody_core::Span::new(0, 1),
        };
        assert_eq!(
            message.encode(),
            vec![b'/', b'a', b'b', 0, b',', b'i', 0, 0, 0, 0, 0, 1]
        );

        // Every OSC element is a multiple of four bytes long, whatever the string lengths.
        for address in ["/a", "/abc", "/abcd", "/abcde"] {
            let message = Message {
                at: super::Onset {
                    numerator: 0,
                    denominator: 1,
                },
                address: address.to_owned(),
                args: vec![Arg::Str("hello".to_owned()), Arg::Float(0.5)],
                span: prosody_core::Span::new(0, 1),
            };
            let encoded = message.encode();
            assert_eq!(
                encoded.len() % 4,
                0,
                "{address} encoded to {}",
                encoded.len()
            );
        }
    }

    #[test]
    fn a_string_is_always_null_terminated_and_padded() {
        // The one alignment rule, and the only place this encoder can be got wrong. Four bytes of
        // content still take a whole extra word, because the null is mandatory.
        let mut out = Vec::new();
        pad(&mut out, b"abcd");
        assert_eq!(out, b"abcd\0\0\0\0");
        let mut out = Vec::new();
        pad(&mut out, b"abc");
        assert_eq!(out, b"abc\0");
    }

    #[test]
    fn the_schedule_is_in_beats_and_the_tempo_is_separate() {
        // Invariant I3 at this backend: a tempo is palette, so changing it must not move a single
        // onset — only the rate at which they are played.
        let source = "bo bAn\nteA rot\n";
        let score = score_of(source);
        let slow = Style::parse("+++\ntempo_range = [60, 60]\n+++").unwrap();
        let fast = Style::parse("+++\ntempo_range = [180, 180]\n+++").unwrap();
        let backend = Osc::new();
        let a = backend.render(&score, &slow).unwrap();
        let b = backend.render(&score, &fast).unwrap();

        assert_ne!(a.beats_per_minute(), b.beats_per_minute());
        assert_eq!(
            a.messages().iter().map(Message::at).collect::<Vec<_>>(),
            b.messages().iter().map(Message::at).collect::<Vec<_>>(),
            "a tempo cannot move an onset"
        );
    }

    #[test]
    fn the_palette_reaches_the_pitch_and_nothing_else_structural() {
        let source = "teA bAn\nteA bAn\n";
        let score = score_of(source);
        let backend = Osc::new();
        let c = backend
            .render(&score, &Style::parse("+++\nroot = \"C\"\n+++").unwrap())
            .unwrap();
        let d = backend
            .render(&score, &Style::parse("+++\nroot = \"D\"\n+++").unwrap())
            .unwrap();
        let notes = |schedule: &Schedule| -> Vec<i32> {
            schedule
                .messages()
                .iter()
                .filter_map(|m| match m.args().get(4) {
                    Some(Arg::Int(note)) if *note >= 0 => Some(*note),
                    _ => None,
                })
                .collect()
        };
        // Per line: `teA`'s stressed AA is the bass's open vowel, degree 0; `bAn` is the rhyme,
        // class degree 0. Both sit on the root — two lines, then both again in the outro.
        assert_eq!(notes(&c), vec![36; 8]);
        assert_eq!(notes(&d), vec![38; 8], "the root moves every note");
        assert_eq!(c.messages().len(), d.messages().len(), "and nothing else");
    }

    #[test]
    fn the_keys_it_does_not_apply_are_named_not_dropped() {
        let score = score_of("teA bAn\nteA bAn\n");
        let backend = Osc::new();
        let plain = backend.render(&score, &Style::default()).unwrap();
        assert!(plain.unapplied().is_empty());
        assert!(!plain.to_text().contains("not applied"));

        let set =
            Style::parse("+++\nbass_octave = 1\ndensity_level = 1\nride_fallback = \"crash\"\n+++")
                .unwrap();
        let moved = backend.render(&score, &set).unwrap();
        // Key order, and only the two that shape a number this schedule sends.
        assert_eq!(
            moved.unapplied(),
            &[
                ("bass_octave", "1".to_owned()),
                ("density_level", "1.00".to_owned())
            ]
        );
        let text = moved.to_text();
        assert_eq!(
            text.lines().nth(1),
            Some(
                "# not applied here: bass_octave = 1, density_level = 1.00 \
                 (pitch is sent from MIDI 36, velocity unscaled)"
            ),
            "{text}"
        );
        // ...and nothing it sends changes: the disclosure is the whole of the difference.
        assert_eq!(moved.messages(), plain.messages());
    }

    #[test]
    fn a_silent_score_is_refused() {
        assert_eq!(
            Osc::new().render(&score_of("zz\n"), &Style::default()),
            Err(OscError::Silent)
        );
    }

    #[test]
    fn scheduling_is_reproducible() {
        assert_eq!(schedule("bo bAn\nteA rot\n"), schedule("bo bAn\nteA rot\n"));
    }

    #[test]
    fn a_looped_line_is_fitted_into_its_hosts_bar() {
        // As in the Strudel test: stanza 2 is stated with stanza 1 looping under it, so slot 2
        // hosts `tA nA` with `bA` under it, and slot 3 hosts `rA sA` with the six-syllable line
        // under it. Bar-locked: a looped line starts with its host, and all of it fits inside the
        // host's bar however many beats it has of its own.
        let source = "bA\nbA bA bA bA bA bA\n\ntA nA\nrA sA\n";
        let score = score_of(source);
        let schedule = schedule(source);
        let onsets_of = |start: usize| -> Vec<f64> {
            schedule
                .messages()
                .iter()
                .filter(|m| m.span().start() == start)
                .map(|m| m.at().as_beats())
                .collect()
        };
        // `bA` at byte 0 sounds three times: at the start, with `tA` (byte 22), and with `tA`
        // again in the outro, where stanza 1 keeps looping under stanza 2's last pass.
        let host = onsets_of(22);
        assert_eq!(onsets_of(0), vec![0.0, host[0], host[1]]);

        // The six-syllable line's words start at bytes 3, 6, … 18; under `rA` (byte 28) every
        // one of them falls inside slot 3.
        let slot_start = onsets_of(28)[0];
        let slot_end =
            slot_start + f64::from(u32::try_from(score.arrangement().slots()[3].beats()).unwrap());
        for word in [3, 6, 9, 12, 15, 18] {
            // Its second sounding: the first is its own statement, the third the outro.
            let looped = onsets_of(word)[1];
            assert!(
                (slot_start..slot_end).contains(&looped),
                "word at {word} sounds at {looped}, outside {slot_start}..{slot_end}"
            );
        }
        assert!(
            (onsets_of(3)[1] - slot_start).abs() < 1e-9,
            "it starts with its host"
        );
    }

    #[test]
    fn a_call_is_fitted_like_any_loop() {
        // `de dA, se sA`: the call is `de dA`, one iamb. Bar 0 plays the line from beat 0; bar 1
        // (`ne nA ne nA`, two beats, from beat 2) has the call under it. One beat fits in two, so
        // it keeps its pace and rests for the second (`Slot::room_for`): `de` on 2, `dA` on 2½.
        let schedule = schedule("de dA, se sA\nne nA ne nA\n");
        let onsets_of = |start: usize| -> Vec<f64> {
            schedule
                .messages()
                .iter()
                .filter(|m| m.span().start() == start)
                .map(|m| m.at().as_beats())
                .collect()
        };
        // Then the outro repeats both bars from beat 4: the whole line at 4 — its call silent
        // under it — and the call alone under bar 1 at 6.
        assert_eq!(onsets_of(0), vec![0.0, 2.0, 4.0, 6.0]);
        assert_eq!(onsets_of(3), vec![0.5, 2.5, 4.5, 6.5]);
        assert_eq!(
            onsets_of(7),
            vec![1.0, 5.0],
            "`se` is past the caesura and is not in the call: only its line's two passes"
        );
    }
}
