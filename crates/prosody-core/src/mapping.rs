//! Choosing between candidate mappings, to hear what each does.
//!
//! Some mappings are still candidates: added by a design iteration and marked *pending review* in
//! `SPEC.md`, they change what a poem becomes, and whether they belong is judged by listening. Each
//! is a *selector*: a [`Mapping`] has a fixed, ordered list of [`Variant`]s — always including
//! a first variant that is what preceded the mapping ([`Variant::Off`] for every mapping but the
//! pulse, which cannot be left out and goes back to [`Variant::Elastic`], the stack, whose
//! lines always sound somehow and which goes back to [`Variant::Crossing`], and the function
//! words, which are always struck somehow and go back to [`Variant::Full`], the long stanzas,
//! whose arrivals always take the loop's place somehow and which go back to
//! [`Variant::Gradual`], the drums, which always play a piece of the kit and go back to
//! [`Variant::Flat`], and the rhyme, which always rhymes line endings somehow and goes back to
//! [`Variant::Strict`]) — and one of them is
//! its default. A [`Mappings`] value picks one variant per
//! mapping, so the same poem can be heard under each choice, side by side.
//!
//! This is structure, not palette, so it is deliberately **not** part of the style (`SPEC.md` §6,
//! I3: the style may change only parameters). It is a separate argument to
//! [`lower::run_with`](crate::lower::run_with) (and to the emitter's reading plan), and every
//! invariant holds within one choice: the same poem under the same choices is bit-identical, and
//! an edit to one stanza leaves the others' voices alone. [`Mappings::all`] is every mapping at
//! its default — the piece as specified, and the default everywhere.
//!
//! A variant a mapping does not have cannot be stored: a [`Mappings`] is changed only by a
//! [`Choice`], and a [`Choice`] is made only by [`Mapping::choose`] or [`Mapping::choose_named`],
//! which refuse a variant outside that mapping's list.

use std::collections::BTreeMap;

/// One candidate mapping, chosen between by its [`Variant`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mapping {
    /// §4.6's held lane: a line whose rhyme is answered later in its stanza sounds until it is.
    HeldLane,
    /// §4.6's refrain lane: a refrain line loops until the next refrain line takes over.
    RefrainLane,
    /// §4.6's bass: a stressed foundation syllable pitched by its vowel's height
    /// ([`Variant::Height`], the default), or a walking bass — on every syllable
    /// ([`Variant::Syllables`], W1) or a note a foot on the beat ([`Variant::Feet`], W2). See
    /// `crate::bass`.
    Bass,
    /// §4.6's bar lock to the stanza: a bar lasts its stanza's longest line. Off, a bar lasts its
    /// own line, and whatever sounds under it is stretched or squeezed to fit.
    StanzaPeriod,
    /// §4.6's outro: the loops play one more pass when the poem runs out.
    Outro,
    /// §4.6's harmony layer: a chord per bar of a foundation stanza, from its rhyme class
    /// ([`Variant::Rhyme`], H1), from rhyme closure ([`Variant::Cadence`], H2), or both
    /// ([`Variant::Both`]). Off by default: the proposal is a candidate (`crate::harmony`).
    Harmony,
    /// §4.7's reading: when each line of the poem is spoken over the music. Decided in the
    /// emitter (`prosody_emit::reading`), not in the lowering, which never reads it.
    Reading,
    /// §4.2's pulse: how long a bar of the Strudel pattern lasts. [`Variant::Steady`] (the
    /// default): as long as its beats, so the beat is one speed throughout. [`Variant::Elastic`]:
    /// every bar the same span, so a bar of more beats plays them faster — the pattern as it was.
    /// When events fall is structure, not palette (I3). The lowering records it on the score and
    /// lays out nothing by it: the backends read it (`prosody_emit::strudel`).
    Pulse,
    /// §4.2 and §4.6: how a line stacked under the bar's host — held, refrain, call, a loop
    /// yielding or carrying on — divides a beat. [`Variant::Crossing`] (the default): by its own
    /// feet, so a triplet can sound against the host's pair. [`Variant::Straight`]: fitted to the
    /// host's subdivision of that beat wherever the two would cross (`crate::stack`). When events
    /// fall is structure, not palette (I3); the score records it and [`Score::placed_in`]
    /// applies it, so every backend and the scorer read the same onsets.
    ///
    /// [`Score::placed_in`]: crate::voice::Score::placed_in
    Stack,
    /// §4.2's "stress → accent" for a one-syllable function word ("or", "thy", "what", "the",
    /// "and", …), which the dictionary gives full stress. [`Variant::Full`] (the default): struck
    /// at its dictionary stress, like any word. [`Variant::Light`]: struck lightly — secondary
    /// force — unless the meter's scansion puts it on a strong position (`crate::accent`). Which
    /// words are function words is the lexicon's to say
    /// ([`Lexicon::is_function_word`](crate::surprisal::Lexicon::is_function_word)). How hard an
    /// event is struck is structure, not palette (I3): it decides which syllables are accents,
    /// and so where the bass plays and what the scorer counts as syncopation.
    Function,
    /// §4.6's long stanzas: what happens when a stanza arrives into a running loop and either
    /// stanza is long — more than [`LONG_STANZA`](crate::arrange::LONG_STANZA) lines.
    /// [`Variant::Erode`] (the default): the old loop keeps looping under the arriving stanza and
    /// thins out over [`EROSION_PASSES`](crate::arrange::EROSION_PASSES) passes until it is silent
    /// (`crate::arrange`). [`Variant::Gradual`]: the old loop is replaced line by line, as it is
    /// for short stanzas under either variant — which made a stanza of *n* lines take about *n*
    /// passes of *n* bars. Stanzas of eight lines or fewer arrange the same under both.
    LongStanzas,
    /// §4.1 and §4.2's drum kit articulation: which piece of the drum kit each syllable plays.
    /// [`Variant::Flat`] (the default): one piece per layer — the pulse (`bd` / `rd`) on every
    /// foundation syllable and the ornament (`hh` / `sd`) on every flourish syllable, as the
    /// piece shipped. [`Variant::Articulated`]: each syllable's piece chosen from its layer, beat
    /// parity (`beat % 2`, downbeat against backbeat) and stress within the foot (`crate::drums`).
    Drums,
    /// §4.5's rhyme scoring: how line endings are rhymed. [`Variant::Strict`] (the default):
    /// reckoned strictly from the last stressed vowel, masculine and standard feminine rhymes
    /// only. [`Variant::Slant`]: candidate tails across the last stress and final syllable rime,
    /// with apocopated endings and continuous coda slant scoring down to 0.5.
    Rhyme,
    /// §4.3's repetition anchors: how line-initial anaphora and line-final epistrophe figures are
    /// rendered. [`Variant::Off`] (the default): unrendered in Strudel. [`Variant::On`]:
    /// downbeat-anchored anaphora ostinatos accented with punchy transients, and epistrophe
    /// turnaround cadences ringing through the bar.
    Anchor,
}

