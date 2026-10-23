//! Choosing between candidate mappings, to hear what each does.
//!
//! Some mappings are still candidates: added by a design iteration and marked *pending review* in
//! `SPEC.md`, they change what a poem becomes, and whether they belong is judged by listening. Each
//! is a *selector*: a [`Mapping`] has a fixed, ordered list of [`Variant`]s — always including
//! a first variant that is what preceded the mapping ([`Variant::Off`] for every mapping but the
//! pulse, which cannot be left out and goes back to [`Variant::Elastic`], the stack, whose
//! lines always sound somehow and which goes back to [`Variant::Crossing`], and the function
//! words, which are always struck somehow and go back to [`Variant::Full`]) — and one of them is
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
        }
    }
}

/// The variants of a mapping that is simply on or off.
const SWITCH: &[Variant] = &[Variant::Off, Variant::On];

/// The reading's variants: off, line by line where each arrives, or a stanza at a time.
const READING: &[Variant] = &[Variant::Off, Variant::Arrives, Variant::Stanza];

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

impl Mapping {
    /// Every mapping, in the order a page lists them.
    pub const ALL: [Self; 10] = [
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
                "the poem read aloud: each line where it arrives, or a stanza a bar at a time"
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
        }
    }

    /// Its variants, in a fixed order that starts with what preceded the mapping:
    /// [`Variant::Off`], except for the pulse, which starts with [`Variant::Elastic`], the
    /// stack, which starts with [`Variant::Crossing`], and the function words, which start with
    /// [`Variant::Full`].
    #[must_use]
    pub const fn variants(self) -> &'static [Variant] {
        match self {
            Self::HeldLane | Self::RefrainLane | Self::StanzaPeriod | Self::Outro => SWITCH,
            Self::Bass => BASS,
            Self::Reading => READING,
            Self::Harmony => HARMONY,
            Self::Pulse => PULSE,
            Self::Stack => STACK,
            Self::Function => FUNCTION,
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
            // SPEC.md §4.7: each line is read where it is first heard — the reading as it shipped.
            Self::Reading => Variant::Arrives,
            // A candidate the user approved for comparison, not yet the piece as specified: off
            // keeps every existing pattern and score as it was (`PLAN.md`, the harmony proposal).
            Self::Harmony => Variant::Off,
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
    /// [`Variant::Elastic`], the stack, which goes back to [`Variant::Crossing`], and the
    /// function words, which go back to [`Variant::Full`].
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
    /// pulse, the stack and the function words, which have no off.
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
    fn the_defaults_are_todays_piece() {
        // The four machine-cycle switches are on, the bass is by vowel height, the reading reads
        // each line where it arrives, and the harmony — a candidate not yet in the piece — is off.
        let all = Mappings::all();
        for mapping in Mapping::ALL {
            assert_eq!(all.has(mapping), mapping != Mapping::Harmony, "{mapping:?}");
        }
        assert_eq!(all.get(Mapping::Pulse), Variant::Steady);
        assert_eq!(all.get(Mapping::Bass), Variant::Height);
        assert_eq!(all.get(Mapping::Reading), Variant::Arrives);
        assert_eq!(all.get(Mapping::Harmony), Variant::Off);
        assert_eq!(all.get(Mapping::Stack), Variant::Crossing);
        assert_eq!(all.get(Mapping::Function), Variant::Full);
        assert_eq!(all.changed().count(), 0);
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
            .with(Mapping::Reading.choose(Variant::Arrives).unwrap());
        assert_eq!(back, Mappings::all());
    }

    #[test]
    fn every_combination_is_listed_once() {
        // Four on/off switches, a four-way bass, a four-way harmony, a three-way reading, a
        // two-way pulse, a two-way stack and two-way function words:
        // 2^4 × 4 × 4 × 3 × 2 × 2 × 2 = 6144.
        let every = Mappings::every();
        assert_eq!(every.len(), 6144);
        for (index, one) in every.iter().enumerate() {
            assert!(!every[index + 1..].contains(one), "{one:?} twice");
        }
        assert!(every.contains(&Mappings::all()));
    }
}
