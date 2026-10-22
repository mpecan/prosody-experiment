//! The style header: `SPEC.md` §5.
//!
//! > TOML frontmatter. Palette and constraint **only**. […] Never structure. See invariant I3.
//!
//! # Why this type lives in this crate
//!
//! `prosody-core` cannot see it. That is the whole point, and it is the same trick `CLAUDE.md` §2
//! uses to keep the analysis passes pure: a dependency edge is checkable by the compiler, and a
//! rule in a document is not. `prosody-emit` depends on `prosody-core`, so `voice` and `lower`
//! *cannot* import `Style` — not "should not". I3 stops being a claim and becomes an edge.
//!
//! # Palette, constraint, and the one thing the poem chooses
//!
//! Every field here answers "what does it sound like", never "what happens". A scale supplies the
//! notes a rhyme class may take; it cannot decide how many classes there are. A bank supplies the
//! sounds; it cannot decide how many events fire.
//!
//! [`Style::tempo_range`] is the interesting case, and it is why §5 says "palette **and
//! constraint**". A range is not a tempo. The style says the piece lives between 128 and 134; the
//! *poem* says where in that window, from how densely its syllables pack into bars. So a style
//! change can shift the whole window and cannot reorder a single beat, and an edit to the poem
//! moves the tempo within a window it does not control. That is the division §5 describes.

use std::collections::BTreeMap;

/// Why a style header could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StyleError {
    /// The block is not valid TOML.
    #[error("the style header is not valid TOML: {0}")]
    Malformed(String),
    /// A key was present but its value was the wrong shape.
    #[error("`{key}` should be {expected}")]
    Invalid {
        /// The key at fault.
        key: String,
        /// What it should have been.
        expected: String,
    },
    /// A named value is not one this build knows.
    #[error("unknown {kind} `{name}`; known: {known}")]
    Unknown {
        /// What sort of name it is — a scale, a note.
        kind: String,
        /// The name given.
        name: String,
        /// What would have been accepted.
        known: String,
    },
}

/// A note of the chromatic scale, as semitones above C.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Note(u8);

impl Note {
    /// The semitone offset from C, in `0..12`.
    #[must_use]
    pub const fn semitones(self) -> u8 {
        self.0
    }

    /// Parses a note name: `C`, `F#`, `Bb`, `Eb`.
    ///
    /// # Errors
    ///
    /// [`StyleError::Unknown`] if the name is not a note.
    pub fn parse(name: &str) -> Result<Self, StyleError> {
        let unknown = || StyleError::Unknown {
            kind: "note".to_owned(),
            name: name.to_owned(),
            known: "C, C#, Db, D, … B".to_owned(),
        };
        let mut chars = name.chars();
        let letter = chars.next().ok_or_else(unknown)?;
        let base = match letter.to_ascii_uppercase() {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return Err(unknown()),
        };
        let accidental = match chars.next() {
            None => 0_i8,
            Some('#') => 1,
            Some('b') => -1,
            Some(_) => return Err(unknown()),
        };
        if chars.next().is_some() {
            return Err(unknown());
        }
        Ok(Self(
            u8::try_from((base + i16::from(accidental)).rem_euclid(12)).unwrap_or(0),
        ))
    }

    /// The name, always spelled with sharps.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self.0 {
            0 => "c",
            1 => "cs",
            2 => "d",
            3 => "ds",
            4 => "e",
            5 => "f",
            6 => "fs",
            7 => "g",
            8 => "gs",
            9 => "a",
            10 => "as",
            _ => "b",
        }
    }
}

/// A named palette value: a sample bank or a synth.
///
/// Validated against a known list at the IO boundary, like [`Scale`] and [`Note`] and unlike the
/// free strings an earlier version used. Two reasons, and the second is the serious one:
///
/// - An unknown name reaches the backend and produces **silence** — `SPEC.md` §5's own example
///   header says `bank = "tr909"`, and a backend that passes that through unchanged asks Strudel
///   for a sample nobody has. §7.6 exists so a poem can be *heard*.
/// - A free string is written straight into generated JavaScript. `bank = "a\").fast(4).s(\"bd"`
///   appended `.fast(4)` and a second sample source to every voice — a style header rewriting the
///   arrangement, which is exactly what invariant I3 forbids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Named {
    name: &'static str,
}

impl Named {
    /// The name, as written in the header.
    #[must_use]
    pub const fn get(self) -> &'static str {
        self.name
    }

    /// Finds a name in a list, or reports what would have been accepted.
    fn parse(kind: &str, name: &str, known: &[&'static str]) -> Result<Self, StyleError> {
        known
            .iter()
            .find(|candidate| **candidate == name)
            .map(|candidate| Self { name: candidate })
            .ok_or_else(|| StyleError::Unknown {
                kind: kind.to_owned(),
                name: name.to_owned(),
                known: known.join(", "),
            })
    }
}

/// Every sample bank a poem may name.
///
/// The vocabulary is the poem's, not Strudel's: a backend translates these to whatever it can
/// play. `SPEC.md` §5's example uses `tr909`.
///
/// `r8` is the Roland R-8, a sampled *acoustic* kit, and the closest thing to a jazz kit the
/// drum-machine sample map the page loads has: none of its 71 machines has brushes, and the R-8's
/// ride and snare are recordings of real ones (checked against `tidal-drum-machines.json` on
/// `strudel.b-cdn.net`, 2026-10-22: `RolandR8_rd` and `RolandR8_sd` exist).
pub const BANKS: &[&str] = &["tr909", "tr808", "tr707", "linn", "casio", "r8"];

/// Every synth a poem may name. `SPEC.md` §5 names four; the plain waveforms are here so a palette
/// can ask for something ordinary.
pub const SYNTHS: &[&str] = &[
    "karplus",
    "fm",
    "subtractive",
    "physical",
    "sine",
    "sawtooth",
    "square",
    "triangle",
];

/// Every recorded instrument a pitched voice may name: General MIDI soundfonts.
///
/// Named as Strudel names them, because unlike [`SYNTHS`] there is nothing to translate — a GM
/// program *is* a palette word with one meaning. The list is curated rather than complete: basses
/// for the vowel-height bass, keys and mallets for the rhyme melody (`SPEC.md` §4.5 makes every
/// rhyme word a struck note, so struck instruments suit it), a plucked string and a pad. Adding to
/// it is palette work (`CLAUDE.md` §10); every name here must exist in `@strudel/soundfonts`' GM
/// table, or it plays silence.
pub const SOUNDFONTS: &[&str] = &[
    "gm_acoustic_bass",
    "gm_electric_bass_finger",
    "gm_electric_bass_pick",
    "gm_fretless_bass",
    "gm_synth_bass_1",
    "gm_synth_bass_2",
    "gm_epiano1",
    "gm_epiano2",
    "gm_piano",
    "gm_vibraphone",
    "gm_marimba",
    "gm_celesta",
    "gm_music_box",
    "gm_kalimba",
    "gm_glockenspiel",
    "gm_acoustic_guitar_nylon",
    "gm_orchestral_harp",
    "gm_pad_warm",
    "gm_string_ensemble_1",
];

/// Every named palette — a starting point a header or the page can pick with `preset = "…"`.
///
/// A preset sets only the sound and the mix; scale, root, tempo window, swing and bank are left
/// as they were, so choosing one never moves a note. See [`Style::preset`].
///
/// One exception, `jazz`: a jazz palette over a minor scale played straight is not a jazz palette,
/// so it also sets a scale, a swing and a bank (see [`preset_keys`]). Every one of those is still
/// palette — I3 lets a style move every note and every onset by a fraction of a beat, and no
/// event — but choosing `jazz` does move notes, where the other three never do.
pub const PRESETS: &[&str] = &["default", "warm", "raw", "jazz"];

/// The header keys a preset sets. Every preset sets the sound and the mix; `jazz` also sets its
/// scale, swing and bank. The page's Sound menu applies exactly these, so choosing `warm` after
/// `jazz` changes the sound back and leaves the scale where `jazz` put it.
#[must_use]
pub fn preset_keys(name: &str) -> Option<Vec<&'static str>> {
    let preset = preset_palette(name)?;
    let mut keys = SOUND_KEYS.to_vec();
    if preset.feel.is_some() {
        keys.extend(["scale", "swing", "bank"]);
    }
    Some(keys)
}

/// The keys every preset sets: the instruments, how they are played, and the mix.
const SOUND_KEYS: &[&str] = &[
    "bass",
    "melody",
    "kit",
    "ride_fallback",
    "voicing",
    "bass_octave",
    "melody_octave",
    "chord_sound",
    "reverb",
    "room_size",
    "delay",
    "high_cut",
    "drive",
    "compress",
    "humanise",
    "drums_gain",
    "bass_gain",
    "melody_gain",
    "chord_gain",
    "density_level",
];