/// One way a [`Mapping`] can go. Which variants a mapping has is [`Mapping::variants`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Variant {
    /// The mapping is not applied: what preceded it.
    Off,
    /// The mapping is applied — for a mapping that is simply on or off.
    On,
    /// The reading's line-by-line variant: each line spoken in the slot where it first arrives.
    Arrives,
    /// The reading's stanza variant: a stanza's lines spoken one a bar, in order, from the
    /// stanza's first hosting slot.
    Stanza,
    /// The reading's default: each stanza read through, one line a bar, from where its first line
    /// is first heard or one silent bar after the previous stanza's reading, whichever is later.
    Breath,
    /// The harmony's H1: each line's rhyme class is its bar's chord, on the class's degree.
    Rhyme,
    /// The harmony's H2: an open rhyme is the dominant of its degree, its answer the tonic, a
    /// line with no rhyme in its stanza the subdominant, and the stanza's last bar a turnaround.
    Cadence,
    /// The harmony's H1 and H2 together: H2 where it applies (open and closing lines, the
    /// turnaround), H1 in every other bar.
    Both,
    /// The bass by vowel height: every primary-stressed foundation syllable not in a rhyme word,
    /// open root, mid third, close fifth — the bass as it shipped (*machine cycle 1*).
    Height,
    /// The bass's W1: every syllable of a foundation line walks — the stressed ones on their
    /// height note, the ones between by scale step, the last before each a chromatic approach.
    Syllables,
    /// The bass's W2: a note a foot, on the beat — the foot's vowel height chooses the chord tone,
    /// and the next foot's the direction of a step where the tone would repeat.
    Feet,
    /// The pulse's default: a bar lasts as many beats as it holds, so every beat of the piece
    /// lasts the same time.
    Steady,
    /// The pulse as it was: every bar lasts the same time, so the beat stretches in a short bar
    /// and hurries in a long one.
    Elastic,
    /// The stack as it was, and its default: a stacked line divides each beat by its own foot,
    /// whatever the host does — three against two where their feet differ.
    Crossing,
    /// The stack straightened: where a stacked line's beat would cross its host's, its syllables
    /// are fitted to the host's subdivision of that beat, each still its own event.
    Straight,
    /// The function words as they were, and their default: every syllable struck at its
    /// dictionary stress, so "or" in "hand or eye" is as loud as "hand".
    Full,
    /// The function words lightened: a one-syllable function word off the meter's strong
    /// positions is struck at secondary force, so it no longer reads as an accent.
    Light,
    /// The long stanzas as they were: an arriving stanza replaces the running loop one line a
    /// pass, however long either stanza is.
    Gradual,
    /// The long stanzas' default: where either stanza is long, the running loop erodes under the
    /// arriving stanza — fewer of its syllables each pass, until it is silent.
    Erode,
    /// The drums as they were, and their default: one drum piece per layer on every syllable.
    Flat,
    /// The drums articulated across the kit by layer, beat parity and stress within the foot.
    Articulated,
    /// The rhyme pass as it was, and its default: from the last stressed vowel only.
    Strict,
    /// The rhyme pass with feminine and slant scoring: candidate tails and continuous coda slant
    /// matches.
    Slant,
}

