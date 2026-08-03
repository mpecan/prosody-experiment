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
pub const BANKS: &[&str] = &["tr909", "tr808", "tr707", "linn", "casio"];

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
    synth: Named,
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

impl Default for Style {
    /// The palette a poem with no header gets.
    ///
    /// A poem is a complete score on its own; the header only recolours it. Defaulting rather than
    /// refusing is what makes that true — and `SPEC.md` §7.6 wants the cheapest possible audition
    /// path, which a mandatory header would not be.
    fn default() -> Self {
        Self {
            scale: Scale::MINOR,
            root: Note(0),
            tempo_range: TempoRange {
                low: Bpm(110),
                high: Bpm(130),
            },
            swing: Swing(0),
            bank: Named { name: "tr909" },
            synth: Named { name: "triangle" },
        }
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

    /// The synth pitched voices play.
    #[must_use]
    pub const fn synth(&self) -> Named {
        self.synth
    }

    /// Reads a style header — the whole `+++` block, delimiters and all.
    ///
    /// Every value is validated here and nowhere else (`CLAUDE.md` §5). Unknown keys are ignored
    /// rather than refused, so a header written for a later version still plays.
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

        for (key, value) in table {
            match (key.as_str(), value) {
                ("scale", toml::Value::String(name)) => style.scale = Scale::parse(name)?,
                ("root", toml::Value::String(name)) => style.root = Note::parse(name)?,
                ("bank", toml::Value::String(name)) => {
                    style.bank = Named::parse("bank", name, BANKS)?;
                }
                ("synth", toml::Value::String(name)) => {
                    style.synth = Named::parse("synth", name, SYNTHS)?;
                }
                ("swing", value) => style.swing = Swing(read_swing(value)?),
                ("tempo_range", toml::Value::Array(bounds)) => {
                    style.tempo_range = read_tempo(bounds)?;
                }
                ("scale" | "root" | "bank" | "synth", _) => return Err(wrong(key, "a string")),
                ("tempo_range", _) => return Err(wrong(key, "a pair of numbers")),
                // Unknown keys are palette this build has no opinion about yet.
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
    #[must_use]
    pub fn describe(&self) -> BTreeMap<&'static str, String> {
        let mut fields = BTreeMap::new();
        fields.insert("scale", self.scale.name.to_owned());
        fields.insert("root", self.root.name().to_owned());
        fields.insert(
            "tempo_range",
            format!("{}..{}", self.tempo_range.low.0, self.tempo_range.high.0),
        );
        fields.insert("swing", format!("{}", self.swing.0));
        fields.insert("bank", self.bank.get().to_owned());
        fields.insert("synth", self.synth.get().to_owned());
        fields
    }
}

/// Reads `swing = 0.12` or `swing = 0`, as hundredths.
fn read_swing(value: &toml::Value) -> Result<u8, StyleError> {
    let wrong = || StyleError::Invalid {
        key: "swing".to_owned(),
        expected: "a number between 0 and 1".to_owned(),
    };
    let hundredths = match value {
        toml::Value::Integer(0) => 0,
        toml::Value::Integer(1) => 100,
        toml::Value::Float(fraction) => {
            if !(0.0..=1.0).contains(fraction) {
                return Err(wrong());
            }
            // Rounded once, here, so the value downstream is an integer and no float ever decides
            // anything (`CLAUDE.md` §4).
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded 0..=100
            {
                (fraction * 100.0).round() as u8
            }
        }
        _ => return Err(wrong()),
    };
    Ok(hundredths)
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
    use super::{Note, Scale, Style, StyleError, Swing};

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
    fn the_spec_example_reads() {
        // SPEC.md §5's block, verbatim.
        let style = Style::parse(HEADER).unwrap();
        assert_eq!(style.scale().name(), "phrygian_dominant");
        assert_eq!(style.root(), Note::parse("D").unwrap());
        assert_eq!(style.tempo_range().low().get(), 128);
        assert_eq!(style.tempo_range().high().get(), 134);
        assert_eq!(style.swing(), Swing(12));
        assert_eq!(style.bank().get(), "tr909");
        assert_eq!(style.synth().get(), "karplus");
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
        assert_eq!(style.synth().get(), "triangle");
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
            vec!["bank", "root", "scale", "swing", "synth", "tempo_range"]
        );
        assert_eq!(described["scale"], "phrygian_dominant");
        assert_eq!(described["root"], "d");
        assert_eq!(described["tempo_range"], "128..134");
        assert_eq!(described["swing"], "12");
        assert_eq!(described["bank"], "tr909");
        assert_eq!(described["synth"], "karplus");
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