/// Which sound each drum layer plays.
///
/// `SPEC.md` §4.1 makes a foundation the pulse and a flourish its ornament — that is structure, and
/// it comes from the score. Which *sounds* carry them is palette, as `strudel::sample_for` has
/// always said; this names the choice so a palette can make it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kit {
    /// A kick on the foundation and a closed hat on the flourish: the drum machine.
    Machine,
    /// A ride cymbal on the foundation and the snare on the flourish. In a jazz rhythm section the
    /// ride keeps the time and the snare comps around it, which is exactly the foundation/flourish
    /// division.
    Ride,
}

/// Every kit a header may name, as it names them.
pub const KITS: &[&str] = &["machine", "ride"];

impl Kit {
    /// Its name in a header.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Ride => "ride",
        }
    }

    fn parse(name: &str) -> Result<Self, StyleError> {
        match name {
            "machine" => Ok(Self::Machine),
            "ride" => Ok(Self::Ride),
            _ => Err(StyleError::Unknown {
                kind: "kit".to_owned(),
                name: name.to_owned(),
                known: KITS.join(", "),
            }),
        }
    }
}

/// What keeps time under [`Kit::Ride`] on a bank that has no ride sample.
///
/// The TR-808 and TR-707 in the sample map have no ride, and asking for one silences the whole
/// foundation layer (`SPEC.md` §4.1's pulse), which no palette may do. Which drum stands in is a
/// choice of colour, not of events — the stand-in strikes exactly where the ride would — so it is
/// palette, and a header can make it. Both stand-ins are in every bank the page loads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RideFallback {
    /// The open hat: the drum machine's usual stand-in for a ride, short and bright.
    OpenHat,
    /// The crash: a real cymbal with a long wash, nearer a ride's ring, and louder.
    Crash,
}

/// Every ride fallback a header may name.
pub const RIDE_FALLBACKS: &[&str] = &["open_hat", "crash"];

impl RideFallback {
    /// Its name in a header.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::OpenHat => "open_hat",
            Self::Crash => "crash",
        }
    }

    /// What the pattern's header calls it.
    #[must_use]
    pub const fn spoken(self) -> &'static str {
        match self {
            Self::OpenHat => "open hat",
            Self::Crash => "crash",
        }
    }

    fn parse(name: &str) -> Result<Self, StyleError> {
        match name {
            "open_hat" => Ok(Self::OpenHat),
            "crash" => Ok(Self::Crash),
            _ => Err(StyleError::Unknown {
                kind: "ride_fallback".to_owned(),
                name: name.to_owned(),
                known: RIDE_FALLBACKS.join(", "),
            }),
        }
    }
}

/// The octave a pitched role plays in, in scientific pitch notation: octave 4 starts at middle C,
/// MIDI 60.
///
/// The register moves every note of a role by whole octaves and moves no event, so it is palette
/// (`SPEC.md` §5). Each role has its own range, validated at the boundary: the bass from octave 1
/// to 3 (MIDI 24–48), where a bass instrument has its body, and the melody from 3 to 5 (MIDI
/// 48–72), where keys and mallets sing. The ranges meet at octave 3 and never cross, so the
/// melody is never below the bass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Octave(u8);

impl Octave {
    /// The octave number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// The MIDI note its C has: octave 2 is 36, octave 4 is 60.
    #[must_use]
    pub fn midi_c(self) -> i32 {
        12 * (i32::from(self.0) + 1)
    }
}

/// The octaves the bass may play in.
pub const BASS_OCTAVES: std::ops::RangeInclusive<u8> = 1..=3;

/// The octaves the melody may play in.
pub const MELODY_OCTAVES: std::ops::RangeInclusive<u8> = 3..=5;

/// How a rhyme note is voiced: one tone, or a seventh chord built on it.
///
/// # Why this is palette (invariant I3)
///
/// I3 lets a style change how an event is *realised* and never which events exist or where they
/// fall. A chord voicing is a realisation of one event, in the way an organ's mixture stop sounds
/// several pipes for one key, or a soundfont sample carries many partials for one note:
///
/// - **Same events.** Each rhyme word is still one event at one onset, with one gain, one cutoff
///   and one stray, and every tone of its chord takes those same values. No event is added, moved
///   or removed, and the chord ends when the note would.
/// - **Same degree, from the same word.** The chord is built on the event's own scale degree — the
///   rhyme class's (`SPEC.md` §4.5) — by stacking every other degree of the *style's* scale above
///   it: degrees *d*, *d*+2, *d*+4, *d*+6. Nothing else goes in. It does not look at the previous
///   chord, the next one, the stanza or the bass, so it cannot express a progression: which rhyme
///   classes share a note, and when they change, is still entirely the poem's. Every tone traces to
///   the one rhyme word's span (I4).
/// - **A function of palette only.** Given the degree, the tones depend on the scale alone, which
///   §5 already makes palette. Switch the voicing off and the lowest tone of every chord is the note
///   that played before — the I3 property test in `prosody-emit/tests/i3.rs` checks exactly that.
///
/// The name is the seven-note case. Stacking every other degree builds a seventh chord only in a
/// seven-note scale: in `pentatonic_minor` it stacks fourths (C–F–B♭–E♭), in `whole_tone` an
/// augmented stack, in `chromatic` a cluster of whole tones. That is still one event voiced from
/// palette alone, so still palette; the pattern's header says which it is.
///
/// What it does *not* do is harmonise the poem. A chord progression derived from the poem — which
/// chord follows which, decided by stanza or rhyme scheme — would be a new mapping and therefore
/// structure; it is a proposal in `PLAN.md`, not this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Voicing {
    /// One tone per rhyme word: the melody as §4.5 first describes it.
    Note,
    /// A seventh chord on the rhyme word's degree, in close root position.
    Seventh,
}

/// Every voicing a header may name.
pub const VOICINGS: &[&str] = &["note", "seventh"];

impl Voicing {
    /// Its name in a header.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Seventh => "seventh",
        }
    }

    /// The scale-degree offsets above a rhyme note that sound with it: the note alone, or every
    /// other degree up to the seventh.
    #[must_use]
    pub const fn degrees(self) -> &'static [usize] {
        match self {
            Self::Note => &[0],
            Self::Seventh => &[0, 2, 4, 6],
        }
    }

    fn parse(name: &str) -> Result<Self, StyleError> {
        match name {
            "note" => Ok(Self::Note),
            "seventh" => Ok(Self::Seventh),
            _ => Err(StyleError::Unknown {
                kind: "voicing".to_owned(),
                name: name.to_owned(),
                known: VOICINGS.join(", "),
            }),
        }
    }
}

/// A scale: which semitones, relative to the root, its degrees take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scale {
    name: &'static str,
    steps: &'static [u8],
}

impl Scale {
    /// The scale's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The semitone offsets of its degrees, within one octave.
    #[must_use]
    pub const fn steps(self) -> &'static [u8] {
        self.steps
    }

    /// How many degrees before it repeats an octave higher.
    #[must_use]
    pub const fn len(self) -> usize {
        self.steps.len()
    }

    /// Whether the scale has no degrees. Never true for a scale from [`Scale::parse`].
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.steps.is_empty()
    }

    /// The semitone offset of a degree, wrapping into higher octaves as the degree exceeds the
    /// scale's length.
    ///
    /// A poem with more rhyme classes than the scale has notes keeps going upward rather than
    /// folding back onto notes already in use — two rhyme classes that sound the same pitch would
    /// be two distinct poetic facts made indistinguishable, which is what invariant I4 is for.
    #[must_use]
    pub fn degree(self, degree: usize) -> i32 {
        if self.steps.is_empty() {
            return 0;
        }
        let octave = degree / self.steps.len();
        let within = degree % self.steps.len();
        let step = self.steps.get(within).copied().unwrap_or(0);
        i32::from(step) + 12 * i32::try_from(octave).unwrap_or(0)
    }

    /// Parses a scale name.
    ///
    /// # Errors
    ///
    /// [`StyleError::Unknown`] if the name is not one of [`Scale::ALL`].
    pub fn parse(name: &str) -> Result<Self, StyleError> {
        Self::ALL
            .iter()
            .find(|scale| scale.name == name)
            .copied()
            .ok_or_else(|| StyleError::Unknown {
                kind: "scale".to_owned(),
                name: name.to_owned(),
                known: Self::ALL
                    .iter()
                    .map(|s| s.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            })
    }

    /// Every scale this build knows.
    ///
    /// `SPEC.md` §5's example is `phrygian_dominant`; the rest are here because a palette with one
    /// colour is not a palette. Adding to this list is style work, which `CLAUDE.md` §10 says is
    /// legitimate and separate from changing a mapping.
    /// The scale a poem with no header gets.
    ///
    /// Referenced by [`Style::default`] rather than duplicated there: two copies of the same steps
    /// could drift, and a headerless poem would then play different notes from one that wrote
    /// `scale = "minor"`.
    pub const MINOR: Self = Self {
        name: "minor",
        steps: &[0, 2, 3, 5, 7, 8, 10],
    };

    /// Every scale, in a fixed order.
    pub const ALL: &'static [Self] = &[
        Self {
            name: "major",
            steps: &[0, 2, 4, 5, 7, 9, 11],
        },
        Self::MINOR,
        Self {
            name: "harmonic_minor",
            steps: &[0, 2, 3, 5, 7, 8, 11],
        },
        Self {
            name: "dorian",
            steps: &[0, 2, 3, 5, 7, 9, 10],
        },
        Self {
            name: "phrygian",
            steps: &[0, 1, 3, 5, 7, 8, 10],
        },
        Self {
            name: "phrygian_dominant",
            steps: &[0, 1, 4, 5, 7, 8, 10],
        },
        Self {
            name: "lydian",
            steps: &[0, 2, 4, 6, 7, 9, 11],
        },
        Self {
            name: "mixolydian",
            steps: &[0, 2, 4, 5, 7, 9, 10],
        },
        Self {
            name: "locrian",
            steps: &[0, 1, 3, 5, 6, 8, 10],
        },
        Self {
            name: "pentatonic_minor",
            steps: &[0, 3, 5, 7, 10],
        },
        Self {
            name: "whole_tone",
            steps: &[0, 2, 4, 6, 8, 10],
        },
        Self {
            name: "chromatic",
            steps: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        },
    ];
}

