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
//! | bass | §4.6 | [`Event::bass`], on a foundation line's syllables or feet ([`crate::bass`]) |
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

use crate::arrange::{Arrangement, Lane, Part, Placement, Slot};
use crate::bass::BassNote;
use crate::drums::Drum;
use crate::harmony::{Chord, Harmony};
use crate::mapping::Variant;
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
/// `index / of` of the way through, lasting [`Subdivision::parts`] of those parts. In the score
/// a syllable is always one part — there is no held note, tie or rest, which is what `SPEC.md`
/// §4.2's "syllable → subdivision within the foot" describes and no more. Only a stacked line
/// fitted to its host ([`Score::placed_in`] under the straight stack, `crate::stack`) can last
/// longer: the long syllable of a long-short.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subdivision {
    index: usize,
    of: usize,
    parts: usize,
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

    /// How many of the beat's [`Subdivision::of`] parts it lasts: one, except where the straight
    /// stack fits a stacked line to its host and a syllable takes the long half of a long-short.
    #[must_use]
    pub const fn parts(self) -> usize {
        self.parts
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
    source: PitchSource,
}

/// Where a note came from, for tracing it back (invariant I4).
///
/// Only a rhyme gives an event a [`Pitch`] — the melody. The bass was a second source here until
/// its walking variants needed a note under a rhyme word too; it is now [`Event::bass`], so one
/// event can carry both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PitchSource {
    /// The word a rhyme rests on: its class's degree (`SPEC.md` §4.5).
    Rhyme(RhymeClassId),
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

    /// The rhyme class this came from, if a rhyme gave it.
    #[must_use]
    pub const fn class(self) -> Option<RhymeClassId> {
        match self.source {
            PitchSource::Rhyme(class) => Some(class),
        }
    }

    /// What gave it.
    #[must_use]
    pub const fn source(self) -> PitchSource {
        self.source
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
    bass: Option<BassNote>,
    sharpness: Surprisal,
    family: Option<Family>,
    anchor: Option<Anchor>,
    bank: Option<Phoneme>,
    light: bool,
    drum: Drum,
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

    /// Whether the light function words (`Mapping::Function`, `crate::accent`) struck this
    /// syllable at [`Velocity::SECONDARY`] rather than its dictionary's primary stress: a
    /// one-syllable function word on a position the meter calls weak. Always false under the
    /// default, full. Carried so the debug view can say which pass decided the gain (`SPEC.md`
    /// §8), and so a backend's header can count them.
    #[must_use]
    pub const fn is_light(self) -> bool {
        self.light
    }

    /// Which piece of the drum kit this syllable plays (`Mapping::Drums`, [`crate::drums`]). Under
    /// [`Variant::Flat`] (the default), [`Drum::Pulse`] on a foundation voice and
    /// [`Drum::Ornament`] on a flourish voice; under [`Variant::Articulated`], chosen by layer,
    /// beat parity and stress within the foot.
    #[must_use]
    pub const fn drum(self) -> Drum {
        self.drum
    }

    /// Its note, if this syllable is the one the line's rhyme rests on.
    ///
    /// `None` for every other syllable, which is most of them: `SPEC.md` §4.5 maps rhyme to
    /// melody, and a rhyme is a property of one word.
    #[must_use]
    pub const fn pitch(self) -> Option<Pitch> {
        self.pitch
    }

    /// Its bass note, if the bass mapping gave it one (`SPEC.md` §4.6, [`crate::bass`]): a
    /// stressed foundation syllable under the height bass, any foundation syllable under W1, a
    /// foot's first syllable under W2. Independent of [`Event::pitch`]: under the walking variants
    /// a rhyme word carries both.
    #[must_use]
    pub const fn bass(self) -> Option<BassNote> {
        self.bass
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
    arrangement: Arrangement,
    harmony: Vec<Harmony>,
    bass: Variant,
    pulse: Variant,
    stack: Variant,
    function: Variant,
    drums: Variant,
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
            arrangement: Arrangement::default(),
            harmony: Vec::new(),
            bass: Variant::Off,
            pulse: Variant::Steady,
            stack: Variant::Crossing,
            function: Variant::Full,
            drums: Variant::Flat,
        }
    }

    /// The same score, recording which variant of the bass made its bass notes.
    pub(crate) const fn with_bass(mut self, variant: Variant) -> Self {
        self.bass = variant;
        self
    }

    /// Which variant of [`Mapping::Bass`](crate::mapping::Mapping::Bass) made the bass notes
    /// ([`Event::bass`]), so a backend and the debug view can say what they are.
    #[must_use]
    pub const fn bass(&self) -> Variant {
        self.bass
    }

    /// The same score, recording which variant of the pulse its backends play it at.
    pub(crate) const fn with_pulse(mut self, variant: Variant) -> Self {
        self.pulse = variant;
        self
    }

    /// Which variant of [`Mapping::Pulse`](crate::mapping::Mapping::Pulse) the piece is played at:
    /// [`Variant::Steady`], every bar as long as its beats, or [`Variant::Elastic`], every bar the
    /// same span. Nothing in the score is laid out by it — every event's bar, beat and subdivision
    /// is the same under both — so it is carried for the backends, which decide when a bar
    /// sounds, and for the debug view.
    #[must_use]
    pub const fn pulse(&self) -> Variant {
        self.pulse
    }

    /// The same score, recording which variant of the stack fits its stacked lines.
    pub(crate) const fn with_stack(mut self, variant: Variant) -> Self {
        self.stack = variant;
        self
    }

    /// Which variant of [`Mapping::Stack`](crate::mapping::Mapping::Stack) the piece's stacked
    /// lines are laid out by: [`Variant::Crossing`], each by its own feet, or
    /// [`Variant::Straight`], fitted to its host where they would cross. Nothing in
    /// [`Score::events`] changes with it — a line's own material is the same under both; it is
    /// applied slot by slot, by [`Score::placed_in`].
    #[must_use]
    pub const fn stack(&self) -> Variant {
        self.stack
    }

    /// The same score, recording which variant of the function words struck its syllables.
    pub(crate) const fn with_function(mut self, variant: Variant) -> Self {
        self.function = variant;
        self
    }

    /// Which variant of [`Mapping::Function`](crate::mapping::Mapping::Function) struck the
    /// piece's syllables: [`Variant::Full`], every one at its dictionary stress, or
    /// [`Variant::Light`], a one-syllable function word off the meter's strong positions at
    /// secondary force ([`Event::is_light`]). Unlike the pulse and the stack, it is already applied
    /// to [`Score::events`]; it is recorded so the backends and the debug view can name it.
    #[must_use]
    pub const fn function(&self) -> Variant {
        self.function
    }

    /// How many syllables of the poem the light function words struck lightly
    /// ([`Event::is_light`]): each once, however many slots its line sounds in. Zero under
    /// [`Variant::Full`]. The one count the pattern header, the OSC schedule and the explain view
    /// print.
    #[must_use]
    pub fn lightened(&self) -> usize {
        self.events.iter().filter(|event| event.is_light()).count()
    }

    /// The same score, recording which variant of the drums assigned each syllable's drum piece.
    pub(crate) const fn with_drums(mut self, variant: Variant) -> Self {
        self.drums = variant;
        self
    }

    /// Which variant of [`Mapping::Drums`](crate::mapping::Mapping::Drums) assigned the piece's
    /// drum roles ([`Event::drum`]): [`Variant::Flat`], one piece per layer, or
    /// [`Variant::Articulated`], chosen by layer, beat parity and stress within the foot.
    #[must_use]
    pub const fn drums(&self) -> Variant {
        self.drums
    }

    /// How many syllables of the poem play a [`Drum`] different from their voice's flat piece
    /// ([`Drum::flat_for`]): each counted once in [`Score::events`], zero under [`Variant::Flat`].
    #[must_use]
    pub fn articulated(&self) -> usize {
        self.events
            .iter()
            .filter(|event| {
                let layer = self
                    .voice(event.voice())
                    .map_or(Layer::Flourish, |voice| voice.layer());
                event.drum() != Drum::flat_for(layer)
            })
            .count()
    }

    /// The same score, with each line's harmony ([`crate::harmony`]).
    pub(crate) fn with_harmony(mut self, harmony: Vec<Harmony>) -> Self {
        self.harmony = harmony;
        self
    }

    /// What the harmony layer made of each line, in bar order: a chord, or a rest and why — or
    /// nothing at all when [`Mapping::Harmony`](crate::mapping::Mapping::Harmony) is off.
    #[must_use]
    pub fn harmony(&self) -> &[Harmony] {
        &self.harmony
    }

    /// Every chord, in the order they sound: bar by bar, and within a turning bar its own chord
    /// before its turnaround.
    pub fn chords(&self) -> impl Iterator<Item = Chord> + '_ {
        self.harmony.iter().flat_map(|harmony| harmony.chords())
    }

    /// The harmony of one line, by its bar.
    #[must_use]
    pub fn harmony_of(&self, bar: usize) -> Option<Harmony> {
        self.harmony
            .iter()
            .find(|harmony| harmony.bar().get() == bar)
            .copied()
    }

    /// The harmony a slot of the arrangement plays: that of the whole line sounding in the
    /// foundation layer there — its host, or the foundation's loop under a flourish host. Never a
    /// line in the call, held or refrain lane: one harmony a bar — one chord, or a stanza's last
    /// bar's two, one after the other ([`Harmony::Turning`]).
    ///
    /// The one place this is decided, so every backend and the scorer agree on it.
    #[must_use]
    pub fn harmony_in(&self, slot: &Slot) -> Option<Harmony> {
        let foundation = slot.placements().iter().find(|placement| {
            placement.lane() == Lane::Layer(crate::role::Layer::Foundation)
                && placement.part() == Part::Line
        })?;
        self.harmony_of(foundation.bar().get())
    }

    /// Whether `placement`'s bass notes sound in `slot`.
    ///
    /// Under the height bass, yes, wherever the line sounds — as it always did: its notes are the
    /// root, third and fifth of one scale, so two lines' notes at one instant are a chord, never a
    /// clash, and the default piece stays the piece it was.
    ///
    /// Under a walking bass (W1, W2), only the slot's foundation-layer line — the one
    /// [`Score::harmony_in`] takes the bar's chords from. A walk is *one* line moving by step and by
    /// semitone; a held, refrain or call line walking in its own lane under it put two or three
    /// walks in one bar, and their passing and approach tones landed a semitone apart (on Blake's
    /// Tyger under W1, 59 of 257 bass onsets). One walk a bar, as one chord a bar (`SPEC.md` §4.6).
    /// The line under it still sounds everything else it sounds — its drums, its melody.
    ///
    /// The one place this is decided, so every backend and the scorer agree on it.
    #[must_use]
    pub fn sounds_bass(&self, slot: &Slot, placement: &Placement) -> bool {
        match self.bass {
            Variant::Syllables | Variant::Feet => slot
                .placements()
                .iter()
                .find(|candidate| {
                    candidate.lane() == Lane::Layer(crate::role::Layer::Foundation)
                        && candidate.part() == Part::Line
                })
                .is_some_and(|walking| walking == placement),
            _ => true,
        }
    }

    /// The same score, laid out in time by [`crate::arrange`].
    pub(crate) fn with_arrangement(mut self, arrangement: Arrangement) -> Self {
        self.arrangement = arrangement;
        self
    }

    /// Which lines sound in which bar of the piece, and why ([`crate::arrange`]).
    ///
    /// The events above are each line's material, placed within *its own* bar; this says where
    /// those bars sound — a stanza stated, looping under another, or transforming into the next.
    /// A backend plays the piece by walking these slots, not the bars.
    #[must_use]
    pub const fn arrangement(&self) -> &Arrangement {
        &self.arrangement
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

    /// How many lines the score has material for — one bar each (`SPEC.md` §4.2).
    ///
    /// The piece's length is [`Score::arrangement`]'s slot count, which is longer once stanzas
    /// loop.
    ///
    /// Counted while lowering, not inferred from the last event: a final line whose words the
    /// phonemiser cannot pronounce produces no events, and a piece that ends in silence is still
    /// that long. Deriving this from `events.last()` dropped the last bar.
    #[must_use]
    pub const fn bars(&self) -> usize {
        self.bars
    }

    /// How many beats a bar has — its line's foot count (`SPEC.md` §4.2).
    ///
    /// Bars genuinely differ: §4.2 insists "iambic pentameter yields five beats […] Don't quantise
    /// to 4/4", so a consumer that needs a bar's length must be told rather than assume one. Zero
    /// for a bar whose words could not be pronounced.
    #[must_use]
    pub fn beats_in(&self, bar: usize) -> usize {
        self.events
            .iter()
            .filter(|event| event.bar.0 == bar)
            .map(|event| event.beat.0 + 1)
            .max()
            .unwrap_or(0)
    }

    /// What a placement sounds: its line's events — or, for a call, only those inside the call;
    /// for a line eroding under a long arrival, only those its pass keeps
    /// ([`Cause::Eroding`](crate::arrange::Cause::Eroding)) — and the beats they span. An eroding
    /// line keeps its whole line's beats, so what is left of it sounds where it always did.
    ///
    /// Every consumer of the arrangement goes through this, so a backend never has to know a call
    /// from a line: it renders [`Placed::events`] with each beat counted from
    /// [`Placed::first_beat`], across [`Placed::beats`] beats, fitted to the slot.
    #[must_use]
    pub fn placed(&self, placement: Placement) -> Placed<'_> {
        let bar = placement.bar().get();
        let mut events: Vec<&Event> = self
            .events
            .iter()
            .filter(|event| event.bar.0 == bar && placement.part().covers(event.span))
            .collect();
        if let crate::arrange::Cause::Eroding { pass } = placement.cause() {
            crate::arrange::erode(&mut events, pass);
        }
        let (first_beat, beats) = match placement.part() {
            Part::Line => (0, self.beats_in(bar)),
            Part::Call(_) => {
                let first = events.iter().map(|e| e.beat.0).min().unwrap_or(0);
                let last = events.iter().map(|e| e.beat.0).max().unwrap_or(0);
                (
                    first,
                    if events.is_empty() {
                        0
                    } else {
                        last - first + 1
                    },
                )
            }
        };
        let subdivisions = events.iter().map(|event| event.subdivision).collect();
        Placed {
            events,
            subdivisions,
            first_beat,
            beats,
            straightened: Vec::new(),
        }
    }

    /// Whether `placement` sounds `event`: its line's, inside its part, and — on a line eroding
    /// under a long arrival — one its pass keeps. [`Placed::sounds`] without building the
    /// placement where nothing erodes.
    #[must_use]
    pub fn sounds(&self, placement: Placement, event: &Event) -> bool {
        event.bar == placement.bar()
            && placement.part().covers(event.span)
            && (!matches!(placement.cause(), crate::arrange::Cause::Eroding { .. })
                || self.placed(placement).sounds(event))
    }

    /// What a placement sounds *in this slot*: [`Score::placed`], with each event's position in
    /// its beat as it is heard here — under the straight stack, a stacked line's beat that would
    /// cross the host's is fitted to the host's subdivision ([`crate::stack`]).
    ///
    /// Every consumer that times an event — the Strudel pattern and the page's trace, the OSC
    /// schedule, the scorer's timeline — reads [`Placed::subdivision`] from this, so they agree on
    /// where every syllable falls under either variant. Under [`Variant::Crossing`] it is exactly
    /// [`Score::placed`].
    #[must_use]
    pub fn placed_in(&self, slot: &Slot, placement: Placement) -> Placed<'_> {
        let mut placed = self.placed(placement);
        if self.stack == Variant::Straight {
            crate::stack::straighten(self, slot, placement, &mut placed);
        }
        placed
    }

    /// How many beats of the piece the straight stack fitted: each `(slot, beat)` in which any
    /// stacked line was fitted to its host ([`Score::placed_in`]) counts once, however many lines
    /// were fitted there. Zero under [`Variant::Crossing`].
    ///
    /// The one count the pattern header, the OSC schedule and the explain view all print, so the
    /// debug surfaces cannot disagree about it.
    #[must_use]
    pub fn straightened_beats(&self) -> usize {
        self.arrangement
            .slots()
            .iter()
            .map(|slot| {
                slot.placements()
                    .iter()
                    .flat_map(|placement| {
                        self.placed_in(slot, *placement)
                            .straightened()
                            .iter()
                            .map(|fitted| fitted.beat())
                            .collect::<Vec<_>>()
                    })
                    .collect::<std::collections::BTreeSet<usize>>()
                    .len()
            })
            .sum()
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

/// What one [`Placement`] sounds — see [`Score::placed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed<'a> {
    events: Vec<&'a Event>,
    /// Each event's position in its beat as it sounds here, parallel to `events`.
    subdivisions: Vec<Subdivision>,
    first_beat: usize,
    beats: usize,
    straightened: Vec<Straightened>,
}