impl Variant {
    /// Its name on a command line or in a query string.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Arrives => "arrives",
            Self::Stanza => "stanza",
            Self::Breath => "breath",
            Self::Rhyme => "rhyme",
            Self::Cadence => "cadence",
            Self::Both => "both",
            Self::Height => "height",
            Self::Syllables => "syllables",
            Self::Feet => "feet",
            Self::Steady => "steady",
            Self::Elastic => "elastic",
            Self::Crossing => "crossing",
            Self::Straight => "straight",
            Self::Full => "full",
            Self::Light => "light",
            Self::Gradual => "gradual",
            Self::Erode => "erode",
            Self::Flat => "flat",
            Self::Articulated => "articulated",
            Self::Strict => "strict",
            Self::Slant => "slant",
        }
    }
}

/// The variants of a mapping that is simply on or off.
const SWITCH: &[Variant] = &[Variant::Off, Variant::On];

/// The reading's variants: off, line by line where each arrives, a stanza at a time, or a stanza
/// at a time with a silent bar between stanzas and never before the music begins one.
const READING: &[Variant] = &[
    Variant::Off,
    Variant::Arrives,
    Variant::Stanza,
    Variant::Breath,
];

/// The harmony's variants: none, the rhyme scheme as harmonic rhythm (H1), rhyme closure as
/// tension and release (H2), or H2 where it applies and H1 elsewhere.
const HARMONY: &[Variant] = &[
    Variant::Off,
    Variant::Rhyme,
    Variant::Cadence,
    Variant::Both,
];

/// The bass's variants: none, by vowel height (today's), walking every syllable (W1), or walking
/// a note a foot (W2).
const BASS: &[Variant] = &[
    Variant::Off,
    Variant::Height,
    Variant::Syllables,
    Variant::Feet,
];

/// The pulse's variants: elastic first, because it is what preceded the mapping — a piece cannot
/// be without a pulse, so the pulse has no [`Variant::Off`] — then steady.
const PULSE: &[Variant] = &[Variant::Elastic, Variant::Steady];

/// The stack's variants: crossing first, because it is what preceded the mapping — a stacked line
/// always divides its beats somehow, so the stack has no [`Variant::Off`] — then straight.
const STACK: &[Variant] = &[Variant::Crossing, Variant::Straight];

/// The function words' variants: full first, because it is what preceded the mapping — a word is
/// always struck somehow, so there is no [`Variant::Off`] — then light.
const FUNCTION: &[Variant] = &[Variant::Full, Variant::Light];

/// The long stanzas' variants: gradual first, because it is what preceded the mapping — a stanza
/// arriving into a loop always takes its place somehow, so there is no [`Variant::Off`] — then
/// erode.
const LONG_STANZAS: &[Variant] = &[Variant::Gradual, Variant::Erode];

/// The drums' variants: flat first, because it is what preceded the mapping — every syllable
/// always plays a piece of the kit, so there is no [`Variant::Off`] — then articulated.
const DRUMS: &[Variant] = &[Variant::Flat, Variant::Articulated];

/// The rhyme's variants: strict first, because it is what preceded the mapping — line endings
/// always rhyme somehow, so there is no [`Variant::Off`] — then slant.
const RHYME: &[Variant] = &[Variant::Strict, Variant::Slant];

impl Mapping {
    /// Every mapping, in the order a page lists them.
    pub const ALL: [Self; 14] = [
        Self::HeldLane,
        Self::RefrainLane,
        Self::Bass,
        Self::StanzaPeriod,
        Self::Outro,
        Self::Harmony,
        Self::Reading,
        Self::Pulse,
        Self::Stack,
        Self::Function,
        Self::LongStanzas,
        Self::Drums,
        Self::Rhyme,
        Self::Anchor,
    ];

