//! Which layer a stanza plays: `SPEC.md` §4.1.
//!
//! > Do **not** hardcode "stanza 1 = base loop". Compute per-stanza metrical variance […]
//! > low variance → periodic → foundation layer […] high variance → aperiodic → flourish layer.
//! > This is the load-bearing decision.
//!
//! It is load-bearing because it is what makes writing a tight stanza *be* writing a tight loop.
//! Everything downstream — which voices carry the pulse, which ornament it — follows from this
//! one classification, so it is also the pass where a wrong answer is least visible and most
//! damaging: a plausible arrangement in which nothing traces to the text (`SPEC.md` §1).
//!
//! # What decides a role
//!
//! Only line-count evidence: the spread of syllable counts and of foot counts across a stanza's
//! lines, both measured relative to line length so that a long-lined stanza is not called ragged
//! merely for being long. Both must clear [`PERIODIC`]; a stanza whose lines share a syllable
//! count but not a foot count is not periodic in the sense that matters, because the beats fall
//! differently even where the syllables do not.
//!
//! Deliberately *not* [`LineMeter::dominant_foot`](crate::meter::LineMeter::dominant_foot), which
//! agrees with the metre a reader would name only 50–57% of the time on iambic pentameter —
//! `SPEC.md` §9's lexical-versus-phrasal stress limitation. §4.1 asks for counts, which are
//! reliable, and not for foot kind, which is not.
//!
//! # What the measure cannot see
//!
//! **Variance is blind to order.** It is computed from `n`, `Σx` and `Σx²`, so it is
//! permutation-invariant: `[8,6,8,6]`, `[8,6,6,8]` and `[6,8,8,6]` are one stanza to this pass,
//! though only the first is a repeating cell. A ballad scores well here for the same reason its
//! shuffle does — not because anything detected the alternation.
//!
//! That is faithful to §4.1, which specifies dispersion ("stddev of syllable count"), but §4.1
//! *names* the thing it wants "periodic", and dispersion is not periodicity. The cost is
//! measurable in both directions: a limerick and a Sapphic stanza are rigidly fixed forms that
//! this pass calls ragged, and no threshold fixes them, because their spread exceeds free verse
//! the pass correctly rejects. `crates/prosody-lts/tests/roles.rs` measures and pins exactly that,
//! and `PLAN.md` records it as an open question rather than absorbing it into the constant.
//! Detecting period would be a change to *what* a poetic feature maps to, which `CLAUDE.md` §10
//! says is a spec change to raise, not to make in passing.
//!
//! # Where a role stops being local
//!
//! Invariant I2 exists so that editing one stanza cannot disturb another. Classifying against a
//! fixed threshold keeps that promise for the stanza grain: a stanza is periodic on its own
//! evidence, and no edit elsewhere can change its measured spreads or take away a
//! [`StanzaBasis::Regularity`] verdict.
//!
//! Two judgements here are poem-wide, and both are deliberate:
//!
//! - [`StanzaBasis::MostRegular`] — `SPEC.md` §9's free-verse fallback. In free verse every
//!   stanza is ragged, so every stanza reads as flourish and the piece has no foundation at all:
//!   "needs a fallback […] so the *most* regular stanza in a poem always anchors." It fires only
//!   when the absolute rule finds nothing, so locality holds in any poem with at least one
//!   regular stanza.
//! - [`LineBasis::Refrain`] — a line is a refrain *because another line matches it*, which is
//!   inherently a fact about the pair. Editing stanza 1 into a copy of a line in stanza 3 makes
//!   that line in stanza 3 a refrain, though stanza 3 is untouched. This is `SPEC.md` §4.3's
//!   figure, not §4.1's measurement, and it cannot be made local without ceasing to detect
//!   repetition.
//!
//! Both are recorded in the basis rather than folded into the layer, so a poem-wide judgement is
//! visible in the output instead of hidden in it.
//!
//! # Two grains
//!
//! A refrain defeats stanza-grain variance. In Psalm 136 or a villanelle the refrain lines are
//! the most loop-worthy thing in the poem, but a stanza's variance is a fact about how its lines
//! *differ*, so the refrain is invisible exactly where it matters. Lines therefore carry roles
//! too, and a line that repeats elsewhere is foundation whatever its stanza is doing.
//!
//! §4.1 also asks for a second tier "where a line-internal caesura splits a voice". That is
//! **not implemented**: [`Line::segments`](crate::parse::Line::segments) already exposes the
//! candidate boundaries and nothing here reads them. Deferred deliberately, and recorded in
//! `PLAN.md`; the refrain tier is a different second tier, not that one.

