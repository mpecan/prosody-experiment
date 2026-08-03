//! The Voice IR: what the poem says, as sounding events, before anything decides how it sounds.
//!
//! `SPEC.md` §3 puts this between the analysis passes and the backends, fed by per-unit seeded
//! randomness and by style resolution. This module is the half that style never touches.
//!
//! # Why nothing here has a unit
//!
//! Invariant I3 says style is palette, not structure: "the moment the style block can alter
//! arrangement, the poem stops being the score and becomes an ornament on a preset." The
//! mechanical guarantee is that this module contains no quantity a style header could supply.
//! There is a [`ScaleDegree`] — which note of the scale — and no semitone, because §4.5 assigns
//! "scale degree […] from the style's scale" and *which* scale is palette. There is a
//! [`Velocity`] as an exact fraction and no decibel; a [`Detune`] as a fraction of a scale step
//! and no cents; a [`Bar`] and a [`Subdivision`] and no tempo.
//!
//! So a style can change every sound in the piece and cannot move a single event. That is I3.
//!
//! It is not yet *mechanically* enforced: no `Style` type exists anywhere in the workspace, so
//! there is nothing to import and nothing preventing a future `use crate::style::…` from landing
//! here. The real gate is the one `CLAUDE.md` §2 already uses for purity — realisation and
//! `Style` belong in a crate that depends on `prosody-core`, so that this module *cannot* reach
//! them. `PLAN.md` records that as the shape M6 must take.
//!
//! # What a voice is
//!
//! An instrument. Lines that repeat verbatim share one (`SPEC.md` §4.3's refrain row is "a literal
//! loop of that voice"), so a [`Voice`] is keyed by the [`VoiceId`] the repetition pass assigns
//! and carries the layer §4.1 gave it. Its [`Seed`] comes from the line's own content, and exists
//! for the residual jitter `CLAUDE.md` §5 describes — never for identity, which is exactly what
//! seeds cannot express, since they avalanche and "burn"/"burning" must stay one instrument.
//!
//! # What an event carries, and from where
//!
//! Six passes contribute, and they compose rather than compete, because they are opinions about
//! different things:
//!
//! | From | `SPEC.md` | Lands on |
//! |---|---|---|
//! | role | §4.1 | [`Voice::layer`] — which layer the instrument plays |
//! | repetition | §4.3 | [`Voice::id`] — which lines are the same instrument |
//! | meter | §4.2 | [`Event::bar`], [`Event::beat`], [`Event::subdivision`], [`Event::velocity`] |
//! | rhyme | §4.5 | [`Event::pitch`], on the rhyming word only |
//! | surprisal | §4.4 | [`Event::sharpness`] |
//! | word families | §4.3 | [`Event::family`] — modulation depth within an instrument |
//!
//! The one apparent collision is a word that is both a family member and a rhyme — recorded as an
//! open question since M4b. It is not a conflict: §4.3's word voice is a *timbral* family and
//! §4.5's class is *pitch*. One event carries both, and they drive different parameters.
//!
//! # Where the IR stops being local
//!
//! Invariant I2 wants an edit to disturb only what it touches. Two things here are poem-wide:
//!
//! - **A [`ScaleDegree`] is a poem-wide number.** §4.5 assigns degrees "in order of first
//!   appearance", which is inherently a fact about the whole poem: introduce a new rhyme in
//!   stanza 1 and every later class shifts up one, so a byte-identical stanza 3 is transposed.
//!   That is I2's own words — "changing one word reshuffles the whole piece" — and it follows
//!   directly from what §4.5 asks for, so it is raised in `PLAN.md` rather than fixed here.
//!   `CLAUDE.md` §10: changing *what* a feature maps to is a spec change.
//! - **[`Family`] and [`Voice`] identity** are likewise assigned by first appearance, inherited
//!   from the repetition passes.
//!
//! Everything else — position, velocity, sharpness, provenance — is derived from a unit's own
//! content and cannot move when another unit changes.
//!
//! # Melody is the rhyme words
//!
//! §4.5 maps rhyme to melody, and a rhyme is a property of the word a line ends on. Pitch
//! therefore lands on that word's syllables and nowhere else. Giving every syllable of the line
//! its class's degree would be a monotone, and would attribute a pitch to syllables that earned
//! none — which invariant I4 forbids more strongly than it forbids silence.

use crate::meter::FootKind;
use crate::phoneme::{Phoneme, Provenance};
use crate::repetition::VoiceId;
use crate::rhyme::RhymeClassId;
use crate::role::Layer;
use crate::seed::Seed;
use crate::sound::{Distance, WordVoiceId};
use crate::span::Span;
use crate::surprisal::Surprisal;