    /// Its name on a command line or in a query string.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HeldLane => "held",
            Self::RefrainLane => "refrain",
            Self::Bass => "bass",
            Self::StanzaPeriod => "period",
            Self::Outro => "outro",
            Self::Harmony => "harmony",
            Self::Reading => "reading",
            Self::Pulse => "pulse",
            Self::Stack => "stack",
            Self::Function => "function-words",
            Self::LongStanzas => "long-stanzas",
            Self::Drums => "drums",
            Self::Rhyme => "rhyme",
            Self::Anchor => "anchor",
        }
    }

    /// The mapping called `name`, if there is one.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mapping| mapping.name() == name)
    }

    /// What it does, in a phrase.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::HeldLane => "an open rhyme sounds until its answer",
            Self::RefrainLane => "a refrain line loops until the next one",
            Self::Bass => {
                "a foundation bass: stressed vowels by height (height), walking every syllable \
                 with passing and approach tones (syllables), or a note a foot on the beat from \
                 the chord (feet)"
            }
            Self::StanzaPeriod => "every bar of a stanza lasts its longest line",
            Self::Outro => "the loops play one more pass at the end",
            Self::Harmony => {
                "a chord per foundation bar: its rhyme class (rhyme), open rhymes as V and \
                 answers as I (cadence), or cadence where it applies and rhyme elsewhere (both)"
            }
            Self::Reading => {
                "the poem read aloud: each line where it arrives (arrives), a stanza a line a bar \
                 from its first arrival (stanza), or a stanza a line a bar, a silent bar after the \
                 last and never before the music begins it (breath)"
            }
            Self::Pulse => {
                "how long a bar lasts: as long as its beats, one beat speed throughout (steady), \
                 or every bar the same, so a longer bar hurries its beats (elastic)"
            }
            Self::Stack => {
                "a line stacked under the bar's line: its own subdivision of each beat, three \
                 against two where the feet differ (crossing), or fitted to the bar's line's \
                 subdivision wherever they would cross (straight)"
            }
            Self::Function => {
                "a one-syllable function word (or, thy, the, and, …): struck at its dictionary \
                 stress like any word (full), or lightly unless the metre puts it on a strong \
                 position (light)"
            }
            Self::LongStanzas => {
                "a stanza arriving into a loop where either has more than eight lines: the old \
                 loop thins out under it over three passes until silent (erode), or is replaced \
                 a line a pass (gradual); shorter stanzas are replaced a line a pass under both"
            }
            Self::Drums => {
                "which drum piece each syllable plays: one piece per layer (flat), or articulated \
                 across kick/ride, snare, toms and closed hat by layer, beat parity and stress \
                 within the foot (articulated)"
            }
            Self::Rhyme => {
                "how line endings are rhymed: from the last stressed vowel only (strict), or with \
                 apocopated endings and continuous coda slant scoring across candidate tails (slant)"
            }
            Self::Anchor => {
                "how repetition figures anchor in Strudel: unrendered (off), or emphasizing \
                 line-initial anaphora on the downbeat and ringing out line-final epistrophe cadences (on)"
            }
        }
    }

    /// Its variants, in a fixed order that starts with what preceded the mapping:
    /// [`Variant::Off`], except for the pulse, which starts with [`Variant::Elastic`], the
    /// stack, which starts with [`Variant::Crossing`], the function words, which start with
    /// [`Variant::Full`], the long stanzas, which start with [`Variant::Gradual`], the drums,
    /// which start with [`Variant::Flat`], and the rhyme, which starts with [`Variant::Strict`].
    #[must_use]
    pub const fn variants(self) -> &'static [Variant] {
        match self {
            Self::HeldLane
            | Self::RefrainLane
            | Self::StanzaPeriod
            | Self::Outro
            | Self::Anchor => SWITCH,
            Self::Bass => BASS,
            Self::Reading => READING,
            Self::Harmony => HARMONY,
            Self::Pulse => PULSE,
            Self::Stack => STACK,
            Self::Function => FUNCTION,
            Self::LongStanzas => LONG_STANZAS,
            Self::Drums => DRUMS,
            Self::Rhyme => RHYME,
        }
    }

    /// The variant it takes unless told otherwise: the piece as specified.
    #[must_use]
    pub const fn default_variant(self) -> Variant {
        match self {
            Self::HeldLane | Self::RefrainLane | Self::StanzaPeriod | Self::Outro => Variant::On,
            // The bass as it shipped. The walking variants are candidates the user approved for
            // comparison (`PLAN.md`, the walking-bass proposal), so the default piece keeps it.
            Self::Bass => Variant::Height,
            // The user decided (PLAN.md, "the reading takes a breath"): a stanza is read whole, a
            // line a bar, with a silent bar before the next and never before the music has begun
            // it (SPEC.md §4.7). `arrives` — the reading as it shipped — and `stanza` stay as
            // variants to hear against it.
            Self::Reading => Variant::Breath,
            // Candidates the user approved for comparison, not yet the piece as specified: off
            // keeps every existing pattern and score as it was (`PLAN.md`: the harmony proposal,
            // and the repetition anchor proposal in Strudel).
            Self::Harmony | Self::Anchor => Variant::Off,
            // The user decided (PLAN.md, "a steady beat"): a bar lasts as long as its beats, so
            // the beat is one speed throughout — SPEC.md §4.2's "foot → beat" taken at its word.
            // Elastic stays as a variant to compare against.
            Self::Pulse => Variant::Steady,
            // The user decided (PLAN.md, "straightening stacked lines"): today's crossing stays
            // the piece, and straight is a variant to hear against it. SPEC.md §4.2's "syllable →
            // subdivision within the foot" is what crosses; straight bends it only under a host.
            Self::Stack => Variant::Crossing,
            // The user decided (PLAN.md, "softening function words"): full stays the piece, and
            // light is a variant to hear against it. SPEC.md §4.2 maps stress to accent, and the
            // dictionary's stress on "or" is lexical, not the line's; light lets the meter's
            // scansion overrule it where the meter calls the position weak.
            Self::Function => Variant::Full,
            // The user decided (PLAN.md, "erosion for long stanzas"): a long stanza's arrival
            // erodes the old loop instead of replacing it a line a pass, which grew the piece
            // with the square of stanza length (Lowell's "Patterns": 1,331 bars). Only above the
            // threshold, so every shorter stanza is arranged as before (SPEC.md §4.6).
            Self::LongStanzas => Variant::Erode,
            // A candidate the user approved for comparison: flat keeps every existing pattern as
            // it was, while articulated distributes syllables across kick/ride, snare, toms and
            // closed hat by beat parity and stress within the foot (`PLAN.md`).
            Self::Drums => Variant::Flat,
            // A candidate the user approved for comparison: strict keeps every existing pattern
            // and score as it was, while slant scores weak endings and shared codas across
            // candidate tails (`PLAN.md`).
            Self::Rhyme => Variant::Strict,
        }
    }

    /// This mapping set to `variant`, if it is one of its variants.
    #[must_use]
    pub fn choose(self, variant: Variant) -> Option<Choice> {
        self.variants().contains(&variant).then_some(Choice {
            mapping: self,
            variant,
        })
    }

    /// This mapping set to the variant called `name`, if it has one.
    #[must_use]
    pub fn choose_named(self, name: &str) -> Option<Choice> {
        self.variants()
            .iter()
            .find(|variant| variant.name() == name)
            .and_then(|variant| self.choose(*variant))
    }

    /// This mapping left out: its first variant, what preceded it — [`Variant::Off`] for every
    /// mapping but the pulse, which a piece cannot be without, and which goes back to
    /// [`Variant::Elastic`], the stack, which goes back to [`Variant::Crossing`], the function
    /// words, which go back to [`Variant::Full`], the long stanzas, which go back to
    /// [`Variant::Gradual`], the drums, which go back to [`Variant::Flat`], and the rhyme,
    /// which goes back to [`Variant::Strict`].
    #[must_use]
    pub const fn off(self) -> Choice {
        let variant = match self.variants() {
            [first, ..] => *first,
            [] => Variant::Off,
        };
        Choice {
            mapping: self,
            variant,
        }
    }
}