use std::collections::BTreeMap;

use crate::meter::{PoemMeter, StanzaMeter, Variance};
use crate::parse::StanzaIdx;
use crate::repetition::{PoemRepetition, VoiceId};
use crate::span::Span;

/// The relative variance at or below which a stanza counts as periodic.
///
/// Measured, not chosen: `crates/prosody-lts/tests/roles.rs` classifies real verse of known form
/// and pins this from both sides. Since a stanza needs *both* spreads under the threshold, the
/// value that decides each case is its larger spread, and the binding constraints are Coleridge's
/// expanded common-measure stanza at 0.0364, which must stay in, and Whitman's "Song of Myself"
/// at 0.0880, which must stay out. A window of about 2.4×, and the test asserts the window rather
/// than an accuracy score — the corpus is hand-picked, so a percentage would grade the corpus.
///
/// Compare it only against a *relative* variance
/// ([`StanzaMeter::relative_syllable_variance`](crate::meter::StanzaMeter::relative_syllable_variance)),
/// never a raw one.
pub const PERIODIC: Variance = Variance::threshold(1, 20);

/// Which musical layer a unit plays (`SPEC.md` §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Periodic: drums, bass, ostinato. The pulse.
    Foundation,
    /// Aperiodic: fills, stabs, one-shots, ornament.
    Flourish,
}

/// Why a *stanza* got the layer it got.
///
/// Carried rather than recomputed, because §4.1's rule has a poem-wide escape hatch and two
/// degenerate cases, and a role that only says "foundation" cannot be checked against the text
/// that produced it (invariant I4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StanzaBasis {
    /// Both relative spreads are at or below [`PERIODIC`].
    Regularity,
    /// At least one spread exceeds [`PERIODIC`].
    Irregularity,
    /// No stanza in the poem was regular, and this was the most regular of them.
    ///
    /// `SPEC.md` §9's free-verse fallback.
    MostRegular,
    /// Fewer than two lines, so there is nothing to vary against.
    ///
    /// A one-line stanza has zero spread because it has no second line, which is not evidence of
    /// periodicity.
    TooShort,
    /// No syllables at all, so there is nothing to measure.
    ///
    /// A stanza of numerals or punctuation, or — under a phonemiser with no letter-to-sound
    /// fallback — one in a language the dictionary does not cover (`SPEC.md` §9). Its spread
    /// computes to zero, which reads identically to "perfectly regular"; distinguishing them is
    /// what stops a decorative divider from becoming the pulse of the piece.
    Unmeasured,
    /// Promoted although its evidence is too thin to judge, because nothing in the poem had any.
    ///
    /// A one-line poem, or a poem written one line per stanza. `SPEC.md` §9 wants the most
    /// regular stanza to *always* anchor; this says the anchor was chosen for want of an
    /// alternative rather than on merit.
    OnlyCandidate,
}

/// Why a *line* got the layer it got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LineBasis {
    /// The line belongs to a voice that occurs more than once, so it loops whatever its stanza
    /// does (`SPEC.md` §4.3's refrain row).
    ///
    /// Carries the voice so that a reader — and the §8 debug view — can be told *which* other
    /// lines it loops with, rather than only that it repeats something.
    Refrain(VoiceId),
    /// No separate claim of its own: the line takes its stanza's verdict, for this reason.
    Inherited(StanzaBasis),
}

/// One line's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRole {
    span: Span,
    layer: Layer,
    basis: LineBasis,
}

impl LineRole {
    /// The line's extent, for tracing the role back to the text (invariant I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Which layer the line plays.
    #[must_use]
    pub const fn layer(&self) -> Layer {
        self.layer
    }