/// Which bar an event falls in — `SPEC.md` §4.2's "line → bar".
///
/// Counted across the whole poem in reading order, so bar *n* is the *n*th line. Blank lines do
/// not consume one: a stanza break is a unit boundary, not a rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bar(usize);

impl Bar {
    /// The bar number, counting from zero.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Which beat of its bar an event falls on — `SPEC.md` §4.2's "foot → beat".
///
/// Named `BeatIndex` rather than `Beat` because [`meter::Beat`](crate::meter::Beat) is already a
/// syllable record. A bar has as many beats as its line has feet, which is the point of §4.2's
/// warning: "iambic pentameter yields five beats […] Don't quantise to 4/4."
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BeatIndex(usize);

impl BeatIndex {
    /// The beat number within its bar, counting from zero.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Where a syllable sits inside its beat — `SPEC.md` §4.2's "syllable → subdivision within the
/// foot".
///
/// Carries the divisor as well as the position, because a foot's syllables divide *that* beat and
/// the next beat may divide differently: a trochee and an anapest in one line are a beat in two
/// and a beat in three.
///
/// Deliberately **not** [`Ord`]. A derived order compares `(index, of)` lexicographically, which
/// puts `1/2` — halfway through the beat — before `1/3`, the reverse of when they sound; and two
/// subdivisions of *different* beats have no order worth defining. Sort events by
/// [`Event::bar`], [`Event::beat`] and then the index, which is what the lowering does.
///
/// The convention a backend needs: a beat divided into `of` parts sounds this event at
/// `index / of` of the way through, each part lasting `1 / of` of the beat. There is no held
/// note, tie or rest — a syllable is one part, which is what `SPEC.md` §4.2's "syllable →
/// subdivision within the foot" describes and no more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subdivision {
    index: usize,
    of: usize,
}

impl Subdivision {
    /// Which subdivision this is, counting from zero.
    #[must_use]
    pub const fn index(self) -> usize {
        self.index
    }

    /// How many the beat is divided into.
    #[must_use]
    pub const fn of(self) -> usize {
        self.of
    }
}

/// How hard a syllable is struck — `SPEC.md` §4.2's "stress → accent / velocity".
///
/// An exact fraction of full force, never a decibel or a MIDI number: how loud that *is* belongs
/// to realisation. Three values, because `CMUdict` distinguishes three stresses and inventing a
/// continuum between them would be inventing information the dictionary does not have.
/// Three thirds of full force, so the derived [`Ord`] is the numeric one — there is a single
/// implicit denominator rather than a per-value one, which is what makes deriving safe here and
/// unsafe on [`Detune`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Velocity(u32);

impl Velocity {
    /// The denominator every velocity shares.
    const FORCE: u32 = 3;

    /// Full force: a primary-stressed syllable.
    pub const PRIMARY: Self = Self(3);
    /// Secondary stress.
    pub const SECONDARY: Self = Self(2);
    /// An unstressed syllable, which still sounds — it is not a rest.
    pub const UNSTRESSED: Self = Self(1);

    /// The fraction, for display and for the backend.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.0) / f64::from(Self::FORCE)
    }
}

/// Which note of the style's scale — `SPEC.md` §4.5's "rhyme class → scale degree, assigned in
/// order of first appearance".
///
/// A degree, deliberately not a semitone. Which scale those degrees index into, and what root it
/// starts from, are palette; the poem decides only that these two lines take the *same* note and
/// that one is the third distinct note to appear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScaleDegree(usize);

impl ScaleDegree {
    /// The degree, counting from zero in order of first appearance.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// How far off the degree to sit — `SPEC.md` §4.5's "perfect rhyme → exact pitch; slant rhyme →
/// detune or neighbour tone."
///
/// A fraction of a scale step, so zero is a perfect rhyme and larger means further off. Kept as an
/// exact rational because it is derived from [`RhymeScore`](crate::rhyme::RhymeScore), which is
/// exact, and because `CLAUDE.md` §4 keeps floats out of anything that decides a branch.
///
/// The denominator is the rhyme tail's length and so varies from line to line — `1/2` and `1/4`
/// both occur in ordinary verse. [`Ord`] and [`PartialEq`] are therefore written out by
/// cross-multiplication rather than derived: a derive compares the *representation*, which makes
/// `1/2 < 1/4` and calls the worse slant rhyme the closer one. `Variance` and `RhymeScore` each
/// learned this; this type is the third, so the lesson is recorded at all three sites now.
#[derive(Debug, Clone, Copy)]
pub struct Detune {
    numerator: u64,
    denominator: u64,
}

impl PartialEq for Detune {
    fn eq(&self, other: &Self) -> bool {
        u128::from(self.numerator) * u128::from(other.denominator)
            == u128::from(other.numerator) * u128::from(self.denominator)
    }
}

impl Eq for Detune {}

impl core::hash::Hash for Detune {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Must agree with `PartialEq`, so hash the reduced fraction rather than the fields.
        let divisor = gcd(self.numerator, self.denominator);
        (self.numerator / divisor).hash(state);
        (self.denominator / divisor).hash(state);
    }
}