/// A mapping and one of *its* variants. Made only by [`Mapping::choose`],
/// [`Mapping::choose_named`] or [`Mapping::off`], so it cannot name a variant its mapping lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Choice {
    mapping: Mapping,
    variant: Variant,
}

impl Choice {
    /// The mapping chosen for.
    #[must_use]
    pub const fn mapping(self) -> Mapping {
        self.mapping
    }

    /// The variant chosen.
    #[must_use]
    pub const fn variant(self) -> Variant {
        self.variant
    }
}

/// One variant for every mapping: what a lowering, and the reading, apply.
///
/// Only the choices that differ from a mapping's default are stored, so two values that choose
/// the same variants are equal however they were built.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mappings {
    changed: BTreeMap<Mapping, Variant>,
}

impl Mappings {
    /// Every mapping at its default: the piece as specified.
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// The same choices with `choice` made.
    #[must_use]
    pub fn with(mut self, choice: Choice) -> Self {
        if choice.variant == choice.mapping.default_variant() {
            self.changed.remove(&choice.mapping);
        } else {
            self.changed.insert(choice.mapping, choice.variant);
        }
        self
    }

    /// The same choices with `mapping` switched off.
    #[must_use]
    pub fn without(self, mapping: Mapping) -> Self {
        self.with(mapping.off())
    }

    /// The variant chosen for `mapping`.
    #[must_use]
    pub fn get(&self, mapping: Mapping) -> Variant {
        self.changed
            .get(&mapping)
            .copied()
            .unwrap_or_else(|| mapping.default_variant())
    }

    /// Whether `mapping` applies at all: any variant but [`Variant::Off`]. Always true of the
    /// pulse, the stack, the function words, the long stanzas, the drums and the rhyme, which have no off.
    #[must_use]
    pub fn has(&self, mapping: Mapping) -> bool {
        self.get(mapping) != Variant::Off
    }