/// One beat of a stacked line fitted to its host's subdivision by the straight stack
/// ([`crate::stack`]): which beat, and the two grids that would have crossed there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Straightened {
    beat: usize,
    own: usize,
    host: usize,
}

impl Straightened {
    pub(crate) const fn new(beat: usize, own: usize, host: usize) -> Self {
        Self { beat, own, host }
    }

    /// The beat of the slot, counted from the placement's first beat — which is also the slot's
    /// beat, since only a placement on the slot's own grid is fitted.
    #[must_use]
    pub const fn beat(self) -> usize {
        self.beat
    }

    /// The line's own subdivision of the beat: the least common multiple of its onsets'
    /// denominators there (3 for a triplet).
    #[must_use]
    pub const fn own(self) -> usize {
        self.own
    }

    /// The host's subdivision of the same beat, which the line was fitted to (2 for a pair).
    #[must_use]
    pub const fn host(self) -> usize {
        self.host
    }
}

impl<'a> Placed<'a> {
    /// The events, in playing order.
    #[must_use]
    pub fn events(&self) -> &[&'a Event] {
        &self.events
    }

    /// The line beat the placement starts on: 0 for a whole line, later for a call that does not
    /// open its line.
    #[must_use]
    pub const fn first_beat(&self) -> usize {
        self.first_beat
    }