impl Ord for Detune {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Widened to `u128`: the cross product of two `u64`s overflows, which is how an exact
        // comparison silently becomes a wrong one.
        (u128::from(self.numerator) * u128::from(other.denominator))
            .cmp(&(u128::from(other.numerator) * u128::from(self.denominator)))
    }
}

impl PartialOrd for Detune {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Greatest common divisor, so [`Hash`] can agree with [`PartialEq`].
const fn gcd(a: u64, b: u64) -> u64 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    if a == 0 { 1 } else { a }
}

impl Detune {
    /// Dead on the degree.
    pub const EXACT: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    /// Whether this is exact — a perfect rhyme.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        self.numerator == 0
    }

    /// The fraction of a scale step, for the backend.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // display only; ordering uses the exact rational
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

/// A note: which degree, and how far off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pitch {
    degree: ScaleDegree,
    detune: Detune,
    class: RhymeClassId,
}

impl Pitch {
    /// Which note of the scale.
    #[must_use]
    pub const fn degree(self) -> ScaleDegree {
        self.degree
    }

    /// How far off it — zero for a perfect rhyme.
    #[must_use]
    pub const fn detune(self) -> Detune {
        self.detune
    }

    /// The rhyme class this came from, for tracing it back (invariant I4).
    #[must_use]
    pub const fn class(self) -> RhymeClassId {
        self.class
    }
}

/// A word's place in its morphological family — `SPEC.md` §4.3's "same voice, varied parameters".
///
/// The row the spec calls the one that earns the project. `depth` is the phoneme edit distance
/// from the family's root, and drives modulation depth: "burn" is the root at zero, "burnt" is
/// nearer than "burning".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Family {
    voice: WordVoiceId,
    depth: Distance,
}

impl Family {
    /// Which family — the timbral group, not the instrument.
    ///
    /// Distinct from [`Voice::id`]: that is which *line* this belongs to and decides the
    /// instrument; this is which *word family* and decides how the instrument is modulated. A
    /// word can be in both, which is not a conflict — they drive different parameters.
    #[must_use]
    pub const fn voice(self) -> WordVoiceId {
        self.voice
    }

    /// How far from the family's root, in phonemes.
    #[must_use]
    pub const fn depth(self) -> Distance {
        self.depth
    }
}

/// One instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voice {
    id: VoiceId,
    layer: Layer,
    span: Span,
    seed: Seed,
}

impl Voice {
    /// Which voice — shared by every line with identical text (`SPEC.md` §4.3's refrain row).
    #[must_use]
    pub const fn id(self) -> VoiceId {
        self.id
    }

    /// Which layer it plays (`SPEC.md` §4.1).
    #[must_use]
    pub const fn layer(self) -> Layer {
        self.layer
    }

    /// The first line that sounds this voice, for tracing it back (invariant I4).
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// The seed for this voice's residual choices.
    ///
    /// Derived from the line's own content (invariant I2), and **not** what makes it this voice —
    /// seeds avalanche, so they cannot express "burn and burning are one instrument". Identity is
    /// [`Voice::id`]; this is only the jitter within it (`CLAUDE.md` §5).
    #[must_use]
    pub const fn seed(self) -> Seed {
        self.seed
    }
}

/// What structural figure an event belongs to — the rest of `SPEC.md` §4.3's table.
///
/// §4.3 gives five rows and names a distinct musical result for each. Refrain and morphological
/// near-repetition are carried by [`Voice`] and [`Family`]; these are the other two, and they are
/// here rather than left to the backend because "downbeat-anchored" is a statement about *where in
/// the bar an event sits*. That is structure, and a backend cannot recover it from a `Score` that
/// never recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Anchor {
    /// The line-initial n-gram of an anaphora: "downbeat-anchored ostinato".
    Downbeat,
    /// The line-final n-gram of an epistrophe: "cadence figure / turnaround".
    Cadence,
}