    /// Every choice that differs from its mapping's default, in [`Mapping::ALL`]'s order.
    pub fn changed(&self) -> impl Iterator<Item = Choice> + '_ {
        self.changed.iter().map(|(mapping, variant)| Choice {
            mapping: *mapping,
            variant: *variant,
        })
    }

    /// Every way the mappings can be chosen, in a fixed order: each mapping's variants in their
    /// listed order, the first mapping slowest — so it starts with every mapping off. For a test
    /// that must cover the whole space.
    #[must_use]
    pub fn every() -> Vec<Self> {
        Mapping::ALL
            .into_iter()
            .fold(vec![Self::all()], |sets, mapping| {
                sets.into_iter()
                    .flat_map(|set| {
                        mapping.variants().iter().filter_map(move |variant| {
                            mapping
                                .choose(*variant)
                                .map(|choice| set.clone().with(choice))
                        })
                    })
                    .collect()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{Mapping, Mappings, Variant};

    #[test]
    fn every_mapping_is_found_by_its_own_name() {
        for mapping in Mapping::ALL {
            assert_eq!(Mapping::named(mapping.name()), Some(mapping));
        }
        assert_eq!(Mapping::named("nonsense"), None);
    }

    #[test]
    fn every_mapping_can_be_left_out_and_defaults_to_a_variant_it_has() {
        for mapping in Mapping::ALL {
            // What preceded it: off, or for the pulse — which no piece can be without — elastic,
            // and for the stack — whose lines always divide their beats somehow — crossing.
            let preceded = match mapping {
                Mapping::Pulse => Variant::Elastic,
                Mapping::Stack => Variant::Crossing,
                Mapping::Function => Variant::Full,
                Mapping::LongStanzas => Variant::Gradual,
                Mapping::Drums => Variant::Flat,
                Mapping::Rhyme => Variant::Strict,
                _ => Variant::Off,
            };
            assert_eq!(mapping.variants().first(), Some(&preceded), "{mapping:?}");
            assert_eq!(mapping.off().variant(), preceded, "{mapping:?}");
            assert!(mapping.variants().contains(&mapping.default_variant()));
        }
    }

    #[test]
    fn the_pulse_is_steady_unless_elastic_is_chosen() {
        assert_eq!(
            Mapping::Pulse.variants(),
            &[Variant::Elastic, Variant::Steady]
        );
        assert_eq!(Mappings::all().get(Mapping::Pulse), Variant::Steady);
        assert!(Mapping::Pulse.choose(Variant::Off).is_none());
        assert!(Mapping::Pulse.choose_named("off").is_none());
        assert_eq!(Mapping::named("pulse"), Some(Mapping::Pulse));
        let elastic = Mappings::all().without(Mapping::Pulse);
        assert_eq!(elastic.get(Mapping::Pulse), Variant::Elastic);
        assert_eq!(
            elastic,
            Mappings::all().with(Mapping::Pulse.choose_named("elastic").unwrap())
        );
        // Neither variant is "off": the pulse always applies.
        assert!(elastic.has(Mapping::Pulse));
    }

    #[test]
    fn the_stack_crosses_unless_straight_is_chosen() {
        assert_eq!(
            Mapping::Stack.variants(),
            &[Variant::Crossing, Variant::Straight]
        );
        assert_eq!(Mapping::named("stack"), Some(Mapping::Stack));
        assert_eq!(Mappings::all().get(Mapping::Stack), Variant::Crossing);
        assert!(Mapping::Stack.choose(Variant::Off).is_none());
        assert!(Mapping::Stack.choose_named("off").is_none());
        let straight = Mappings::all().with(Mapping::Stack.choose_named("straight").unwrap());
        assert_eq!(straight.get(Mapping::Stack), Variant::Straight);
        assert_eq!(straight.changed().count(), 1);
        // Left out, it is what preceded it: crossing, the default, so nothing is stored.
        assert_eq!(straight.without(Mapping::Stack), Mappings::all());
        assert!(Mappings::all().has(Mapping::Stack));
    }

    #[test]
    fn function_words_are_full_unless_light_is_chosen() {
        assert_eq!(
            Mapping::Function.variants(),
            &[Variant::Full, Variant::Light]
        );
        assert_eq!(Mapping::named("function-words"), Some(Mapping::Function));
        assert_eq!(Mappings::all().get(Mapping::Function), Variant::Full);
        assert!(Mapping::Function.choose(Variant::Off).is_none());
        assert!(Mapping::Function.choose_named("off").is_none());
        assert!(Mapping::Function.choose_named("straight").is_none());
        let light = Mappings::all().with(Mapping::Function.choose_named("light").unwrap());
        assert_eq!(light.get(Mapping::Function), Variant::Light);
        assert_eq!(light.changed().count(), 1);
        // Left out, it is what preceded it: full, the default, so nothing is stored.
        assert_eq!(light.without(Mapping::Function), Mappings::all());
        assert!(Mappings::all().has(Mapping::Function));
    }

    #[test]
    fn long_stanzas_erode_unless_gradual_is_chosen() {
        assert_eq!(
            Mapping::LongStanzas.variants(),
            &[Variant::Gradual, Variant::Erode]
        );
        assert_eq!(Mapping::named("long-stanzas"), Some(Mapping::LongStanzas));
        assert_eq!(Mappings::all().get(Mapping::LongStanzas), Variant::Erode);
        assert!(Mapping::LongStanzas.choose(Variant::Off).is_none());
        assert!(Mapping::LongStanzas.choose_named("off").is_none());
        assert!(Mapping::LongStanzas.choose_named("light").is_none());
        assert!(Mapping::Stack.choose_named("erode").is_none());
        let gradual = Mappings::all().with(Mapping::LongStanzas.choose_named("gradual").unwrap());
        assert_eq!(gradual.get(Mapping::LongStanzas), Variant::Gradual);
        assert_eq!(gradual.changed().count(), 1);
        // Left out, it is what preceded it: gradual, a change from the default.
        assert_eq!(
            Mappings::all().without(Mapping::LongStanzas),
            gradual.clone()
        );
        assert!(gradual.has(Mapping::LongStanzas));
        assert_eq!(
            gradual.with(Mapping::LongStanzas.choose_named("erode").unwrap()),
            Mappings::all()
        );
    }

    #[test]
    fn the_defaults_are_todays_piece() {
        // The four machine-cycle switches are on, the bass is by vowel height, the reading reads
        // a stanza at a time with a breath between, and the harmony — a candidate not yet in the
        // piece — is off.
        let all = Mappings::all();
        for mapping in Mapping::ALL {
            assert_eq!(
                all.has(mapping),
                mapping != Mapping::Harmony && mapping != Mapping::Anchor,
                "{mapping:?}"
            );
        }
        assert_eq!(all.get(Mapping::Pulse), Variant::Steady);
        assert_eq!(all.get(Mapping::Bass), Variant::Height);
        assert_eq!(all.get(Mapping::Reading), Variant::Breath);
        assert_eq!(all.get(Mapping::Harmony), Variant::Off);
        assert_eq!(all.get(Mapping::Stack), Variant::Crossing);
        assert_eq!(all.get(Mapping::Function), Variant::Full);
        assert_eq!(all.get(Mapping::LongStanzas), Variant::Erode);
        assert_eq!(all.get(Mapping::Drums), Variant::Flat);
        assert_eq!(all.get(Mapping::Rhyme), Variant::Strict);
        assert_eq!(all.get(Mapping::Anchor), Variant::Off);
        assert_eq!(all.changed().count(), 0);
    }

    #[test]
    fn the_anchor_is_off_unless_on_is_chosen() {
        assert_eq!(Mapping::Anchor.variants(), &[Variant::Off, Variant::On]);
        assert_eq!(Mapping::named("anchor"), Some(Mapping::Anchor));
        assert_eq!(Mappings::all().get(Mapping::Anchor), Variant::Off);
        assert_eq!(Mapping::Anchor.default_variant(), Variant::Off);
        let on = Mappings::all().with(Mapping::Anchor.choose_named("on").unwrap());
        assert_eq!(on.get(Mapping::Anchor), Variant::On);
        assert_eq!(on.changed().count(), 1);
        assert_eq!(on.clone().without(Mapping::Anchor), Mappings::all());
        assert!(!Mappings::all().has(Mapping::Anchor));
        assert!(on.has(Mapping::Anchor));
    }

    #[test]
    fn the_drums_are_flat_unless_articulated_is_chosen() {
        assert_eq!(
            Mapping::Drums.variants(),
            &[Variant::Flat, Variant::Articulated]
        );
        assert_eq!(Mapping::named("drums"), Some(Mapping::Drums));
        assert_eq!(Mappings::all().get(Mapping::Drums), Variant::Flat);
        assert!(Mapping::Drums.choose(Variant::Off).is_none());
        assert!(Mapping::Drums.choose_named("off").is_none());
        assert!(Mapping::Drums.choose_named("light").is_none());
        let articulated = Mappings::all().with(Mapping::Drums.choose_named("articulated").unwrap());
        assert_eq!(articulated.get(Mapping::Drums), Variant::Articulated);
        assert_eq!(articulated.changed().count(), 1);
        // Left out, it is what preceded it: flat, the default, so nothing is stored.
        assert_eq!(articulated.without(Mapping::Drums), Mappings::all());
        assert!(Mappings::all().has(Mapping::Drums));
    }

    #[test]
    fn the_rhyme_is_strict_unless_slant_is_chosen() {
        assert_eq!(
            Mapping::Rhyme.variants(),
            &[Variant::Strict, Variant::Slant]
        );
        assert_eq!(Mapping::named("rhyme"), Some(Mapping::Rhyme));
        assert_eq!(Mappings::all().get(Mapping::Rhyme), Variant::Strict);
        assert!(Mapping::Rhyme.choose(Variant::Off).is_none());
        assert!(Mapping::Rhyme.choose_named("off").is_none());
        assert!(Mapping::Rhyme.choose_named("light").is_none());
        let slant = Mappings::all().with(Mapping::Rhyme.choose_named("slant").unwrap());
        assert_eq!(slant.get(Mapping::Rhyme), Variant::Slant);
        assert_eq!(slant.changed().count(), 1);
        // Left out, it is what preceded it: strict, the default, so nothing is stored.
        assert_eq!(slant.without(Mapping::Rhyme), Mappings::all());
        assert!(Mappings::all().has(Mapping::Rhyme));
    }

    #[test]
    fn the_reading_breathes_unless_another_variant_is_chosen() {
        assert_eq!(
            Mapping::Reading.variants(),
            &[
                Variant::Off,
                Variant::Arrives,
                Variant::Stanza,
                Variant::Breath
            ]
        );
        assert_eq!(Mappings::all().get(Mapping::Reading), Variant::Breath);
        let breath = Mapping::Reading.choose_named("breath").unwrap();
        assert_eq!(breath.variant(), Variant::Breath);
        assert!(Mapping::Bass.choose_named("breath").is_none());
        // The default is stored as nothing; the reading as it shipped is a change.
        assert_eq!(Mappings::all().with(breath), Mappings::all());
        let arrives = Mappings::all().with(Mapping::Reading.choose_named("arrives").unwrap());
        assert_eq!(arrives.changed().count(), 1);
        assert_eq!(arrives.get(Mapping::Reading), Variant::Arrives);
        // Left out, it is off, like every mapping that can be.
        assert_eq!(
            Mappings::all()
                .without(Mapping::Reading)
                .get(Mapping::Reading),
            Variant::Off
        );
    }

    #[test]
    fn a_variant_a_mapping_lacks_cannot_be_chosen() {
        assert!(Mapping::Bass.choose(Variant::Stanza).is_none());
        assert!(Mapping::Bass.choose(Variant::On).is_none());
        assert!(Mapping::Harmony.choose(Variant::Feet).is_none());
        let feet = Mapping::Bass.choose_named("feet").unwrap();
        assert_eq!(feet.variant(), Variant::Feet);
        assert_eq!(
            Mapping::Bass.variants(),
            &[
                Variant::Off,
                Variant::Height,
                Variant::Syllables,
                Variant::Feet
            ]
        );
        assert!(Mapping::Reading.choose(Variant::On).is_none());
        assert!(Mapping::Reading.choose_named("on").is_none());
        assert!(Mapping::Outro.choose_named("polka").is_none());
        assert!(Mapping::Harmony.choose(Variant::On).is_none());
        assert!(Mapping::Reading.choose_named("cadence").is_none());
        let both = Mapping::Harmony.choose_named("both").unwrap();
        assert_eq!(both.variant(), Variant::Both);
        let stanza = Mapping::Reading.choose_named("stanza").unwrap();
        assert_eq!(
            (stanza.mapping(), stanza.variant()),
            (Mapping::Reading, Variant::Stanza)
        );
    }

    #[test]
    fn choices_are_kept_until_set_back_to_the_default() {
        let some = Mappings::all()
            .without(Mapping::Outro)
            .without(Mapping::Bass)
            .with(Mapping::Reading.choose(Variant::Stanza).unwrap());
        assert!(!some.has(Mapping::Outro));
        assert!(!some.has(Mapping::Bass));
        assert!(some.has(Mapping::HeldLane));
        assert!(some.has(Mapping::Reading));
        assert_eq!(some.get(Mapping::Reading), Variant::Stanza);
        let changed: Vec<(Mapping, Variant)> = some
            .changed()
            .map(|choice| (choice.mapping(), choice.variant()))
            .collect();
        assert_eq!(
            changed,
            vec![
                (Mapping::Bass, Variant::Off),
                (Mapping::Outro, Variant::Off),
                (Mapping::Reading, Variant::Stanza),
            ]
        );
        // Choosing a default again is the same value as never having changed it.
        let back = some
            .with(Mapping::Bass.choose(Variant::Height).unwrap())
            .with(Mapping::Outro.choose(Variant::On).unwrap())
            .with(Mapping::Reading.choose(Variant::Breath).unwrap());
        assert_eq!(back, Mappings::all());
    }

    #[test]
    fn every_combination_is_listed_once() {
        // Five on/off switches, a four-way bass, a four-way harmony, a four-way reading, a
        // two-way pulse, a two-way stack, two-way function words, two-way long stanzas,
        // two-way drums and two-way rhyme: 2^5 × 4 × 4 × 4 × 2 × 2 × 2 × 2 × 2 × 2 = 131072.
        let every = Mappings::every();
        assert_eq!(every.len(), 131_072);
        let unique: std::collections::BTreeSet<_> = every.iter().map(|m| &m.changed).collect();
        assert_eq!(unique.len(), every.len());
        assert!(every.contains(&Mappings::all()));
    }
}