    /// How many beats it spans, counted from [`Placed::first_beat`]. Zero if it makes no sound.
    #[must_use]
    pub const fn beats(&self) -> usize {
        self.beats
    }

    /// Where `event` sounds in its beat here: its own [`Event::subdivision`], or — under the
    /// straight stack, in a beat that would have crossed the host's — the fitted one. An event
    /// this placement does not sound keeps its own.
    #[must_use]
    pub fn subdivision(&self, event: &Event) -> Subdivision {
        self.events
            .iter()
            .zip(&self.subdivisions)
            .find(|(own, _)| {
                own.bar == event.bar
                    && own.beat == event.beat
                    && own.subdivision == event.subdivision
                    && own.span == event.span
            })
            .map_or(event.subdivision, |(_, fitted)| *fitted)
    }

    /// Whether the placement sounds `event` — the one test of membership every consumer uses, so
    /// a syllable an eroding line has lost is absent from the pattern, the trace and the schedule
    /// alike.
    #[must_use]
    pub fn sounds(&self, event: &Event) -> bool {
        self.events.contains(&event)
    }

    /// Every event with its position here, in playing order.
    pub fn onsets(&self) -> impl Iterator<Item = (&'a Event, Subdivision)> + '_ {
        self.events
            .iter()
            .copied()
            .zip(self.subdivisions.iter().copied())
    }

    /// The beats the straight stack fitted to the host, in order — empty under crossing, for the
    /// host itself, and wherever nothing would have crossed.
    #[must_use]
    pub fn straightened(&self) -> &[Straightened] {
        &self.straightened
    }

    /// Fits the events of one beat: each `(position, subdivision)` replaces the subdivision of
    /// the event at that position of [`Placed::events`].
    pub(crate) fn fit(&mut self, fitted: &[(usize, Subdivision)], record: Straightened) {
        for (position, subdivision) in fitted {
            if let Some(slot) = self.subdivisions.get_mut(*position) {
                *slot = *subdivision;
            }
        }
        self.straightened.push(record);
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
        Self {
            index,
            of,
            parts: 1,
        }
    }

    /// A position lasting `parts` parts — only ever made by `crate::stack`.
    pub(crate) const fn lasting(index: usize, of: usize, parts: usize) -> Self {
        Self { index, of, parts }
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
            source: PitchSource::Rhyme(class),
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
            bass: None,
            sharpness: Surprisal::NONE,
            family: None,
            anchor: None,
            bank: None,
            light: false,
            drum: Drum::Pulse,
        }
    }

    /// The same syllable struck lightly: secondary force, marked as the function words' doing.
    pub(crate) const fn lightened(mut self) -> Self {
        self.velocity = Velocity::SECONDARY;
        self.light = true;
        self
    }

    pub(crate) const fn with_drum(mut self, drum: Drum) -> Self {
        self.drum = drum;
        self
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

    pub(crate) const fn with_bass(mut self, bass: BassNote) -> Self {
        self.bass = Some(bass);
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