/// Beats per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bpm(u16);

impl Bpm {
    /// The tempo.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// The tempo window a piece must sit in (`SPEC.md` §5).
///
/// A window, not a tempo: the style constrains and the poem chooses. Validated on the way in, so
/// nothing downstream re-checks that `low <= high` — `CLAUDE.md` §5's parse-don't-validate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TempoRange {
    low: Bpm,
    high: Bpm,
}

impl TempoRange {
    /// The slowest tempo allowed.
    #[must_use]
    pub const fn low(self) -> Bpm {
        self.low
    }

    /// The fastest tempo allowed.
    #[must_use]
    pub const fn high(self) -> Bpm {
        self.high
    }

    /// Where in the window a piece of the given density sits.
    ///
    /// `syllables` and `bars` come from the poem. A denser poem — more syllables per line — takes
    /// the faster end, because it has more to say per bar. Integer arithmetic throughout, so the
    /// tempo is byte-stable across platforms (`CLAUDE.md` §4).
    ///
    /// `DENSE` is the syllables-per-bar at which the window is exhausted. Twelve is a long line in
    /// English verse — an alexandrine — so ordinary metres spread across the window rather than
    /// pinning to its top.
    #[must_use]
    pub fn at_density(self, syllables: usize, bars: usize) -> Bpm {
        const DENSE: usize = 12;
        if bars == 0 || self.low == self.high {
            return self.low;
        }
        let per_bar = (syllables / bars).min(DENSE);
        let span = u32::from(self.high.0 - self.low.0);
        let offset = span * u32::try_from(per_bar).unwrap_or(0) / u32::try_from(DENSE).unwrap_or(1);
        Bpm(self
            .low
            .0
            .saturating_add(u16::try_from(offset).unwrap_or(0)))
    }
}

/// A palette (`SPEC.md` §5). Never structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Style {
    scale: Scale,
    root: Note,
    tempo_range: TempoRange,
    swing: Swing,
    bank: Named,
    bass: Named,
    melody: Named,
    chord_sound: Named,
    kit: Kit,
    ride_fallback: RideFallback,
    voicing: Voicing,
    bass_octave: Octave,
    melody_octave: Octave,
    mix: Mix,
}

/// How much to delay every second subdivision, in hundredths.
///
/// An integer, not the float `SPEC.md` §5 writes, because a swing amount is compared and
/// interpolated and `CLAUDE.md` §4 keeps floats out of both. `0.12` in the header becomes `12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Swing(u8);

impl Swing {
    /// Hundredths of a subdivision.
    #[must_use]
    pub const fn hundredths(self) -> u8 {
        self.0
    }
}

/// An amount between nothing and all of it, in hundredths: `0.35` in the header is `35`.
///
/// Integer for the same reason as [`Swing`]: amounts are compared (zero disables an effect), and
/// `CLAUDE.md` §4 keeps floats out of control flow. A float exists only on the way out, through
/// [`Amount::as_f64`] and `fmt_param`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(u16);

impl Amount {
    /// Hundredths.
    #[must_use]
    pub const fn hundredths(self) -> u16 {
        self.0
    }

    /// Whether the amount is zero — the effect is off.
    #[must_use]
    pub const fn is_off(self) -> bool {
        self.0 == 0
    }