/// One sounding event: a syllable, placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    span: Span,
    voice: VoiceId,
    bar: Bar,
    beat: BeatIndex,
    subdivision: Subdivision,
    foot: FootKind,
    velocity: Velocity,
    provenance: Provenance,
    pitch: Option<Pitch>,
    sharpness: Surprisal,
    family: Option<Family>,
    anchor: Option<Anchor>,
    bank: Option<Phoneme>,
}

impl Event {
    /// The word this syllable belongs to.
    ///
    /// A word's span, not the syllable's: spelling does not divide into syllables, so this is as
    /// far as invariant I4 can truthfully reach. [`Event::subdivision`] says which syllable.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// Which instrument sounds it.
    #[must_use]
    pub const fn voice(self) -> VoiceId {
        self.voice
    }

    /// Which bar.
    #[must_use]
    pub const fn bar(self) -> Bar {
        self.bar
    }

    /// Which beat of that bar.
    #[must_use]
    pub const fn beat(self) -> BeatIndex {
        self.beat
    }

    /// Where in the beat.
    #[must_use]
    pub const fn subdivision(self) -> Subdivision {
        self.subdivision
    }

    /// The foot this beat is, which is what decided the subdivision.
    #[must_use]
    pub const fn foot(self) -> FootKind {
        self.foot
    }

    /// How hard it is struck.
    #[must_use]
    pub const fn velocity(self) -> Velocity {
        self.velocity
    }

    /// Its note, if this syllable is the one the line's rhyme rests on.
    ///
    /// `None` for every other syllable, which is most of them: `SPEC.md` §4.5 maps rhyme to
    /// melody, and a rhyme is a property of one word.
    #[must_use]
    pub const fn pitch(self) -> Option<Pitch> {
        self.pitch
    }

    /// How distinctive its word is (`SPEC.md` §4.4), driving sharpness and brightness.
    #[must_use]
    pub const fn sharpness(self) -> Surprisal {
        self.sharpness
    }

    /// Its word's family, if it has one (`SPEC.md` §4.3).
    #[must_use]
    pub const fn family(self) -> Option<Family> {
        self.family
    }

    /// Which structural figure it belongs to, if any (`SPEC.md` §4.3).
    #[must_use]
    pub const fn anchor(self) -> Option<Anchor> {
        self.anchor
    }

    /// The phoneme whose repetition binds this word to its neighbours (`SPEC.md` §4.3).
    ///
    /// The alliteration or assonance row: "timbral continuity — same sample bank". Words sharing
    /// a bank should be played from one bank of sounds, which is a structural grouping the poem
    /// made; *which* bank is palette.
    #[must_use]
    pub const fn bank(self) -> Option<Phoneme> {
        self.bank
    }

    /// Whether the pronunciation behind this event was looked up or guessed.
    ///
    /// Carried because `CLAUDE.md` §6 requires a fallback to be visible and §8 makes the debug
    /// view the enforcement mechanism for I4 — and the debug view cannot render what the IR threw
    /// away. Without it, an event whose position, velocity, pitch and family all rest on a
    /// letter-to-sound guess is byte-identical to one resting on the dictionary.
    #[must_use]
    pub const fn provenance(self) -> Provenance {
        self.provenance
    }

    /// Whether this event rests on a guessed pronunciation.
    #[must_use]
    pub const fn rests_on_a_guess(self) -> bool {
        matches!(self.provenance, Provenance::LetterToSound)
    }
}

/// A whole poem, lowered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    voices: Vec<Voice>,
    events: Vec<Event>,
    bars: usize,
    silent: Vec<Span>,
}

impl Score {
    /// Builds a score. Crate-internal: the only way to get one is to lower a poem.
    pub(crate) fn new(
        voices: Vec<Voice>,
        events: Vec<Event>,
        bars: usize,
        silent: Vec<Span>,
    ) -> Self {
        Self {
            voices,
            events,
            bars,
            silent,
        }
    }

    /// Every instrument, ordered by [`VoiceId`].
    #[must_use]
    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// Every event, in playing order: by bar, then beat, then subdivision.
    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// One voice, by id.
    #[must_use]
    pub fn voice(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.iter().find(|voice| voice.id == id)
    }