    /// Why.
    #[must_use]
    pub const fn basis(&self) -> LineBasis {
        self.basis
    }
}

/// One stanza's role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanzaRole {
    span: Span,
    layer: Layer,
    basis: StanzaBasis,
    relative_syllable_variance: Variance,
    relative_foot_variance: Variance,
    lines: Vec<LineRole>,
}

impl StanzaRole {
    /// The stanza's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Which layer the stanza plays.
    #[must_use]
    pub const fn layer(&self) -> Layer {
        self.layer
    }

    /// Why.
    #[must_use]
    pub const fn basis(&self) -> StanzaBasis {
        self.basis
    }

    /// The syllable-count spread that decided it, relative to line length.
    ///
    /// Named `relative_` because
    /// [`StanzaMeter::syllable_variance`](crate::meter::StanzaMeter::syllable_variance) is the
    /// *raw* figure and the two differ by a factor of mean² — comparing one against the other, or
    /// against [`PERIODIC`], gives a silently wrong answer.
    #[must_use]
    pub const fn relative_syllable_variance(&self) -> Variance {
        self.relative_syllable_variance
    }

    /// The foot-count spread that decided it, relative to line length.
    #[must_use]
    pub const fn relative_foot_variance(&self) -> Variance {
        self.relative_foot_variance
    }

    /// The stanza's lines, in order.
    #[must_use]
    pub fn lines(&self) -> &[LineRole] {
        &self.lines
    }
}

/// Every role in a poem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoemRoles {
    poem: crate::seed::Fingerprint,
    stanzas: Vec<StanzaRole>,
}

impl PoemRoles {
    /// Which poem this describes; see [`Poem::fingerprint`](crate::parse::Poem::fingerprint).
    ///
    /// Taken from the meter, which [`assign`] has already checked against the repetition. Without
    /// it a later pass could combine these roles with another poem's analyses and invert the
    /// arrangement — §4.1's load-bearing decision, silently taken from the wrong text.
    #[must_use]
    pub const fn poem(&self) -> crate::seed::Fingerprint {
        self.poem
    }

    /// The poem's stanzas, in order.
    #[must_use]
    pub fn stanzas(&self) -> &[StanzaRole] {
        &self.stanzas
    }

    /// One stanza's role.
    #[must_use]
    pub fn stanza(&self, index: StanzaIdx) -> Option<&StanzaRole> {
        self.stanzas.get(index.0)
    }

    /// Every line that plays the foundation layer, with the stanza it sits in.
    ///
    /// The pulse of the piece: what §4.1 calls drums, bass and ostinato. This is the accessor a
    /// backend consumes, so it is the one worth asserting against — a stanza marked
    /// [`Layer::Foundation`] whose lines were all flourishes would leave a piece with no pulse
    /// while looking correct at the stanza grain.
    pub fn foundation(&self) -> impl Iterator<Item = (&StanzaRole, &LineRole)> + '_ {
        self.lines()
            .filter(|(_, line)| line.layer == Layer::Foundation)
    }

    /// Every line's role, in order, paired with the stanza it belongs to.
    ///
    /// Paired because the next pass builds one voice per unit and needs the stanza's verdict
    /// alongside each line; recovering it by searching for a containing span would be a
    /// re-derivation of something already known here.
    pub fn lines(&self) -> impl Iterator<Item = (&StanzaRole, &LineRole)> + '_ {
        self.stanzas
            .iter()
            .flat_map(|stanza| stanza.lines.iter().map(move |line| (stanza, line)))
    }
}

/// Why a poem's roles could not be assigned.
///
/// One variant, because there is one way to get this wrong: handing [`assign`] two analyses that
/// came from different poems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("meter and repetition describe different poems")]
pub struct DifferentPoems;