    /// As a fraction, for output only.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / 100.0
    }

    fn describe(self) -> String {
        format!("{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

/// The mix: how the instruments sit together. Palette, every field — none of it decides an event.
///
/// Each field is validated to a range at the IO boundary ([`Style::parse`]), so a backend reads
/// them without re-checking (`CLAUDE.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mix {
    reverb: Amount,
    room_size: u8,
    delay: Amount,
    high_cut: u16,
    drive: Amount,
    compress: Amount,
    humanise: Amount,
    drums_gain: Amount,
    bass_gain: Amount,
    melody_gain: Amount,
    chord_gain: Amount,
    density_level: Amount,
}

impl Mix {
    /// How much of everything is sent to the reverb, `0..=1`. The drums' send also rises with a
    /// word's distinctiveness (`SPEC.md` §4.4); this is the floor the mapping stands on.
    #[must_use]
    pub const fn reverb(self) -> Amount {
        self.reverb
    }

    /// The reverb's length, in tenths of a second, `1..=100`.
    #[must_use]
    pub const fn room_size_tenths(self) -> u8 {
        self.room_size
    }

    /// How much of the melody is sent to an echo, `0..=1`.
    #[must_use]
    pub const fn delay(self) -> Amount {
        self.delay
    }

    /// The brightest a filter opens, in hertz, `1000..=20000` — the top of `SPEC.md` §4.4's
    /// brightness range. Lowering it scales the range rather than clipping it, so a duller palette
    /// still makes a striking word brighter than a plain one.
    #[must_use]
    pub const fn high_cut(self) -> u16 {
        self.high_cut
    }

    /// Soft saturation on the drums and bass, `0..=1`.
    #[must_use]
    pub const fn drive(self) -> Amount {
        self.drive
    }

    /// Compression on every layer, `0..=1`; zero is none.
    #[must_use]
    pub const fn compress(self) -> Amount {
        self.compress
    }

    /// How far each event strays from the grid and from its stress's gain, `0..=1`; zero is a
    /// machine. The strays come from each line's own seed (invariant I2).
    #[must_use]
    pub const fn humanise(self) -> Amount {
        self.humanise
    }

    /// The drums' level, `0.1..=2` — never zero; see `read_level`.
    #[must_use]
    pub const fn drums_gain(self) -> Amount {
        self.drums_gain
    }

    /// The bass's level, `0.1..=2` — never zero; see `read_level`.
    #[must_use]
    pub const fn bass_gain(self) -> Amount {
        self.bass_gain
    }

    /// The melody's level, `0.1..=2` — never zero; see `read_level`.
    #[must_use]
    pub const fn melody_gain(self) -> Amount {
        self.melody_gain
    }

    /// The chord lane's level, `0.1..=2` — never zero; see `read_level`. The lane sounds only when
    /// the harmony mapping is chosen (`SPEC.md` §4.6); the backend shares it between a chord's
    /// tones, as it does the melody's.
    #[must_use]
    pub const fn chord_gain(self) -> Amount {
        self.chord_gain
    }

    /// How far levels follow density, `0..=1`; zero is the fixed levels above.
    ///
    /// Every gain in a bar is divided by *n* to this power, where *n* is how many lines sound in
    /// that bar: at `0.5` by √*n*, which holds the power of *n* uncorrelated lines at one line's,
    /// and at `1` by *n*, which holds their summed peak when their hits coincide — as a stanza
    /// looping under the next mostly does, since both sit on the bar's beats. A bar of one line is
    /// never changed. It reads how many lines a bar sounds, which is the score's structure, and
    /// changes nothing but gain values; see `strudel::density` for the argument against I3.
    #[must_use]
    pub const fn density_level(self) -> Amount {
        self.density_level
    }
}

/// What a named preset sets: always the sound and the mix, and for `jazz` a feel as well.
#[derive(Debug, Clone, Copy)]
struct Preset {
    bass: &'static str,
    melody: &'static str,
    chords: &'static str,
    kit: Kit,
    voicing: Voicing,
    mix: Mix,
    feel: Option<Feel>,
}

/// The palette values beyond the sound that a preset may also set.
#[derive(Debug, Clone, Copy)]
struct Feel {
    scale: Scale,
    swing: Swing,
    bank: &'static str,
}

/// Dorian, from [`Scale::ALL`] by name rather than duplicated, for the reason
/// [`Scale::MINOR`] gives.
fn dorian() -> Scale {
    Scale::parse("dorian").unwrap_or(Scale::MINOR)
}

/// The named presets.
///
/// Four, so that the before and after of this palette can be heard side by side:
///
/// - **default** — what a poem with no header gets: an electric-piano melody over an acoustic
///   bass, a modest room, a little echo on the melody, light compression and a small human stray.
/// - **warm** — softer and wider: vibraphone over fretless bass, a long room, more echo, a lower
///   high cut, a touch of saturation, and more stray; levels follow density at `0.5`.
/// - **raw** — the palette as it was before soundfonts and a mix existed: triangle oscillators,
///   dry, on the grid. Kept so the difference stays audible, not as a recommendation.
/// - **jazz** — a piano trio's colours: Rhodes voicing every rhyme word as a seventh chord, an
///   upright bass, the ride keeping time with the snare comping, triplet swing, dorian, and a
///   warm, darker room with more human stray; levels follow density at `0.5`. See [`jazz`].
///
/// The levels were set by measurement, not by ear — nobody could listen where they were set.
/// Measured in headless Chromium on Strudel's summed output before the page's master stage, as the
/// peak over twelve seconds, with one role at unity and the others turned right down: on the
/// opening poem the drums peaked at 2.10, the bass at 1.60 and the melody at 0.76; on the Wilde
/// villanelle, which sounds the most lines at once, 3.49, 1.62 and 1.37. The drum-machine samples
/// are normalised loud and several lines' hits coincide; the bass sounds on every stressed
/// foundation syllable; a soundfont note peaks well below full scale. The levels bring each role's
/// peak on the opening poem to roughly 0.5–0.6, so that the rhyme words (`SPEC.md` §4.5), the
/// sparsest and most traceable events, are as present as the pulse under them. Summed, the
/// default palette then peaks near 1.0 on the opening poem and higher on the densest poems, which
/// the page's master stage (a gentle compressor, then a soft clipper whose output cannot pass
/// 0.95) catches. Warm's longer room, echo and drive add up, so its levels sit lower again.
///
/// An earlier set (0.50, 0.55, 1.30) was tuned against a measurement taken too early in the piece
/// and peaked at 1.6 to 2.9 before the master; the compressor alone let drum onsets through above
/// full scale.
fn preset_palette(name: &str) -> Option<Preset> {
    let sound = |bass, melody, chords, mix| Preset {
        bass,
        melody,
        chords,
        kit: Kit::Machine,
        voicing: Voicing::Note,
        mix,
        feel: None,
    };
    match name {
        "default" => Some(sound(
            "gm_acoustic_bass",
            "gm_epiano1",
            // A pad under the electric piano's rhyme notes, so a chord is heard as harmony and
            // not as a louder melody: the two share a register's neighbourhood.
            "gm_pad_warm",
            Mix {
                reverb: Amount(25),
                room_size: 20,
                delay: Amount(15),
                high_cut: 4400,
                drive: Amount(0),
                compress: Amount(30),
                humanise: Amount(20),
                drums_gain: Amount(28),
                bass_gain: Amount(32),
                melody_gain: Amount(75),
                chord_gain: Amount(40),
                density_level: Amount(0),
            },
        )),
        "warm" => Some(sound(
            "gm_fretless_bass",
            "gm_vibraphone",
            // Electric piano comping under the vibraphone.
            "gm_epiano1",
            Mix {
                reverb: Amount(45),
                room_size: 40,
                delay: Amount(30),
                high_cut: 3000,
                drive: Amount(15),
                compress: Amount(40),
                humanise: Amount(40),
                drums_gain: Amount(24),
                bass_gain: Amount(24),
                melody_gain: Amount(60),
                chord_gain: Amount(35),
                // Measured (PLAN.md, *sound and voice, step 4.2*): the Wilde villanelle peaked at
                // 0.80 before the master at 0, 0.61 at 0.5 — out of the soft clipper's knee.
                density_level: Amount(50),
            },
        )),
        "raw" => Some(sound(
            "triangle",
            "triangle",
            "triangle",
            Mix {
                reverb: Amount(0),
                room_size: 20,
                delay: Amount(0),
                high_cut: 4400,
                drive: Amount(0),
                compress: Amount(0),
                humanise: Amount(0),
                drums_gain: Amount(100),
                bass_gain: Amount(100),
                melody_gain: Amount(100),
                chord_gain: Amount(100),
                density_level: Amount(0),
            },
        )),
        "jazz" => Some(jazz()),
        _ => None,
    }
}

/// The jazz palette. Every value is palette; each says what it is for.
///
/// - **Rhodes, as seventh chords** (`gm_epiano1`, [`Voicing::Seventh`]) — the comping instrument of
///   a great deal of small-group jazz, voicing each rhyme word's degree as the diatonic seventh
///   chord on it. The melody level is the chord's: the backend shares it between the four tones.
/// - **Upright bass** (`gm_acoustic_bass`, General MIDI's program 33, *Acoustic Bass*: the
///   upright), a little louder than the default because in a trio it carries the harmony's root.
/// - **The ride keeps time, the snare comps** ([`Kit::Ride`] on `r8`, the sampled acoustic kit;
///   no brushed kit exists in the sample map). The drums sit lower than the default's because a
///   ride rings longer than a kick and every foundation syllable strikes it.
/// - **Triplet swing**, `0.33`: Strudel's `swingBy(x, n)` delays the second half of each of a
///   bar's `n` beats by `x / 2` of a beat, so a third puts an iamb's second syllable at two thirds
///   of the beat — the 2:1 long–short of a swung eighth (`swing(n)` is the same `1/3`;
///   `@strudel/core` `pattern.mjs`, as bundled in `@strudel/web` 1.3.0).
/// - **Dorian** — the minor mode of modal jazz. Its raised sixth makes the seventh chord on its
///   fourth degree a dominant seventh, so the chords on degrees 0 and 3 are the minor-seventh and
///   dominant-seventh pair of the classic dorian vamp (Dm7 and G7 in D).
/// - **Warm mix** — a longer, darker room than the default (lower high cut, more reverb), little
///   echo (the chords already fill the space between rhyme words), a touch of drive and twice the
///   default's human stray.
///
/// The levels are an estimate from the default's measurements, not measured themselves: a chord's
/// four soundfont tones at half level each sum, uncorrelated, to about the single note's level.
fn jazz() -> Preset {
    Preset {
        bass: "gm_acoustic_bass",
        melody: "gm_epiano1",
        // The Rhodes comps the changes too, an octave under its rhyme chords.
        chords: "gm_epiano1",
        kit: Kit::Ride,
        voicing: Voicing::Seventh,
        mix: Mix {
            reverb: Amount(35),
            room_size: 25,
            delay: Amount(5),
            high_cut: 3200,
            drive: Amount(5),
            compress: Amount(35),
            humanise: Amount(40),
            drums_gain: Amount(22),
            bass_gain: Amount(40),
            melody_gain: Amount(75),
            chord_gain: Amount(45),
            // Measured: the Wilde villanelle peaked at 0.72 before the master at 0, 0.51 at 0.5.
            // A ride rings on, so a bar of three lines' rides stacks up longer than three kicks.
            density_level: Amount(50),
        },
        feel: Some(Feel {
            scale: dorian(),
            swing: Swing(33),
            bank: "r8",
        }),
    }
}

impl Default for Style {
    /// The palette a poem with no header gets: the `default` preset.
    ///
    /// A poem is a complete score on its own; the header only recolours it. Defaulting rather than
    /// refusing is what makes that true — and `SPEC.md` §7.6 wants the cheapest possible audition
    /// path, which a mandatory header would not be.
    fn default() -> Self {
        let mut style = Self {
            scale: Scale::MINOR,
            root: Note(0),
            tempo_range: TempoRange {
                low: Bpm(110),
                high: Bpm(130),
            },
            swing: Swing(0),
            bank: Named { name: "tr909" },
            bass: Named { name: "triangle" },
            melody: Named { name: "triangle" },
            chord_sound: Named { name: "triangle" },
            kit: Kit::Machine,
            ride_fallback: RideFallback::OpenHat,
            voicing: Voicing::Note,
            bass_octave: Octave(2),
            melody_octave: Octave(4),
            mix: Mix {
                reverb: Amount(0),
                room_size: 20,
                delay: Amount(0),
                high_cut: 4400,
                drive: Amount(0),
                compress: Amount(0),
                humanise: Amount(0),
                drums_gain: Amount(100),
                bass_gain: Amount(100),
                melody_gain: Amount(100),
                chord_gain: Amount(100),
                density_level: Amount(0),
            },
        };
        style.apply_preset("default");
        style
    }
}

impl Style {
    /// The scale its degrees index into.
    #[must_use]
    pub const fn scale(&self) -> Scale {
        self.scale
    }

    /// The note degree zero starts from.
    #[must_use]
    pub const fn root(&self) -> Note {
        self.root
    }

    /// The tempo window.
    #[must_use]
    pub const fn tempo_range(&self) -> TempoRange {
        self.tempo_range
    }

    /// How much to swing.
    #[must_use]
    pub const fn swing(&self) -> Swing {
        self.swing
    }

    /// The sample bank percussive voices play from.
    #[must_use]
    pub const fn bank(&self) -> Named {
        self.bank
    }

    /// The instrument the bass plays — one of [`SYNTHS`] or [`SOUNDFONTS`].
    #[must_use]
    pub const fn bass(&self) -> Named {
        self.bass
    }

    /// The instrument the rhyme melody plays — one of [`SYNTHS`] or [`SOUNDFONTS`].
    #[must_use]
    pub const fn melody(&self) -> Named {
        self.melody
    }

    /// The instrument the chord lane plays — one of [`SYNTHS`] or [`SOUNDFONTS`]. The lane sounds
    /// only when the harmony mapping is chosen (`SPEC.md` §4.6): which chord, and when, is the
    /// poem's; what it sounds like is this.
    #[must_use]
    pub const fn chord_sound(&self) -> Named {
        self.chord_sound
    }

    /// Which sound each drum layer plays.
    #[must_use]
    pub const fn kit(&self) -> Kit {
        self.kit
    }

    /// How a rhyme note is voiced.
    #[must_use]
    pub const fn voicing(&self) -> Voicing {
        self.voicing
    }

    /// What keeps time under the ride kit on a bank with no ride.
    #[must_use]
    pub const fn ride_fallback(&self) -> RideFallback {
        self.ride_fallback
    }

    /// The octave the bass plays in, within [`BASS_OCTAVES`].
    #[must_use]
    pub const fn bass_octave(&self) -> Octave {
        self.bass_octave
    }

    /// The octave the rhyme melody plays in, within [`MELODY_OCTAVES`].
    #[must_use]
    pub const fn melody_octave(&self) -> Octave {
        self.melody_octave
    }

    /// The mix.
    #[must_use]
    pub const fn mix(&self) -> Mix {
        self.mix
    }

    /// A named palette over the default scale, root, tempo and bank — except where the preset sets
    /// them itself ([`preset_keys`]); `None` for an unknown name.
    #[must_use]
    pub fn preset(name: &str) -> Option<Self> {
        let mut style = Self::default();
        style.apply_preset(name).then_some(style)
    }

    fn apply_preset(&mut self, name: &str) -> bool {
        let Some(preset) = preset_palette(name) else {
            return false;
        };
        self.bass = Named { name: preset.bass };
        self.melody = Named {
            name: preset.melody,
        };
        self.chord_sound = Named {
            name: preset.chords,
        };
        self.kit = preset.kit;
        self.voicing = preset.voicing;
        self.mix = preset.mix;
        // Every preset plays in today's register and keeps time on the open hat where a bank has
        // no ride: the keys are in `SOUND_KEYS`, so the page's Sound menu restores them too.
        self.ride_fallback = RideFallback::OpenHat;
        self.bass_octave = Octave(2);
        self.melody_octave = Octave(4);
        if let Some(feel) = preset.feel {
            self.scale = feel.scale;
            self.swing = feel.swing;
            self.bank = Named { name: feel.bank };
        }
        true
    }

    /// Reads a style header — the whole `+++` block, delimiters and all.
    ///
    /// Every value is validated here and nowhere else (`CLAUDE.md` §5). Unknown keys are ignored
    /// rather than refused, so a header written for a later version still plays.
    ///
    /// Three keys are applied before the rest, so that a header reads the same in any order:
    /// `preset` first (a starting point), then `synth` (the older key, which sets both pitched
    /// instruments at once), then everything else — so `bass` or `melody` overrides `synth`, and
    /// any mix key overrides the preset.
    ///
    /// # Errors
    ///
    /// [`StyleError`] if the block is not TOML, or a value is the wrong shape or an unknown name.
    pub fn parse(frontmatter: &str) -> Result<Self, StyleError> {
        let body = frontmatter
            .trim()
            .trim_start_matches("+++")
            .trim_end_matches("+++");
        let parsed: toml::Table = body
            .parse()
            .map_err(|e: toml::de::Error| StyleError::Malformed(e.message().to_owned()))?;

        // `SPEC.md` §5 nests everything under `[style]`. A bare table is accepted too: the
        // section header carries no information when it is the only one.
        let table: &toml::Table = match parsed.get("style") {
            Some(toml::Value::Table(inner)) => inner,
            Some(_) => {
                return Err(StyleError::Invalid {
                    key: "style".to_owned(),
                    expected: "a table".to_owned(),
                });
            }
            None => &parsed,
        };

        let mut style = Self::default();
        let wrong = |key: &str, expected: &str| StyleError::Invalid {
            key: key.to_owned(),
            expected: expected.to_owned(),
        };

        match table.get("preset") {
            None => {}
            Some(toml::Value::String(name)) => {
                if !style.apply_preset(name) {
                    return Err(StyleError::Unknown {
                        kind: "preset".to_owned(),
                        name: name.clone(),
                        known: PRESETS.join(", "),
                    });
                }
            }
            Some(_) => return Err(wrong("preset", "a string")),
        }
        match table.get("synth") {
            None => {}
            Some(toml::Value::String(name)) => {
                let synth = instrument("synth", name)?;
                style.bass = synth;
                style.melody = synth;
            }
            Some(_) => return Err(wrong("synth", "a string")),
        }

        for (key, value) in table {
            let mix = &mut style.mix;
            match (key.as_str(), value) {
                ("scale", toml::Value::String(name)) => style.scale = Scale::parse(name)?,
                ("root", toml::Value::String(name)) => style.root = Note::parse(name)?,
                ("bank", toml::Value::String(name)) => {
                    style.bank = Named::parse("bank", name, BANKS)?;
                }
                ("bass", toml::Value::String(name)) => style.bass = instrument(key, name)?,
                ("kit", toml::Value::String(name)) => style.kit = Kit::parse(name)?,
                ("ride_fallback", toml::Value::String(name)) => {
                    style.ride_fallback = RideFallback::parse(name)?;
                }
                ("bass_octave", value) => {
                    style.bass_octave = read_octave(key, value, BASS_OCTAVES)?;
                }
                ("melody_octave", value) => {
                    style.melody_octave = read_octave(key, value, MELODY_OCTAVES)?;
                }
                ("voicing", toml::Value::String(name)) => style.voicing = Voicing::parse(name)?,
                ("melody", toml::Value::String(name)) => style.melody = instrument(key, name)?,
                ("chord_sound", toml::Value::String(name)) => {
                    style.chord_sound = instrument(key, name)?;
                }
                ("swing", value) => {
                    style.swing = Swing(narrow(read_scaled(key, value, 100.0, 0, 100)?));
                }
                ("tempo_range", toml::Value::Array(bounds)) => {
                    style.tempo_range = read_tempo(bounds)?;
                }
                ("reverb", value) => mix.reverb = read_amount(key, value, 1)?,
                ("delay", value) => mix.delay = read_amount(key, value, 1)?,
                ("drive", value) => mix.drive = read_amount(key, value, 1)?,
                ("compress", value) => mix.compress = read_amount(key, value, 1)?,
                ("humanise", value) => mix.humanise = read_amount(key, value, 1)?,
                ("drums_gain", value) => mix.drums_gain = read_level(key, value)?,
                ("bass_gain", value) => mix.bass_gain = read_level(key, value)?,
                ("melody_gain", value) => mix.melody_gain = read_level(key, value)?,
                ("chord_gain", value) => mix.chord_gain = read_level(key, value)?,
                ("density_level", value) => mix.density_level = read_amount(key, value, 1)?,
                ("room_size", value) => {
                    mix.room_size = narrow(read_scaled(key, value, 10.0, 1, 100)?);
                }
                ("high_cut", value) => mix.high_cut = read_scaled(key, value, 1.0, 1000, 20000)?,
                (
                    "scale" | "root" | "bank" | "bass" | "melody" | "kit" | "voicing"
                    | "ride_fallback" | "chord_sound",
                    _,
                ) => {
                    return Err(wrong(key, "a string"));
                }
                ("tempo_range", _) => return Err(wrong(key, "a pair of numbers")),
                // `preset` and `synth` were applied above. Unknown keys are palette this build has
                // no opinion about yet.
                _ => {}
            }
        }
        Ok(style)
    }

    /// Reads a style header, or gives the default when a poem has none.
    ///
    /// # Errors
    ///
    /// [`StyleError`] if a header is present and cannot be read.
    pub fn parse_optional(frontmatter: Option<&str>) -> Result<Self, StyleError> {
        frontmatter.map_or_else(|| Ok(Self::default()), Self::parse)
    }

    /// Every field as a name-value pair, in a fixed order, for the debug view.
    ///
    /// Each value is written as the header would write it, so a described palette can be pasted
    /// back into a header — which is how the page's preset menu fills its controls.
    #[must_use]
    pub fn describe(&self) -> BTreeMap<&'static str, String> {
        let mix = self.mix;
        let mut fields = BTreeMap::new();
        fields.insert("scale", self.scale.name.to_owned());
        fields.insert("root", self.root.name().to_owned());
        fields.insert(
            "tempo_range",
            format!("{}..{}", self.tempo_range.low.0, self.tempo_range.high.0),
        );
        fields.insert("swing", Amount(u16::from(self.swing.0)).describe());
        fields.insert("bank", self.bank.get().to_owned());
        fields.insert("bass", self.bass.get().to_owned());
        fields.insert("melody", self.melody.get().to_owned());
        fields.insert("chord_sound", self.chord_sound.get().to_owned());
        fields.insert("kit", self.kit.name().to_owned());
        fields.insert("voicing", self.voicing.name().to_owned());
        fields.insert("ride_fallback", self.ride_fallback.name().to_owned());
        fields.insert("bass_octave", format!("{}", self.bass_octave.0));
        fields.insert("melody_octave", format!("{}", self.melody_octave.0));
        fields.insert("density_level", mix.density_level.describe());
        fields.insert("reverb", mix.reverb.describe());
        fields.insert(
            "room_size",
            format!("{}.{}", mix.room_size / 10, mix.room_size % 10),
        );
        fields.insert("delay", mix.delay.describe());
        fields.insert("high_cut", format!("{}", mix.high_cut));
        fields.insert("drive", mix.drive.describe());
        fields.insert("compress", mix.compress.describe());
        fields.insert("humanise", mix.humanise.describe());
        fields.insert("drums_gain", mix.drums_gain.describe());
        fields.insert("bass_gain", mix.bass_gain.describe());
        fields.insert("melody_gain", mix.melody_gain.describe());
        fields.insert("chord_gain", mix.chord_gain.describe());
        fields
    }
}