    /// Every event a given voice sounds, in playing order.
    pub fn events_of(&self, id: VoiceId) -> impl Iterator<Item = &Event> + '_ {
        self.events.iter().filter(move |event| event.voice == id)
    }

    /// How many bars long the piece is — one per line (`SPEC.md` §4.2).
    ///
    /// Counted while lowering, not inferred from the last event: a final line whose words the
    /// phonemiser cannot pronounce produces no events, and a piece that ends in silence is still
    /// that long. Deriving this from `events.last()` dropped the last bar.
    #[must_use]
    pub const fn bars(&self) -> usize {
        self.bars
    }

    /// The words that make no sound, because no pronunciation could be found for them.
    ///
    /// `SPEC.md` §9's out-of-vocabulary case, kept visible at the IR rather than only in the
    /// meter. Without it a consumer holding a `Score` — which is all M6 and the §8 debug view
    /// hold — cannot tell a short line from a line half of which nobody could pronounce.
    #[must_use]
    pub fn silent(&self) -> &[Span] {
        &self.silent
    }
}

// Constructors used by the lowering passes. Kept crate-internal so that the only route to an
// `Event` is through a pass that had a poem in front of it — a publicly constructible `Event`
// would be an event with no token behind it, which is invariant I4's whole concern.
impl Bar {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
}

impl BeatIndex {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }
}

impl Subdivision {
    pub(crate) const fn new(index: usize, of: usize) -> Self {
        Self { index, of }
    }
}

impl ScaleDegree {
    pub(crate) const fn new(degree: usize) -> Self {
        Self(degree)
    }
}

impl Detune {
    /// How far off the degree a rhyme of this quality sits: `1 − score`.
    ///
    /// A perfect rhyme is exact and a total mismatch is a whole step away, which is the
    /// continuous reading `SPEC.md` §4.5 asks for — "Score rhyme quality continuously […] rather
    /// than boolean" — rather than a binary exact-or-neighbour.
    pub(crate) const fn from_score((matched, total): (u64, u64)) -> Self {
        if total == 0 {
            return Self::EXACT;
        }
        Self {
            numerator: total - matched,
            denominator: total,
        }
    }
}

impl Velocity {
    pub(crate) const fn of(stress: crate::phoneme::Stress) -> Self {
        match stress {
            crate::phoneme::Stress::Primary => Self::PRIMARY,
            crate::phoneme::Stress::Secondary => Self::SECONDARY,
            crate::phoneme::Stress::Unstressed => Self::UNSTRESSED,
        }
    }
}

impl Pitch {
    pub(crate) const fn new(class: RhymeClassId, degree: usize, detune: Detune) -> Self {
        Self {
            degree: ScaleDegree::new(degree),
            detune,
            class,
        }
    }
}

impl Family {
    pub(crate) const fn new(voice: WordVoiceId, depth: Distance) -> Self {
        Self { voice, depth }
    }
}

impl Voice {
    pub(crate) const fn new(id: VoiceId, layer: Layer, span: Span, seed: Seed) -> Self {
        Self {
            id,
            layer,
            span,
            seed,
        }
    }
}

impl Event {
    #[allow(clippy::too_many_arguments)] // the rhythm pass fixes seven facts at once; the rest
    // are added by the passes that own them, via the `with_*` methods below.
    pub(crate) const fn new(
        span: Span,
        voice: VoiceId,
        bar: Bar,
        beat: BeatIndex,
        subdivision: Subdivision,
        foot: FootKind,
        velocity: Velocity,
        provenance: Provenance,
    ) -> Self {
        Self {
            span,
            voice,
            bar,
            beat,
            subdivision,
            foot,
            velocity,
            provenance,
            pitch: None,
            sharpness: Surprisal::NONE,
            family: None,
            anchor: None,
            bank: None,
        }
    }

    pub(crate) const fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = Some(anchor);
        self
    }

    pub(crate) const fn with_bank(mut self, bank: Phoneme) -> Self {
        self.bank = Some(bank);
        self
    }

    pub(crate) const fn with_pitch(mut self, pitch: Pitch) -> Self {
        self.pitch = Some(pitch);
        self
    }

    pub(crate) const fn with_family(mut self, family: Family) -> Self {
        self.family = Some(family);
        self
    }

    pub(crate) const fn with_sharpness(mut self, sharpness: Surprisal) -> Self {
        self.sharpness = sharpness;
        self
    }

    /// The key events are sorted by before they reach a backend (`CLAUDE.md` §4).
    pub(crate) const fn order(&self) -> (usize, usize, usize, usize) {
        (
            self.bar.0,
            self.beat.0,
            self.subdivision.index,
            self.span.start(),
        )
    }
}