/// Assigns a layer to every stanza and line (`SPEC.md` §4.1).
///
/// Takes both analyses because §4.1 needs the variance and the refrain problem needs the
/// repetitions.
///
/// # Errors
///
/// [`DifferentPoems`] if the two analyses did not come from the same poem. This is checked rather
/// than assumed, and checked by *fingerprint* rather than by comparing spans, because spans
/// cannot detect it: "war is kind maiden" and "one two three four" occupy exactly the same bytes,
/// so a mismatched pair does not fail loudly — it silently invents refrains between lines that
/// have never met, which is `SPEC.md` §1's failure mode exactly. `prosody-cli` is what wires
/// these together, so it is the caller most able to get it wrong.
pub fn assign(meter: &PoemMeter, repetition: &PoemRepetition) -> Result<PoemRoles, DifferentPoems> {
    let repeated = repeated_lines(meter, repetition)?;

    let verdicts: Vec<Verdict> = meter.stanzas().iter().map(verdict).collect();
    let promoted = promotion(&verdicts);

    let stanzas = meter
        .stanzas()
        .iter()
        .zip(&verdicts)
        .enumerate()
        .map(|(index, (stanza, verdict))| {
            let (layer, basis) = match promoted {
                Some((at, basis)) if at == index => (Layer::Foundation, basis),
                _ => (verdict.layer, verdict.basis),
            };
            StanzaRole {
                span: stanza.span(),
                layer,
                basis,
                relative_syllable_variance: verdict.syllables,
                relative_foot_variance: verdict.feet,
                lines: stanza
                    .lines()
                    .iter()
                    .map(|line| match repeated.get(&line.span()) {
                        Some(&voice) => LineRole {
                            span: line.span(),
                            layer: Layer::Foundation,
                            basis: LineBasis::Refrain(voice),
                        },
                        None => LineRole {
                            span: line.span(),
                            layer,
                            basis: LineBasis::Inherited(basis),
                        },
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(PoemRoles {
        poem: meter.poem(),
        stanzas,
    })
}

/// A stanza's verdict on its own evidence, before the poem-wide fallback is considered.
struct Verdict {
    layer: Layer,
    basis: StanzaBasis,
    syllables: Variance,
    feet: Variance,
}

impl Verdict {
    /// Whether the stanza had enough evidence to be judged at all.
    const fn measurable(&self) -> bool {
        matches!(
            self.basis,
            StanzaBasis::Regularity | StanzaBasis::Irregularity
        )
    }
}

fn verdict(stanza: &StanzaMeter) -> Verdict {
    // Read rather than recompute: `StanzaMeter` already holds both statistics, and two
    // implementations of one number drift.
    let syllables = stanza.relative_syllable_variance();
    let feet = stanza.relative_foot_variance();
    let without_evidence = |basis| Verdict {
        layer: Layer::Flourish,
        basis,
        syllables,
        feet,
    };

    // One line has nothing to vary against, so its zero spread says nothing about periodicity.
    // Two is the shortest stanza that can genuinely be regular — a couplet.
    if stanza.lines().len() < 2 {
        return without_evidence(StanzaBasis::TooShort);
    }
    // Zero spread also means "nothing was counted". `Variance::relative` returns zero for an empty
    // sum, so a stanza of numerals, punctuation or unpronounceable words is indistinguishable from
    // a perfectly regular one — and, being Foundation, would deny the free-verse fallback to the
    // whole poem.
    if stanza
        .lines()
        .iter()
        .all(|line| line.syllable_count().get() == 0)
    {
        return without_evidence(StanzaBasis::Unmeasured);
    }

    let periodic = syllables <= PERIODIC && feet <= PERIODIC;
    Verdict {
        layer: if periodic {
            Layer::Foundation
        } else {
            Layer::Flourish
        },
        basis: if periodic {
            StanzaBasis::Regularity
        } else {
            StanzaBasis::Irregularity
        },
        syllables,
        feet,
    }
}

/// Which stanza to promote, and under which basis, if any.
///
/// `SPEC.md` §9 wants the most regular stanza to *always* anchor, so this runs whenever nothing
/// qualified on its own evidence, in two tiers:
///
/// 1. the least ragged stanza that could actually be measured — §9's fallback proper;
/// 2. failing that, the first stanza with any sound in it at all, which is how a one-line poem
///    (or one written a line per stanza) still gets a pulse. It is marked
///    [`StanzaBasis::OnlyCandidate`], because "least ragged of nothing" is not a measurement.
///
/// A poem with no syllables anywhere gets no foundation, which is the one honest answer: there is
/// nothing to make a loop out of.
fn promotion(verdicts: &[Verdict]) -> Option<(usize, StanzaBasis)> {
    if verdicts.iter().any(|v| v.layer == Layer::Foundation) {
        return None;
    }
    verdicts
        .iter()
        .enumerate()
        .filter(|(_, v)| v.measurable())
        // Ranked by syllable spread, then foot spread; `min_by` keeps the first of equal minima,
        // so a tie resolves to the earliest stanza. Both halves are pinned by tests: an unpinned
        // ranking or tie-break is an I1 hazard (`CLAUDE.md` §4), because it decides which voice
        // becomes the pulse of the piece.
        .min_by(|(_, a), (_, b)| a.syllables.cmp(&b.syllables).then(a.feet.cmp(&b.feet)))
        .map(|(index, _)| (index, StanzaBasis::MostRegular))
        .or_else(|| {
            verdicts
                .iter()
                .position(|v| v.basis != StanzaBasis::Unmeasured)
                .map(|index| (index, StanzaBasis::OnlyCandidate))
        })
}

/// Which lines repeat, and the voice each repeats with.
///
/// A [`VoiceId`] is shared by lines with identical text (`SPEC.md` §4.3's refrain row), so a voice
/// on more than one line *is* a refrain.
fn repeated_lines(
    meter: &PoemMeter,
    repetition: &PoemRepetition,
) -> Result<BTreeMap<Span, VoiceId>, DifferentPoems> {
    // Identity, checked once. Comparing spans instead would look thorough and catch nothing: two
    // different poems with the same line lengths have byte-identical spans. Equal fingerprints
    // mean equal sources, so the stanza and line structures below are guaranteed to line up and
    // the `zip`s cannot silently truncate.
    if meter.poem() != repetition.poem() {
        return Err(DifferentPoems);
    }

    let mut by_voice: BTreeMap<VoiceId, Vec<Span>> = BTreeMap::new();
    for (scanned, repeated) in meter.stanzas().iter().zip(repetition.stanzas()) {
        for (line, voiced) in scanned.lines().iter().zip(repeated.lines()) {
            by_voice
                .entry(voiced.voice())
                .or_default()
                .push(line.span());
        }
    }

    Ok(by_voice
        .into_iter()
        .filter(|(_, spans)| spans.len() > 1)
        .flat_map(|(voice, spans)| spans.into_iter().map(move |span| (span, voice)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{DifferentPoems, Layer, LineBasis, PERIODIC, StanzaBasis, assign};
    use crate::meter::{Variance, scan};
    use crate::parse::Poem;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, arpabet::ARPABET, syllabify,
    };
    use crate::repetition;

    /// One stressed syllable per vowel letter. Enough that a test can choose a line's length by
    /// writing words, without a dictionary's opinions entering a unit test.
    ///
    /// Under this phonemiser a line of *n* syllables segments into `ceil(n / 2)` feet, which is
    /// what makes the syllable and foot spreads separately predictable by hand.
    struct Counting;

    impl Phonemiser for Counting {
        fn inventory(&self) -> &'static Inventory {
            &ARPABET
        }

        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            let phones: Vec<_> = word
                .chars()
                .filter(|letter| "aeiou".contains(*letter))
                .map(|_| (ARPABET.phoneme("AA").unwrap(), Stress::Primary))
                .collect();
            if phones.is_empty() {
                return Err(Unpronounceable::NotKnown);
            }
            Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
        }
    }

    fn roles(source: &str) -> Vec<(Layer, StanzaBasis)> {
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        assign(&meter, &rep)
            .unwrap()
            .stanzas()
            .iter()
            .map(|s| (s.layer(), s.basis()))
            .collect()
    }

    #[test]
    fn even_lines_are_the_foundation() {
        // Four lines of two syllables each: nothing varies.
        let found = roles("ba ba\nba ba\nba ba\nba ba\n");
        assert_eq!(found, vec![(Layer::Foundation, StanzaBasis::Regularity)]);
    }

    #[test]
    fn ragged_lines_are_a_flourish() {
        // The second stanza is the ragged one. It needs company: a ragged stanza *alone* in a poem
        // is promoted by the free-verse fallback, which is a different rule tested below.
        let found =
            roles("ba ba\nba ba\n\nba\nba ba ba ba ba ba\nba ba\nba ba ba ba ba ba ba ba\n");
        assert_eq!(
            found,
            vec![
                (Layer::Foundation, StanzaBasis::Regularity),
                (Layer::Flourish, StanzaBasis::Irregularity),
            ]
        );
    }

    #[test]
    fn both_spreads_must_clear_the_threshold() {
        // Hand-worked. Stanza 0 is 2 and 3 syllables, so 1 and 2 feet:
        //   syllables (2·13 − 25)/25 = 1/25 = 0.0400 — inside the threshold
        //   feet      (2·5  −  9)/9  = 1/9  = 0.1111 — outside it
        // §4.1 names both spreads, and a stanza regular in syllables but not in feet has its
        // beats falling differently even where its syllables do not. Checking syllables alone
        // would classify this Foundation.
        assert!(Variance::relative(&[2, 3]) <= PERIODIC, "syllables inside");
        assert!(Variance::relative(&[1, 2]) > PERIODIC, "feet outside");
        let source = "ba ba\nba ba ba\n\nba ba\nba ba\n";
        let found = roles(source);
        assert_eq!(found[0], (Layer::Flourish, StanzaBasis::Irregularity));

        // The same stanza also pins which spread is which. The two are both `Variance` and both
        // reported, so swapping the labels changes nothing that any symmetric test can see —
        // and a role whose evidence wears the wrong name is an I4 failure, not a cosmetic one.
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();
        let stanza = &roles.stanzas()[0];
        assert_eq!(
            stanza.relative_syllable_variance(),
            Variance::relative(&[2, 3])
        );
        assert_eq!(stanza.relative_foot_variance(), Variance::relative(&[1, 2]));
        assert!(stanza.relative_syllable_variance() < stanza.relative_foot_variance());
    }

    #[test]
    fn a_single_line_stanza_does_not_claim_periodicity() {
        // Its variance is zero, but only because there is nothing to compare it against. Alone in
        // a poem it is still promoted, for want of anything better — but never on regularity.
        let found = roles("ba ba\n");
        assert_eq!(found, vec![(Layer::Foundation, StanzaBasis::OnlyCandidate)]);
    }

    #[test]
    fn a_stanza_with_no_sound_is_not_the_foundation() {
        // `Variance::relative` returns zero for an empty sum, so without an evidence check this
        // divider reads as perfectly regular — and, being Foundation, would deny the free-verse
        // fallback to the rest of the poem.
        let found = roles("12\n34\n\nba\nba ba ba ba ba ba\nba ba\n");
        assert_eq!(found[0], (Layer::Flourish, StanzaBasis::Unmeasured));
        assert_eq!(found[1], (Layer::Foundation, StanzaBasis::MostRegular));
    }

    #[test]
    fn a_poem_with_no_sound_at_all_has_no_foundation() {
        let found = roles("12\n34\n");
        assert_eq!(found, vec![(Layer::Flourish, StanzaBasis::Unmeasured)]);
    }

    #[test]
    fn a_poem_of_one_ragged_stanza_still_gets_a_foundation() {
        // Free verse in a single stanza is the degenerate case SPEC.md §9 names: with nothing
        // promoted the piece is all ornament and has no pulse at all.
        let found = roles("ba\nba ba ba ba ba ba\nba ba\nba ba ba ba ba ba ba ba\n");
        assert_eq!(found, vec![(Layer::Foundation, StanzaBasis::MostRegular)]);
    }

    #[test]
    fn a_poem_written_one_line_per_stanza_still_gets_a_foundation() {
        // Every stanza is TooShort, so the measured tier finds nothing. SPEC.md §9 says the most
        // regular stanza *always* anchors; this says the anchor was picked for want of one.
        let found = roles("ba ba\n\nba ba ba\n\nba\n");
        assert_eq!(found[0], (Layer::Foundation, StanzaBasis::OnlyCandidate));
        assert_eq!(found[1], (Layer::Flourish, StanzaBasis::TooShort));
        assert_eq!(found[2], (Layer::Flourish, StanzaBasis::TooShort));
    }

    #[test]
    fn the_most_regular_stanza_anchors_free_verse() {
        let source = "ba\nba ba ba ba ba ba ba ba ba ba\n\nba ba ba\nba ba ba ba ba ba\n";
        let found = roles(source);
        assert_eq!(
            found,
            vec![
                (Layer::Flourish, StanzaBasis::Irregularity),
                (Layer::Foundation, StanzaBasis::MostRegular),
            ]
        );
    }

    #[test]
    fn promotion_ranks_by_syllables_before_feet() {
        // Hand-worked. Stanza 0: 2 and 5 syllables → 1 and 3 feet.
        //   syllables (2·29 − 49)/49  =  9/49  = 0.1837 ; feet (2·10 − 16)/16 = 4/16 = 0.2500
        // Stanza 1: 3 and 8 syllables → 2 and 4 feet.
        //   syllables (2·73 − 121)/121 = 25/121 = 0.2066 ; feet (2·20 − 36)/36 = 4/36 = 0.1111
        // Stanza 0 is the better on syllables, stanza 1 on feet. §4.1 names syllable count first
        // and it is the more reliable of the two, so stanza 0 must anchor.
        let found = roles("ba ba\nba ba ba ba ba\n\nba ba ba\nba ba ba ba ba ba ba ba\n");
        assert_eq!(found[0].1, StanzaBasis::MostRegular);
        assert_eq!(found[1].1, StanzaBasis::Irregularity);
    }

    #[test]
    fn promotion_breaks_a_tie_towards_the_earlier_stanza() {
        // Two ragged stanzas measuring identically. Which one anchors decides which voice carries
        // the pulse, so an unpinned tie-break is an I1 hazard.
        let found = roles("ba\nba ba ba ba\n\nbe\nbe be be be\n");
        assert_eq!(found[0].1, StanzaBasis::MostRegular);
        assert_eq!(found[1].1, StanzaBasis::Irregularity);
    }

    #[test]
    fn the_fallback_stays_out_of_the_way_when_a_stanza_qualifies() {
        let found = roles("ba\nba ba ba ba ba ba ba ba ba ba\n\nba ba\nba ba\nba ba\n");
        assert_eq!(
            found,
            vec![
                (Layer::Flourish, StanzaBasis::Irregularity),
                (Layer::Foundation, StanzaBasis::Regularity),
            ]
        );
    }

    #[test]
    fn a_refrain_is_foundation_inside_a_ragged_stanza() {
        // Psalm 136's shape: a varied line, then the same refrain, over and over. Its stanzas are
        // ragged by construction, so variance alone finds nothing to loop.
        let source = "ba\nfor his mercy endureth\n\nba ba ba\nfor his mercy endureth\n";
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();

        let refrains: Vec<_> = roles
            .lines()
            .filter(|(_, line)| matches!(line.basis(), LineBasis::Refrain(_)))
            .collect();
        assert_eq!(refrains.len(), 2, "{refrains:?}");
        for (_, line) in &refrains {
            assert_eq!(line.layer(), Layer::Foundation);
            assert_eq!(line.span().text(source), Some("for his mercy endureth"));
        }
        // Both refrain lines name the same voice — the point of carrying it is that a reader can
        // be told *what* a line loops with, not merely that it repeats.
        let voices: Vec<_> = refrains
            .iter()
            .filter_map(|(_, line)| match line.basis() {
                LineBasis::Refrain(voice) => Some(voice),
                LineBasis::Inherited(_) => None,
            })
            .collect();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[0], voices[1]);
        // ...and no stanza earned foundation on its own regularity, which is the whole point: the
        // refrain is loop-worthy for a reason variance cannot see.
        assert!(
            roles
                .stanzas()
                .iter()
                .all(|s| s.basis() != StanzaBasis::Regularity)
        );
    }

    #[test]
    fn a_line_that_does_not_repeat_takes_its_stanzas_layer() {
        // Distinct lines of equal length: regular, and nothing repeats.
        let source = "ba be\nbi bo\nbu ba\n";
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();
        assert!(roles.lines().all(|(_, l)| {
            l.basis() == LineBasis::Inherited(StanzaBasis::Regularity)
                && l.layer() == Layer::Foundation
        }));
        assert_eq!(roles.foundation().count(), 3);
    }

    #[test]
    fn a_line_role_names_its_own_line() {
        // Not its stanza's extent: a containment check cannot tell the two apart, because a
        // stanza contains itself.
        let source = "ba be\nbi bo ba\n";
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();
        let texts: Vec<_> = roles
            .lines()
            .filter_map(|(_, l)| l.span().text(source))
            .collect();
        assert_eq!(texts, vec!["ba be", "bi bo ba"]);
    }

    #[test]
    fn a_promoted_stanzas_lines_carry_the_pulse_too() {
        // A stanza marked Foundation whose lines are all Flourish leaves the piece with no pulse
        // while looking right at the stanza grain. `foundation()` is what a backend reads.
        let source = "ba\nba ba ba ba ba ba\nba ba\n";
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();
        assert_eq!(roles.stanzas()[0].basis(), StanzaBasis::MostRegular);
        assert_eq!(roles.foundation().count(), 3);
    }

    #[test]
    fn mismatched_analyses_are_refused_rather_than_invented() {
        // Spans are byte offsets, so two unrelated poems collide constantly. Left unchecked this
        // produces confident refrains between lines that have never met.
        let a = Poem::parse("war is kind maiden\nlet us go then you\n").unwrap();
        let b = Poem::parse("one two three four\none two three four\n").unwrap();
        let meter = scan(&a, &Counting);
        let rep = repetition::analyse(&b);
        assert_eq!(
            a.source().len(),
            b.source().len(),
            "same bytes, different poems"
        );
        assert_eq!(assign(&meter, &rep), Err(DifferentPoems));
    }

    #[test]
    fn analyses_of_the_same_poem_are_accepted() {
        // The check must not be so eager that it rejects the ordinary case — an identical poem
        // parsed twice is still the same poem.
        let a = Poem::parse("ba ba\nba ba\n").unwrap();
        let b = Poem::parse("ba ba\nba ba\n").unwrap();
        let meter = scan(&a, &Counting);
        let rep = repetition::analyse(&b);
        assert!(assign(&meter, &rep).is_ok());
    }

    #[test]
    fn common_measure_is_within_the_threshold() {
        // A ballad's alternating four-and-three is the most loop-worthy stanza in English, and it
        // varies. The threshold has to admit it. Syllable counts 8,6,8,6.
        //
        // Note the name: *within the threshold*, not "periodic". See the test below.
        assert!(Variance::relative(&[8, 6, 8, 6]) <= PERIODIC);
        assert!(Variance::relative(&[4, 3, 4, 3]) <= PERIODIC);
    }

    #[test]
    fn the_measure_cannot_tell_a_ballad_from_its_shuffle() {
        // Pinning the known limitation rather than leaving it implicit. Relative variance is
        // permutation-invariant, so it cannot see the alternation that makes common measure a
        // loop. If this ever starts failing, someone has taught the pass to see order, and the
        // module docs and PLAN.md need to change with it.
        let ballad = Variance::relative(&[8, 6, 8, 6]);
        assert_eq!(ballad, Variance::relative(&[8, 6, 6, 8]));
        assert_eq!(ballad, Variance::relative(&[6, 8, 8, 6]));
    }

    #[test]
    fn free_verse_is_not_within_the_threshold() {
        assert!(Variance::relative(&[10, 27, 15, 8]) > PERIODIC);
    }

    #[test]
    fn an_empty_poem_has_no_roles() {
        let poem = Poem::parse("").unwrap();
        let meter = scan(&poem, &Counting);
        let rep = repetition::analyse(&poem);
        let roles = assign(&meter, &rep).unwrap();
        assert!(roles.stanzas().is_empty());
        assert_eq!(roles.foundation().count(), 0);
    }
}