/// A pitched instrument: a synth name or a soundfont.
fn instrument(key: &str, name: &str) -> Result<Named, StyleError> {
    Named::parse(key, name, SYNTHS)
        .or_else(|_| Named::parse(key, name, SOUNDFONTS))
        .map_err(|_| StyleError::Unknown {
            kind: key.to_owned(),
            name: name.to_owned(),
            known: format!("{}, {}", SYNTHS.join(", "), SOUNDFONTS.join(", ")),
        })
}

/// A value already bounded to fit a `u8`.
fn narrow(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

/// Reads an amount between zero and `max`, as hundredths.
fn read_amount(key: &str, value: &toml::Value, max: u16) -> Result<Amount, StyleError> {
    read_scaled(key, value, 100.0, 0, max * 100).map(Amount)
}

/// Reads a role's level, `0.1..=2`.
///
/// Never zero, for the reason the emitter never lets a palette close a filter completely: a role
/// at zero would leave its events in the score and silent on the speaker, and an event nobody can
/// hear is one that cannot be traced to its word (invariant I4). Turning a role down is palette;
/// turning it off would be deciding which events exist, which is structure (I3).
fn read_level(key: &str, value: &toml::Value) -> Result<Amount, StyleError> {
    read_scaled(key, value, 100.0, 10, 200).map(Amount)
}

/// Reads an octave: a whole number within the role's range. Not [`read_scaled`], which rounds — an
/// octave of `2.5` is a mistake to report, not a value to round.
fn read_octave(
    key: &str,
    value: &toml::Value,
    range: std::ops::RangeInclusive<u8>,
) -> Result<Octave, StyleError> {
    value
        .as_integer()
        .and_then(|whole| u8::try_from(whole).ok())
        .filter(|octave| range.contains(octave))
        .map(Octave)
        .ok_or_else(|| StyleError::Invalid {
            key: key.to_owned(),
            expected: format!("a whole number from {} to {}", range.start(), range.end()),
        })
}

/// Reads a number, multiplies it by `scale`, rounds it once, and refuses it outside
/// `low..=high` — so the value downstream is an integer and no float ever decides anything
/// (`CLAUDE.md` §4). Integers and floats are both accepted: `swing = 0` and `swing = 0.12`.
fn read_scaled(
    key: &str,
    value: &toml::Value,
    scale: f64,
    low: u16,
    high: u16,
) -> Result<u16, StyleError> {
    let wrong = || StyleError::Invalid {
        key: key.to_owned(),
        expected: format!(
            "a number between {} and {}",
            f64::from(low) / scale,
            f64::from(high) / scale
        ),
    };
    let number = match value {
        // Exact below 2^53, and anything that large is out of range regardless.
        #[allow(clippy::cast_precision_loss)] // see above
        toml::Value::Integer(whole) => *whole as f64,
        toml::Value::Float(fraction) => *fraction,
        _ => return Err(wrong()),
    };
    let scaled = (number * scale).round();
    if !(f64::from(low)..=f64::from(high)).contains(&scaled) {
        return Err(wrong());
    }
    // Bounded by `high`, a `u16`, on the line above; NaN fails `contains`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded above
    Ok(scaled as u16)
}

/// Reads `tempo_range = [128, 134]`.
fn read_tempo(bounds: &[toml::Value]) -> Result<TempoRange, StyleError> {
    let wrong = |expected: &str| StyleError::Invalid {
        key: "tempo_range".to_owned(),
        expected: expected.to_owned(),
    };
    let [low, high] = bounds else {
        return Err(wrong("a pair of numbers"));
    };
    let (Some(low), Some(high)) = (low.as_integer(), high.as_integer()) else {
        return Err(wrong("a pair of whole numbers"));
    };
    let (Ok(low), Ok(high)) = (u16::try_from(low), u16::try_from(high)) else {
        return Err(wrong("tempos between 1 and 65535"));
    };
    if low == 0 || high == 0 {
        return Err(wrong("tempos above zero"));
    }
    if low > high {
        return Err(wrong("a range whose first value is the lower"));
    }
    Ok(TempoRange {
        low: Bpm(low),
        high: Bpm(high),
    })
}

#[cfg(test)]
mod tests {
    use super::{Kit, Note, RideFallback, Scale, Style, StyleError, Swing, Voicing};

    const HEADER: &str = "+++\n\
        [style]\n\
        scale       = \"phrygian_dominant\"\n\
        root        = \"D\"\n\
        tempo_range = [128, 134]\n\
        swing       = 0.12\n\
        bank        = \"tr909\"\n\
        synth       = \"karplus\"\n\
        +++\n";

    #[test]
    fn the_chord_lane_has_an_instrument_and_a_level_per_preset() {
        // Palette for the harmony mapping's chord lane (`SPEC.md` §4.6): a pad under the default
        // epiano, the epiano under warm's vibraphone and in jazz, a triangle in raw.
        let expect = [
            ("default", "gm_pad_warm", 40),
            ("warm", "gm_epiano1", 35),
            ("raw", "triangle", 100),
            ("jazz", "gm_epiano1", 45),
        ];
        for (name, sound, gain) in expect {
            let style = Style::preset(name).unwrap();
            assert_eq!(style.chord_sound().get(), sound, "{name}");
            assert_eq!(style.mix().chord_gain().hundredths(), gain, "{name}");
            assert!(super::preset_keys(name).unwrap().contains(&"chord_sound"));
            assert!(super::preset_keys(name).unwrap().contains(&"chord_gain"));
        }
        let style =
            Style::parse("+++\nchord_sound = \"gm_vibraphone\"\nchord_gain = 0.8\n+++").unwrap();
        assert_eq!(style.chord_sound().get(), "gm_vibraphone");
        assert_eq!(style.mix().chord_gain().hundredths(), 80);
        // `synth` sets the bass and melody, not the chords.
        let synth = Style::parse("+++\nsynth = \"fm\"\n+++").unwrap();
        assert_eq!(synth.chord_sound().get(), "gm_pad_warm");
        assert!(Style::parse("+++\nchord_sound = \"kazoo\"\n+++").is_err());
        assert!(Style::parse("+++\nchord_gain = 0\n+++").is_err());
        assert!(Style::parse("+++\nchord_sound = 3\n+++").is_err());
    }

    #[test]
    fn the_spec_example_reads() {
        // SPEC.md §5's block, verbatim.
        let style = Style::parse(HEADER).unwrap();
        assert_eq!(style.scale().name(), "phrygian_dominant");
        assert_eq!(style.root(), Note::parse("D").unwrap());
        assert_eq!(style.tempo_range().low().get(), 128);
        assert_eq!(style.tempo_range().high().get(), 134);
        assert_eq!(style.swing(), Swing(12));
        assert_eq!(style.bank().get(), "tr909");
        // `synth` is the older key: it sets both pitched instruments.
        assert_eq!(style.bass().get(), "karplus");
        assert_eq!(style.melody().get(), "karplus");
    }

    #[test]
    fn a_poem_with_no_header_still_has_a_palette() {
        // The values, not `default() == default()`. That earlier assertion could not fail, and
        // every field of the default palette survived being mutated — while `SPEC.md` §7.6 makes
        // the headerless path the *cheapest audition path*, so this is the palette most poems
        // will actually be heard through.
        let style = Style::parse_optional(None).unwrap();
        assert_eq!(style, Style::default());
        assert_eq!(style.scale().name(), "minor");
        assert_eq!(style.root(), Note::parse("C").unwrap());
        assert_eq!(style.tempo_range().low().get(), 110);
        assert_eq!(style.tempo_range().high().get(), 130);
        assert_eq!(style.swing(), Swing(0), "no swing unless asked for");
        assert_eq!(style.bank().get(), "tr909");
        // The default preset: recorded instruments, a modest room, a small human stray.
        assert_eq!(style.bass().get(), "gm_acoustic_bass");
        assert_eq!(style.melody().get(), "gm_epiano1");
        assert_eq!(style.mix().reverb().hundredths(), 25);
        assert_eq!(style.mix().room_size_tenths(), 20);
        assert_eq!(style.mix().delay().hundredths(), 15);
        assert_eq!(style.mix().high_cut(), 4400);
        assert!(style.mix().drive().is_off());
        assert_eq!(style.mix().compress().hundredths(), 30);
        assert_eq!(style.mix().humanise().hundredths(), 20);
        assert_eq!(style.mix().drums_gain().hundredths(), 28);
        assert_eq!(style.mix().bass_gain().hundredths(), 32);
        assert_eq!(style.mix().melody_gain().hundredths(), 75);
        assert_eq!(Some(style), Style::preset("default"));
    }

    #[test]
    fn the_default_scale_is_the_one_from_the_table() {
        // Not a copy of it. Two literals of the same steps could drift, and a headerless poem
        // would then play different notes from one that wrote `scale = "minor"`.
        assert_eq!(
            Style::default().scale(),
            Scale::parse("minor").unwrap(),
            "the default must reference the table, not duplicate it"
        );
    }

    #[test]
    fn describe_names_every_field() {
        // This is the provenance block in the emitted pattern and the `style` section of
        // `--explain`, so `CLAUDE.md` §8 makes it the enforcement mechanism for I4. Returning an
        // empty map silently removed the whole palette record and no test noticed.
        let described = Style::parse(HEADER).unwrap().describe();
        assert_eq!(
            described.keys().copied().collect::<Vec<_>>(),
            vec![
                "bank",
                "bass",
                "bass_gain",
                "bass_octave",
                "chord_gain",
                "chord_sound",
                "compress",
                "delay",
                "density_level",
                "drive",
                "drums_gain",
                "high_cut",
                "humanise",
                "kit",
                "melody",
                "melody_gain",
                "melody_octave",
                "reverb",
                "ride_fallback",
                "room_size",
                "root",
                "scale",
                "swing",
                "tempo_range",
                "voicing"
            ]
        );
        assert_eq!(described["scale"], "phrygian_dominant");
        assert_eq!(described["root"], "d");
        assert_eq!(described["tempo_range"], "128..134");
        assert_eq!(described["swing"], "0.12");
        assert_eq!(described["bank"], "tr909");
        assert_eq!(described["bass"], "karplus");
        assert_eq!(described["melody"], "karplus");
        assert_eq!(described["reverb"], "0.25");
        assert_eq!(described["room_size"], "2.0");
        assert_eq!(described["high_cut"], "4400");
        assert_eq!(described["drums_gain"], "0.28");
        assert_eq!(described["kit"], "machine");
        assert_eq!(described["voicing"], "note");
        assert_eq!(described["bass_octave"], "2");
        assert_eq!(described["melody_octave"], "4");
        assert_eq!(described["density_level"], "0.00");
        assert_eq!(described["ride_fallback"], "open_hat");
    }

    #[test]
    fn a_described_palette_reads_back_as_itself() {
        use std::fmt::Write as _;

        // The page fills its controls from `describe` and writes them back as a header, so the
        // two must agree on every value — a field that read back differently would play one
        // palette and show another.
        for name in super::PRESETS {
            let style = Style::preset(name).unwrap();
            let mut header = String::from("+++\n");
            for (key, value) in style.describe() {
                let value = match key {
                    "tempo_range" => format!("[{}]", value.replace("..", ", ")),
                    "scale" | "bank" | "bass" | "melody" | "kit" | "voicing" | "ride_fallback"
                    | "chord_sound" => {
                        format!("\"{value}\"")
                    }
                    "root" => format!("\"{}\"", value.replace('s', "#").to_uppercase()),
                    _ => value,
                };
                let _ = writeln!(header, "{key} = {value}");
            }
            header.push_str("+++");
            assert_eq!(Style::parse(&header).unwrap(), style, "{name}:\n{header}");
        }
    }

    #[test]
    fn a_preset_sets_the_sound_and_a_key_overrides_it() {
        let warm = Style::parse("+++\npreset = \"warm\"\n+++").unwrap();
        assert_eq!(Some(warm.clone()), Style::preset("warm"));
        assert_eq!(warm.melody().get(), "gm_vibraphone");
        assert_eq!(warm.mix().high_cut(), 3000);
        assert_eq!(warm.mix().density_level().hundredths(), 50);
        assert!(
            Style::preset("default")
                .unwrap()
                .mix()
                .density_level()
                .is_off()
                && Style::preset("raw").unwrap().mix().density_level().is_off(),
            "default and raw keep fixed levels"
        );
        // A preset never moves a note: scale, root, tempo and bank stay the default's.
        assert_eq!(warm.scale(), Style::default().scale());
        assert_eq!(warm.tempo_range(), Style::default().tempo_range());
        // Order in the header does not matter: the preset is applied first, `synth` next, and
        // the per-role keys last.
        let mixed = Style::parse(
            "+++\nmelody = \"gm_marimba\"\nreverb = 0.1\nsynth = \"sine\"\npreset = \"warm\"\n+++",
        )
        .unwrap();
        assert_eq!(mixed.melody().get(), "gm_marimba");
        assert_eq!(mixed.bass().get(), "sine");
        assert_eq!(mixed.mix().reverb().hundredths(), 10);
        assert_eq!(
            mixed.mix().room_size_tenths(),
            40,
            "the rest is the preset's"
        );
        let raw = Style::preset("raw").unwrap();
        assert_eq!(
            (raw.bass().get(), raw.melody().get()),
            ("triangle", "triangle")
        );
        assert!(raw.mix().humanise().is_off() && raw.mix().reverb().is_off());
        assert!(Style::preset("polka").is_none());
        assert!(matches!(
            Style::parse("+++\npreset = \"polka\"\n+++"),
            Err(StyleError::Unknown { .. })
        ));
    }

    #[test]
    fn the_jazz_preset_sets_a_feel_as_well_as_a_sound() {
        // Hand-written from the palette's own description: Rhodes sevenths over an upright, the
        // ride and snare of the R-8, triplet swing, dorian.
        let jazz = Style::preset("jazz").unwrap();
        assert_eq!(jazz.melody().get(), "gm_epiano1");
        assert_eq!(jazz.bass().get(), "gm_acoustic_bass");
        assert_eq!(jazz.kit(), Kit::Ride);
        assert_eq!(jazz.voicing(), Voicing::Seventh);
        assert_eq!(jazz.bank().get(), "r8");
        assert_eq!(jazz.swing(), Swing(33));
        assert_eq!(jazz.scale().name(), "dorian");
        assert_eq!(jazz.mix().density_level().hundredths(), 50);
        assert_eq!(jazz.scale().steps(), &[0, 2, 3, 5, 7, 9, 10]);
        // Root and tempo window are still the default's: the poem's key and pace are not jazz's.
        assert_eq!(jazz.root(), Style::default().root());
        assert_eq!(jazz.tempo_range(), Style::default().tempo_range());
        // Every key a header names still overrides the preset, its feel included.
        let modal = Style::parse(
            "+++
preset = \"jazz\"\nscale = \"mixolydian\"\nswing = 0\n+++",
        )
        .unwrap();
        assert_eq!(modal.scale().name(), "mixolydian");
        assert_eq!(modal.swing(), Swing(0));
        assert_eq!(
            modal.voicing(),
            Voicing::Seventh,
            "the rest is the preset's"
        );
        // The page applies a preset's own keys and no others.
        let keys = super::preset_keys("jazz").unwrap();
        for key in [
            "scale",
            "swing",
            "bank",
            "kit",
            "voicing",
            "melody",
            "melody_gain",
        ] {
            assert!(keys.contains(&key), "{key}");
        }
        assert!(!keys.contains(&"root") && !keys.contains(&"tempo_range"));
        let warm = super::preset_keys("warm").unwrap();
        assert!(!warm.contains(&"scale") && !warm.contains(&"bank") && warm.contains(&"kit"));
        assert!(super::preset_keys("polka").is_none());
    }

    #[test]
    fn a_kit_and_a_voicing_are_named_values() {
        let style = Style::parse("+++\nkit = \"ride\"\nvoicing = \"seventh\"\n+++").unwrap();
        assert_eq!(
            (style.kit(), style.voicing()),
            (Kit::Ride, Voicing::Seventh)
        );
        assert_eq!(Voicing::Note.degrees(), &[0]);
        assert_eq!(
            Voicing::Seventh.degrees(),
            &[0, 2, 4, 6],
            "root, third, fifth, seventh"
        );
        for bad in [
            "kit = \"brushes\"",
            "voicing = \"ninth\"",
            "kit = 1",
            "voicing = true",
        ] {
            assert!(Style::parse(&format!("+++\n{bad}\n+++")).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_register_the_fallback_and_the_density_level_are_read_to_their_ranges() {
        // Today's register, by default: octave 2 is MIDI 36 and octave 4 is MIDI 60.
        let plain = Style::default();
        assert_eq!(plain.bass_octave().get(), 2);
        assert_eq!(plain.bass_octave().midi_c(), 36);
        assert_eq!(plain.melody_octave().get(), 4);
        assert_eq!(plain.melody_octave().midi_c(), 60);
        assert_eq!(plain.ride_fallback(), RideFallback::OpenHat);
        assert!(plain.mix().density_level().is_off());
        let set = Style::parse(
            "+++\nbass_octave = 1\nmelody_octave = 5\nride_fallback = \"crash\"\n\
             density_level = 0.5\n+++",
        )
        .unwrap();
        // Hand-worked: 12 · (1 + 1) = 24, 12 · (5 + 1) = 72.
        assert_eq!(set.bass_octave().midi_c(), 24);
        assert_eq!(set.melody_octave().midi_c(), 72);
        assert_eq!(set.ride_fallback(), RideFallback::Crash);
        assert_eq!(set.mix().density_level().hundredths(), 50);
        for bad in [
            "bass_octave = 0",
            "bass_octave = 4",
            "bass_octave = 2.5",
            "bass_octave = \"2\"",
            "melody_octave = 2",
            "melody_octave = 6",
            "ride_fallback = \"cowbell\"",
            "ride_fallback = 1",
            "density_level = 1.5",
            "density_level = -0.1",
        ] {
            assert!(Style::parse(&format!("+++\n{bad}\n+++")).is_err(), "{bad}");
        }
        // A preset restores today's register and stand-in, like every other sound key.
        let keys = super::preset_keys("warm").unwrap();
        for key in [
            "bass_octave",
            "melody_octave",
            "ride_fallback",
            "density_level",
        ] {
            assert!(keys.contains(&key), "{key}");
        }
        let after = Style::parse("+++\nbass_octave = 3\n+++").unwrap();
        assert_eq!(after.bass_octave().get(), 3);
        let overridden = Style::parse("+++\nbass_octave = 3\npreset = \"raw\"\n+++").unwrap();
        assert_eq!(
            overridden.bass_octave().get(),
            3,
            "a key overrides the preset in any order"
        );
    }

    #[test]
    fn mix_values_are_read_to_their_ranges() {
        // Hand-worked: each in header units, stored as integers.
        let style = Style::parse(
            "+++\nreverb = 0.5\nroom_size = 3.5\ndelay = 0\nhigh_cut = 2500\n\
             humanise = 1\nmelody_gain = 1.25\n+++",
        )
        .unwrap();
        assert_eq!(style.mix().reverb().hundredths(), 50);
        assert_eq!(style.mix().room_size_tenths(), 35);
        assert!(style.mix().delay().is_off());
        assert_eq!(style.mix().high_cut(), 2500);
        assert_eq!(style.mix().humanise().hundredths(), 100);
        assert_eq!(style.mix().melody_gain().hundredths(), 125);
        for bad in [
            "reverb = 1.5",
            "reverb = -0.1",
            "humanise = \"lots\"",
            "room_size = 0",
            "room_size = 11",
            "high_cut = 999",
            "high_cut = 20001",
            "drums_gain = 2.5",
            "melody_gain = 0",
            "bass_gain = 0.05",
            "bass = \"gm_nonesuch\"",
            "melody = 3",
        ] {
            assert!(Style::parse(&format!("+++\n{bad}\n+++")).is_err(), "{bad}");
        }
        // A soundfont or a synth name, for either role.
        assert!(Style::parse("+++\nbass = \"sine\"\nmelody = \"gm_kalimba\"\n+++").is_ok());
    }

    #[test]
    fn a_bank_or_synth_nobody_can_play_is_refused() {
        // Two reasons, and the second is the serious one: an unknown name reaches the backend and
        // produces silence, and a free string is written straight into generated JavaScript.
        assert!(matches!(
            Style::parse("+++\nbank = \"nonesuch\"\n+++"),
            Err(StyleError::Unknown { .. })
        ));
        assert!(matches!(
            Style::parse("+++\nsynth = \"nonesuch\"\n+++"),
            Err(StyleError::Unknown { .. })
        ));
        // The injection that rewrote every voice's arrangement.
        assert!(
            Style::parse("+++\nbank = \"a\\\").fast(4).s(\\\"bd\"\n+++").is_err(),
            "a style header must not be able to alter the arrangement"
        );
    }

    #[test]
    fn note_names_are_semitones_from_c() {
        // Hand-worked from the chromatic scale.
        assert_eq!(Note::parse("C").unwrap().semitones(), 0);
        assert_eq!(Note::parse("D").unwrap().semitones(), 2);
        assert_eq!(Note::parse("F#").unwrap().semitones(), 6);
        assert_eq!(Note::parse("Gb").unwrap().semitones(), 6, "enharmonic");
        assert_eq!(Note::parse("B").unwrap().semitones(), 11);
        assert_eq!(Note::parse("Cb").unwrap().semitones(), 11, "wraps down");
        assert!(Note::parse("H").is_err());
        assert!(Note::parse("C##").is_err());
        assert!(Note::parse("").is_err());
    }

    #[test]
    fn degrees_climb_into_higher_octaves() {
        // Phrygian dominant: 0 1 4 5 7 8 10. A poem with eight rhyme classes needs an eighth note,
        // and folding it back onto degree 0 would make two distinct rhymes sound identical.
        let scale = Scale::parse("phrygian_dominant").unwrap();
        assert_eq!(scale.degree(0), 0);
        assert_eq!(scale.degree(2), 4);
        assert_eq!(scale.degree(6), 10);
        assert_eq!(scale.degree(7), 12, "an octave up, not back to the root");
        assert_eq!(scale.degree(9), 16);
    }

    #[test]
    fn the_poem_picks_the_tempo_inside_the_styles_window() {
        let style = Style::parse(HEADER).unwrap();
        let range = style.tempo_range();
        // Hand-worked: span 6, DENSE 12. Six syllables a bar is halfway, so 128 + 6·6/12 = 131.
        assert_eq!(range.at_density(60, 10).get(), 131);
        // A sparse poem sits at the slow end, a dense one at the fast end, and nothing leaves the
        // window — which is what makes the range a constraint rather than a suggestion.
        assert_eq!(range.at_density(0, 10).get(), 128);
        assert_eq!(range.at_density(1000, 10).get(), 134);
        assert_eq!(range.at_density(0, 0).get(), 128, "a poem with no bars");
    }

    #[test]
    fn a_malformed_header_is_an_error_not_a_panic() {
        // CLAUDE.md §6: never panic on user input.
        assert!(matches!(
            Style::parse("+++\nthis is not toml\n+++"),
            Err(StyleError::Malformed(_))
        ));
        assert!(matches!(
            Style::parse("+++\nscale = 3\n+++"),
            Err(StyleError::Invalid { .. })
        ));
        assert!(matches!(
            Style::parse("+++\nscale = \"klingon\"\n+++"),
            Err(StyleError::Unknown { .. })
        ));
        assert!(matches!(
            Style::parse("+++\ntempo_range = [134, 128]\n+++"),
            Err(StyleError::Invalid { .. })
        ));
        assert!(matches!(
            Style::parse("+++\ntempo_range = [0, 10]\n+++"),
            Err(StyleError::Invalid { .. })
        ));
        assert!(matches!(
            Style::parse("+++\nswing = 2.0\n+++"),
            Err(StyleError::Invalid { .. })
        ));
    }

    #[test]
    fn an_unknown_key_is_palette_we_have_no_opinion_about() {
        // A header written for a later version must still play, or every style change breaks
        // every existing poem.
        let style = Style::parse("+++\nreverb = 0.4\nscale = \"dorian\"\n+++").unwrap();
        assert_eq!(style.scale().name(), "dorian");
    }

    #[test]
    fn the_section_header_is_optional() {
        let nested = Style::parse("+++\n[style]\nscale = \"lydian\"\n+++").unwrap();
        let bare = Style::parse("+++\nscale = \"lydian\"\n+++").unwrap();
        assert_eq!(nested, bare);
    }

    #[test]
    fn every_named_scale_parses_and_has_distinct_degrees() {
        for scale in Scale::ALL {
            let found = Scale::parse(scale.name()).unwrap();
            assert_eq!(found, *scale);
            assert!(!found.is_empty(), "{}", scale.name());
            let mut steps = found.steps().to_vec();
            steps.sort_unstable();
            steps.dedup();
            assert_eq!(steps.len(), found.len(), "{} repeats a step", scale.name());
            assert!(found.steps().iter().all(|s| *s < 12), "{}", scale.name());
        }
    }
}
