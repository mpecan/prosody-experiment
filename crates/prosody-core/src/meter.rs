//! Meter: a line's syllables, its stresses, and how those divide into feet.
//!
//! `SPEC.md` §4.2 sets the mapping this module feeds — foot to beat, stress to accent, syllable
//! to subdivision, line to bar — and warns against the obvious mistake: "iambic pentameter yields
//! five beats. Poetry naturally produces odd meters. Don't quantise to 4/4." Nothing here rounds
//! a line to a convenient length.
//!
//! # What a stress vector rests on
//!
//! Stress comes from a [`Phonemiser`], and a guessed pronunciation carries
//! [`Provenance::LetterToSound`], where stress placement is right about two thirds of the time.
//! Every syllable here records where its stress came from, so a later pass can weigh a scansion
//! built on guesses differently from one built on dictionary entries — and so the debug view
//! (`SPEC.md` §8) can show which is which. Words with no pronunciation at all are listed rather
//! than dropped: a line whose words are half unknown must not read as a confident short line.

use crate::parse::{Line, Poem, Stanza};
use crate::phoneme::{Phonemiser, Provenance, Stress};
use crate::seed::Seed;
use crate::span::Span;

/// How many syllables something has — the subdivision count of `SPEC.md` §4.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyllableCount(usize);

impl SyllableCount {
    /// The count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// How many feet something has — the beat count of `SPEC.md` §4.2.
///
/// Distinct from [`SyllableCount`] so the two cannot be added together or passed to the wrong
/// parameter. The field is private for the same reason: a public `.0` would let arithmetic mix
/// the units and still compile, which is most of what the newtype is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FootCount(usize);

impl FootCount {
    /// The count.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// One syllable of a line, and where it came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// The span of the word this syllable belongs to.
    ///
    /// A word's span, not the syllable's: spelling does not divide into syllables, so there is no
    /// honest sub-span to point at. Attribution stops at the word, which is as far as invariant
    /// I4 can truthfully reach.
    pub word: Span,
    /// Which syllable of that word this is, counting from zero.
    pub index: usize,
    /// How strongly it is stressed.
    pub stress: Stress,
    /// Whether the pronunciation it came from was looked up or guessed.
    pub provenance: Provenance,
}

/// A metrical foot.
///
/// The classical feet, plus [`FootKind::Single`]. A five-syllable line cannot be tiled by
/// two- and three-syllable feet alone, and refusing to scan such lines would discard most free
/// verse — which `SPEC.md` §9 already flags as the hard case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FootKind {
    /// Unstressed, stressed — "the TIDE".
    Iamb,
    /// Stressed, unstressed — "BURNING".
    Trochee,
    /// Two unstressed then stressed — "in the DARK".
    Anapest,
    /// Stressed then two unstressed — "TENDERLY".
    Dactyl,
    /// Two stressed — "HEART BREAK".
    Spondee,
    /// Unstressed, stressed, unstressed — "a-MAZE-ment", "No-VEM-ber".
    ///
    /// English verse does use it, and without it an amphibrachic line still tiles at the same
    /// mismatch cost out of iambs and pyrrhics — so the search never notices, and the feet are
    /// simply named wrong.
    Amphibrach,
    /// Two unstressed.
    Pyrrhic,
    /// A single syllable, where nothing longer fits.
    Single,
}

impl FootKind {
    /// Every kind, in the order ties are broken.
    ///
    /// The order matters and is fixed: the two-syllable feet that carry English verse first, then
    /// the three-syllable ones, then the degenerate cases. Without a fixed order a tie in a short
    /// line would resolve by whatever order the search happened to take.
    pub const ALL: [Self; 8] = [
        Self::Iamb,
        Self::Trochee,
        Self::Anapest,
        Self::Dactyl,
        Self::Amphibrach,
        Self::Spondee,
        Self::Pyrrhic,
        Self::Single,
    ];

    /// The stress pattern, as "is this syllable stressed".
    #[must_use]
    pub const fn template(self) -> &'static [bool] {
        match self {
            Self::Iamb => &[false, true],
            Self::Trochee => &[true, false],
            Self::Anapest => &[false, false, true],
            Self::Dactyl => &[true, false, false],
            Self::Amphibrach => &[false, true, false],
            Self::Spondee => &[true, true],
            Self::Pyrrhic => &[false, false],
            Self::Single => &[true],
        }
    }

    /// How many syllables it covers.
    #[must_use]
    pub const fn syllables(self) -> usize {
        self.template().len()
    }

    /// The conventional name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Iamb => "iamb",
            Self::Trochee => "trochee",
            Self::Anapest => "anapest",
            Self::Dactyl => "dactyl",
            Self::Amphibrach => "amphibrach",
            Self::Spondee => "spondee",
            Self::Pyrrhic => "pyrrhic",
            Self::Single => "single",
        }
    }
}

/// One foot of a line.
///
/// Built only by this module's segmentation, so `beats` always spans exactly
/// `kind.syllables()` syllables. The
/// fields are private to keep that true: a public `Foot` could be assembled with a range whose
/// width disagreed with its kind, and nothing downstream would notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foot {
    kind: FootKind,
    span: Span,
    beats: core::ops::Range<usize>,
}

impl Foot {
    /// Which foot this is.
    #[must_use]
    pub const fn kind(&self) -> FootKind {
        self.kind
    }

    /// The words this foot covers.
    ///
    /// A hull over whole words, so **two adjacent feet may overlap**: in "the BURN-ing TIDE" the
    /// word "burning" gives one syllable to each foot, and both spans contain it. That is the
    /// poem's own doing, not an ambiguity introduced here — a word really can straddle two beats.
    /// Attribution down to the individual syllable is exact, via [`Foot::beats`] and
    /// [`Beat::index`]; this span is the coarser answer for the debug view.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Where this foot's syllables sit in its line's beat list.
    ///
    /// Only meaningful against the [`LineMeter`] it came from; use [`LineMeter::beats_of`] rather
    /// than indexing a slice with it.
    #[must_use]
    pub fn beats(&self) -> core::ops::Range<usize> {
        self.beats.clone()
    }
}

/// A line's meter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMeter {
    span: Span,
    beats: Vec<Beat>,
    feet: Vec<Foot>,
    unpronounced: Vec<Span>,
}

impl LineMeter {
    /// The line's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Every syllable of the line, in order.
    #[must_use]
    pub fn beats(&self) -> &[Beat] {
        &self.beats
    }

    /// The line divided into feet.
    #[must_use]
    pub fn feet(&self) -> &[Foot] {
        &self.feet
    }

    /// Words the phonemiser could not pronounce, so they contributed no syllables.
    ///
    /// Non-empty means this line's syllable count is an undercount, and whatever reads it should
    /// say so rather than treating the line as short.
    #[must_use]
    pub fn unpronounced(&self) -> &[Span] {
        &self.unpronounced
    }

    /// The syllables belonging to `foot`.
    ///
    /// `None` if `foot` came from a different line, which is the only way its range can fall
    /// outside this one. Indexing `beats()` with `foot.beats()` directly would panic instead.
    #[must_use]
    pub fn beats_of(&self, foot: &Foot) -> Option<&[Beat]> {
        self.beats.get(foot.beats())
    }

    /// How many syllables the line has — its subdivisions (`SPEC.md` §4.2).
    #[must_use]
    pub fn syllable_count(&self) -> SyllableCount {
        SyllableCount(self.beats().len())
    }

    /// How many feet the line has — its beats (`SPEC.md` §4.2).
    #[must_use]
    pub fn foot_count(&self) -> FootCount {
        FootCount(self.feet.len())
    }

    /// The foot kind the line is mostly made of, if any is.
    ///
    /// The name a reader would give the line: five iambs make it iambic even with a trochee
    /// substituted in the first position, which is what English verse actually does.
    #[must_use]
    pub fn dominant_foot(&self) -> Option<FootKind> {
        let mut best: Option<(FootKind, usize)> = None;
        for kind in FootKind::ALL {
            let count = self.feet.iter().filter(|f| f.kind() == kind).count();
            if count > 0 && best.is_none_or(|(_, seen)| count > seen) {
                best = Some((kind, count));
            }
        }
        best.map(|(kind, _)| kind)
    }

    /// Whether any of the line's stress came from a guess rather than a dictionary.
    #[must_use]
    pub fn rests_on_a_guess(&self) -> bool {
        self.beats.iter().any(|b| b.provenance.is_guess())
    }
}

/// The spread of a set of small counts, held exactly.
///
/// This is the input to `SPEC.md` §4.1's role assignment, which compares one stanza's regularity
/// against another's. `CLAUDE.md` §4 forbids floats in control flow, and a comparison that
/// decides whether a stanza becomes a drum loop or a fill is control flow of the most audible
/// kind — so the variance is kept as integers and compared by cross-multiplication. Two stanzas
/// order the same way on every platform, with no float equality anywhere.
///
/// Population variance, as the rational `(n·Σx² − (Σx)²) / n²`.
///
/// Equality is by *value*, not by representation. A derived `PartialEq` would compare the stored
/// numerator and denominator, and then two stanzas of different line counts with identical
/// spread would compare unequal while [`Ord`] called them equal — breaking the contract that
/// `cmp` returning `Equal` implies `==`, on the exact comparison this type exists to make.
#[derive(Debug, Clone, Copy)]
pub struct Variance {
    numerator: u128,
    denominator: u128,
}

impl Variance {
    /// The variance of a set of counts.
    ///
    /// An empty set has zero variance, as does a single value.
    #[must_use]
    pub fn of(counts: &[usize]) -> Self {
        let n = counts.len() as u128;
        if n == 0 {
            return Self {
                numerator: 0,
                denominator: 1,
            };
        }
        let sum: u128 = counts.iter().map(|&c| c as u128).sum();
        let sum_squares: u128 = counts.iter().map(|&c| (c as u128) * (c as u128)).sum();
        Self {
            numerator: n * sum_squares - sum * sum,
            denominator: n * n,
        }
    }

    /// The spread relative to the mean, as `variance / mean²`.
    ///
    /// Raw variance is scale-dependent: `[4, 4, 4, 6]` and `[10, 10, 10, 15]` are equally ragged
    /// in proportion, but the second scores over six times higher purely because its lines are
    /// longer. `SPEC.md` §4.1 compares stanzas that may differ in line length, so the raw figure
    /// would systematically call long-lined stanzas more aperiodic than short-lined ones of the
    /// same regularity.
    ///
    /// Still exact, and still no square root — a coefficient of variation would need one, and
    /// would put a float back into the comparison that decides whether a stanza becomes a drum
    /// loop or a fill.
    ///
    /// Zero-mean sets (every line empty) have no meaningful relative spread and return zero.
    #[must_use]
    pub fn relative(counts: &[usize]) -> Self {
        let n = counts.len() as u128;
        let sum: u128 = counts.iter().map(|&c| c as u128).sum();
        if n == 0 || sum == 0 {
            return Self {
                numerator: 0,
                denominator: 1,
            };
        }
        // variance / mean² = (n·Σx² − (Σx)²) / n² ÷ ((Σx)/n)² = (n·Σx² − (Σx)²) / (Σx)²
        let sum_squares: u128 = counts.iter().map(|&c| (c as u128) * (c as u128)).sum();
        Self {
            numerator: n * sum_squares - sum * sum,
            denominator: sum * sum,
        }
    }

    /// A variance of exactly `numerator / denominator`, for expressing a threshold to compare
    /// measurements against.
    ///
    /// Not a measurement, and deliberately **not public**. The private fields are what make a
    /// `Variance` in hand evidence that something was counted — the same property `CLAUDE.md` §5
    /// requires of [`Pronunciation::guessed`](crate::phoneme::Pronunciation::guessed). A cutoff
    /// is the one value that legitimately comes from nowhere, and [`crate::role::PERIODIC`] is
    /// the only one. Its sibling [`crate::rhyme::RHYME_THRESHOLD`] needs no constructor at all
    /// because it sits in the module that owns the fields; this one does not.
    ///
    /// Keeping it crate-internal also closes two holes. `Ord` cross-multiplies without a guard,
    /// so a fabricated numerator near `u128::MAX` would overflow — panicking in debug and, worse,
    /// silently inverting the comparison in release, where the exact integer representation
    /// exists precisely to guarantee a total order. And the `assert!` below is a *runtime* panic
    /// in any non-`const` call, which `clippy::panic` does not catch. With one `const` call site
    /// both are unreachable.
    ///
    /// # Panics
    ///
    /// If `denominator` is zero. Every call site is a `const` initialiser, so this is a
    /// compile-time error in practice.
    #[must_use]
    pub(crate) const fn threshold(numerator: u128, denominator: u128) -> Self {
        assert!(denominator != 0, "a threshold needs a non-zero denominator");
        Self {
            numerator,
            denominator,
        }
    }

    /// Whether this is zero — a perfectly regular set.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.numerator == 0
    }

    /// The value, for display only.
    ///
    /// Never compare two of these: use the [`Ord`] implementation, which is exact.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // display only; ordering uses the exact rational
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl PartialEq for Variance {
    fn eq(&self, other: &Self) -> bool {
        self.numerator * other.denominator == other.numerator * self.denominator
    }
}

impl Eq for Variance {}

impl core::hash::Hash for Variance {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        // Hash the reduced form, so equal values hash equally.
        let divisor = gcd(self.numerator, self.denominator).max(1);
        (self.numerator / divisor).hash(state);
        (self.denominator / divisor).hash(state);
    }
}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    a
}

/// Exact comparison by cross-multiplication, so no float ever decides an ordering.
impl Ord for Variance {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }
}

impl PartialOrd for Variance {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// A stanza's meter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanzaMeter {
    span: Span,
    lines: Vec<LineMeter>,
    syllable_variance: Variance,
    relative_syllable_variance: Variance,
    foot_variance: Variance,
    relative_foot_variance: Variance,
}

impl StanzaMeter {
    /// The stanza's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The stanza's lines.
    #[must_use]
    pub fn lines(&self) -> &[LineMeter] {
        &self.lines
    }

    /// How much the lines' syllable counts vary, in syllables squared.
    ///
    /// **Raw**, so it grows with line length: `[8,6,8,6]` scores 1.0 where
    /// [`StanzaMeter::relative_syllable_variance`] scores 1/49. Only ever compare this against
    /// another *raw* variance — `role::PERIODIC` is a relative one, and testing this against it
    /// calls common measure ragged.
    #[must_use]
    pub const fn syllable_variance(&self) -> Variance {
        self.syllable_variance
    }

    /// How much the lines' foot counts vary, in feet squared.
    ///
    /// **Raw**, like [`StanzaMeter::syllable_variance`]; see the warning there.
    #[must_use]
    pub const fn foot_variance(&self) -> Variance {
        self.foot_variance
    }

    /// Syllable-count spread relative to line length.
    ///
    /// The figure to compare *across* stanzas, since [`Variance::of`] grows with line length even
    /// when the raggedness does not. See [`Variance::relative`].
    #[must_use]
    pub const fn relative_syllable_variance(&self) -> Variance {
        self.relative_syllable_variance
    }

    /// Foot-count spread relative to line length.
    ///
    /// The pair of [`StanzaMeter::relative_syllable_variance`], and what
    /// [`crate::role::assign`] compares against [`crate::role::PERIODIC`]. Both figures live here
    /// rather than being recomputed by the role pass, so there is one implementation of each
    /// statistic and no way for two copies to drift apart.
    #[must_use]
    pub const fn relative_foot_variance(&self) -> Variance {
        self.relative_foot_variance
    }
}

/// A poem's meter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoemMeter {
    poem: Option<Seed>,
    stanzas: Vec<StanzaMeter>,
}

impl PoemMeter {
    /// The poem's stanzas.
    #[must_use]
    pub fn stanzas(&self) -> &[StanzaMeter] {
        &self.stanzas
    }

    /// Which poem this describes; see [`Poem::fingerprint`](crate::parse::Poem::fingerprint).
    #[must_use]
    pub const fn poem(&self) -> Option<Seed> {
        self.poem
    }
}

/// Scans a poem.
///
/// Pure: it reads the parsed poem and asks `phonemiser` how the words sound, and does nothing
/// else. Words the phonemiser cannot pronounce are recorded on the line rather than dropped.
#[must_use]
pub fn scan(poem: &Poem, phonemiser: &impl Phonemiser) -> PoemMeter {
    PoemMeter {
        poem: poem.fingerprint(),
        stanzas: poem
            .stanzas()
            .iter()
            .map(|stanza| scan_stanza(stanza, poem.source(), phonemiser))
            .collect(),
    }
}

fn scan_stanza(stanza: &Stanza, source: &str, phonemiser: &impl Phonemiser) -> StanzaMeter {
    let lines: Vec<LineMeter> = stanza
        .lines()
        .iter()
        .map(|line| scan_line(line, source, phonemiser))
        .collect();
    let syllables: Vec<usize> = lines.iter().map(|l| l.syllable_count().get()).collect();
    let feet: Vec<usize> = lines.iter().map(|l| l.foot_count().get()).collect();
    StanzaMeter {
        span: stanza.span(),
        syllable_variance: Variance::of(&syllables),
        relative_syllable_variance: Variance::relative(&syllables),
        foot_variance: Variance::of(&feet),
        relative_foot_variance: Variance::relative(&feet),
        lines,
    }
}

fn scan_line(line: &Line, source: &str, phonemiser: &impl Phonemiser) -> LineMeter {
    let mut beats = Vec::new();
    let mut unpronounced = Vec::new();

    for token in line.words() {
        let span = token.span();
        let Some(text) = span.text(source) else {
            unpronounced.push(span);
            continue;
        };
        match phonemiser.pronounce(text) {
            // An `Ok` carrying no syllables contributes nothing, so it must be recorded like any
            // other failure. Neither shipped phonemiser can produce one — both route through
            // `syllabify`, which errors instead — but `Phonemiser` is the documented extension
            // point for a second language, and a word that appeared in no list at all would make
            // a line look confidently short.
            Ok(pronunciation) if pronunciation.syllables().is_empty() => unpronounced.push(span),
            Ok(pronunciation) => {
                let provenance = pronunciation.provenance();
                for (index, syllable) in pronunciation.syllables().iter().enumerate() {
                    beats.push(Beat {
                        word: span,
                        index,
                        stress: syllable.stress(),
                        provenance,
                    });
                }
            }
            Err(_) => unpronounced.push(span),
        }
    }

    let feet = segment(&beats);
    LineMeter {
        span: line.span(),
        beats,
        feet,
        unpronounced,
    }
}

/// One cell of the segmentation search: the best way found to cover a prefix of the line.
#[derive(Clone, Copy)]
struct Step {
    /// Syllables disagreeing with their foot's template. Minimised first.
    cost: usize,
    /// How far from the line's end any truncated feet sit. Breaks ties in `cost`.
    truncation: usize,
    /// How many feet. Breaks ties in the two above.
    feet: usize,
    from: usize,
    /// The foot ending here. Inert at index 0, which is the empty prefix rather than a foot.
    kind: FootKind,
}

/// Whether a syllable counts as stressed *for scansion*.
///
/// Primary stress only. `Stress::is_stressed` also counts secondary, which is right for rhyme —
/// "cavalcade" rhymes with "brigade" on a secondary-stressed syllable — but wrong for meter,
/// where a foot is defined by *relative* prominence. Counting secondary as strong turns
/// "intervene" (`IH2 N T ER0 V IY1 N`) from an anapest into a trochee-plus-single and makes
/// anapestic verse unscannable.
///
/// The three-way value stays on every [`Beat`], because `SPEC.md` §4.2 maps stress to accent and
/// velocity, where the distinction does matter.
const fn carries_the_beat(stress: Stress) -> bool {
    matches!(stress, Stress::Primary)
}

/// Divides a stress vector into feet.
///
/// Dynamic programming: the segmentation that disagrees least with the classical templates wins.
/// A greedy left-to-right scan would mis-scan any line whose first foot is substituted, which is
/// most English verse — "Batter my heart" opens with a trochee and continues in iambs, and a
/// greedy reading of it never recovers.
///
/// Ties are broken by fewer feet, then by [`FootKind::ALL`]'s order. Short lines tie constantly,
/// and invariant I1 requires the answer not to depend on the order the search happened to take.
fn segment(beats: &[Beat]) -> Vec<Foot> {
    if beats.is_empty() {
        return Vec::new();
    }
    let stressed: Vec<bool> = beats.iter().map(|b| carries_the_beat(b.stress)).collect();

    // best[i] = the best way to cover the first i syllables.
    let mut best: Vec<Option<Step>> = vec![None; stressed.len() + 1];
    if let Some(start) = best.first_mut() {
        *start = Some(Step {
            cost: 0,
            truncation: 0,
            feet: 0,
            from: 0,
            // Never read as a real foot: the walk-back stops before index 0.
            kind: FootKind::Single,
        });
    }

    for end in 1..=stressed.len() {
        for kind in FootKind::ALL {
            let width = kind.syllables();
            let Some(start) = end.checked_sub(width) else {
                continue;
            };
            let Some(Some(previous)) = best.get(start).copied() else {
                continue;
            };
            let Some(actual) = stressed.get(start..end) else {
                continue;
            };
            let mismatch = actual
                .iter()
                .zip(kind.template())
                .filter(|(a, b)| a != b)
                .count();
            // A truncated foot belongs at the end of a line, not the start: a trochaic line of
            // seven syllables is three trochees and a catalectic final foot, not a stray syllable
            // followed by iambs.
            //
            // This is a *tie-break*, ranked below mismatch — an earlier version added it straight
            // into the cost, which let it override tilings that fit strictly better. Measured on
            // every stress pattern up to length 14, that made 13–16% of odd-length lines scan
            // worse than they needed to: `S S w` took a dactyl at one mismatch rather than
            // `Single` + trochee at none.
            let truncation = if kind == FootKind::Single {
                stressed.len() - end
            } else {
                0
            };
            let candidate = Step {
                cost: previous.cost + mismatch,
                truncation: previous.truncation + truncation,
                feet: previous.feet + 1,
                from: start,
                kind,
            };
            let replace = match best.get(end).copied().flatten() {
                None => true,
                Some(current) => {
                    (candidate.cost, candidate.truncation, candidate.feet)
                        < (current.cost, current.truncation, current.feet)
                }
            };
            if replace && let Some(slot) = best.get_mut(end) {
                *slot = Some(candidate);
            }
        }
    }

    // Walk the choices back to the start.
    let mut feet = Vec::new();
    let mut end = stressed.len();
    while end > 0 {
        let Some(Some(step)) = best.get(end).copied() else {
            break;
        };
        let start = step.from;
        let span = beats
            .get(start..end)
            .and_then(|run| run.iter().map(|b| b.word).reduce(Span::join));
        if let Some(span) = span {
            feet.push(Foot {
                kind: step.kind,
                span,
                beats: start..end,
            });
        }
        end = start;
    }
    feet.reverse();
    feet
}

#[cfg(test)]
mod tests {
    use super::{FootKind, LineMeter, PoemMeter, Variance, scan};
    use crate::parse::Poem;
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Provenance, Stress, Syllable, Unpronounceable,
        syllabify,
    };

    /// A phonemiser driven by a table, so meter tests do not depend on a dictionary.
    struct Fake(&'static [(&'static str, &'static [(&'static str, Stress)])]);

    impl Phonemiser for Fake {
        fn inventory(&self) -> &'static Inventory {
            &ARPABET
        }

        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            let lower = word.to_lowercase();
            let entry = self
                .0
                .iter()
                .find(|(w, _)| *w == lower)
                .ok_or(Unpronounceable::NotKnown)?;
            let phones: Vec<_> = entry
                .1
                .iter()
                .map(|(symbol, stress)| (ARPABET.phoneme(symbol).unwrap(), *stress))
                .collect();
            Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
        }
    }

    const W: Stress = Stress::Unstressed;
    const S: Stress = Stress::Primary;

    /// Every word here is one or two syllables with known stress, so a line's stress vector is
    /// whatever the test says it is.
    static WORDS: &[(&str, &[(&str, Stress)])] = &[
        ("the", &[("DH", W), ("AH", W)]),
        ("a", &[("AH", W)]),
        ("in", &[("IH", W), ("N", W)]),
        ("of", &[("AH", W), ("V", W)]),
        ("to", &[("T", W), ("UW", W)]),
        ("and", &[("AE", W), ("N", W), ("D", W)]),
        ("tide", &[("T", W), ("AY", S), ("D", W)]),
        ("burn", &[("B", W), ("ER", S), ("N", W)]),
        ("fire", &[("F", W), ("AY", S), ("R", W)]),
        ("dark", &[("D", W), ("AA", S), ("R", W), ("K", W)]),
        ("night", &[("N", W), ("AY", S), ("T", W)]),
        ("heart", &[("HH", W), ("AA", S), ("R", W), ("T", W)]),
        ("break", &[("B", W), ("R", W), ("EY", S), ("K", W)]),
        // Two syllables, stress first: BURN-ing.
        (
            "burning",
            &[("B", W), ("ER", S), ("N", W), ("IH", W), ("NG", W)],
        ),
        (
            "tender",
            &[("T", W), ("EH", S), ("N", W), ("D", W), ("ER", W)],
        ),
        // Two syllables, stress second: a-LIVE.
        ("alive", &[("AH", W), ("L", W), ("AY", S), ("V", W)]),
        // Three syllables with SECONDARY stress on the first, primary on the last — the shape of
        // "intervene" (IH2 N T ER0 V IY1 N). Reads as an anapest only because scansion counts
        // primary stress alone.
        (
            "intervene",
            &[
                ("IH", Stress::Secondary),
                ("N", W),
                ("T", W),
                ("ER", W),
                ("V", W),
                ("IY", S),
                ("N", W),
            ],
        ),
        (
            "remain",
            &[("R", W), ("IH", W), ("M", W), ("EY", S), ("N", W)],
        ),
    ];

    fn scan_poem(source: &str) -> PoemMeter {
        let poem = Poem::parse(source).unwrap();
        scan(&poem, &Fake(WORDS))
    }

    fn line_of(meter: &PoemMeter, stanza: usize, line: usize) -> &LineMeter {
        &meter.stanzas()[stanza].lines()[line]
    }

    fn stresses(line: &LineMeter) -> Vec<bool> {
        line.beats()
            .iter()
            .map(|b| b.stress.is_stressed())
            .collect()
    }

    /// The binary scansion actually runs on: primary stress only.
    fn scansion(line: &LineMeter) -> Vec<bool> {
        line.beats()
            .iter()
            .map(|b| b.stress == Stress::Primary)
            .collect()
    }

    fn kinds(line: &LineMeter) -> Vec<&'static str> {
        line.feet().iter().map(|f| f.kind().name()).collect()
    }

    #[test]
    fn a_stress_vector_follows_the_words() {
        let meter = scan_poem("the tide of dark\n");
        let line = line_of(&meter, 0, 0);
        // the(w) tide(S) of(w) dark(S)
        assert_eq!(stresses(line), [false, true, false, true]);
        assert_eq!(line.syllable_count().get(), 4);
    }

    #[test]
    fn iambic_pentameter_yields_five_feet_not_four() {
        // SPEC.md §4.2 names this exactly: five beats, and no rounding to 4/4.
        let meter = scan_poem("the tide of dark to burn a night in fire\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(line.syllable_count().get(), 10);
        assert_eq!(line.foot_count().get(), 5);
        assert_eq!(kinds(line), ["iamb"; 5]);
        assert_eq!(line.dominant_foot(), Some(FootKind::Iamb));
    }

    #[test]
    fn trochaic_lines_scan_as_trochees() {
        // BURN-ing TEN-der BURN-ing TEN-der
        let meter = scan_poem("burning tender burning tender\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(
            stresses(line),
            [true, false, true, false, true, false, true, false]
        );
        assert_eq!(kinds(line), ["trochee"; 4]);
    }

    #[test]
    fn anapests_scan_as_anapests() {
        // in the DARK / in the DARK
        let meter = scan_poem("in the dark in the dark\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(
            stresses(line),
            [false, false, true, false, false, true],
            "two anapests"
        );
        assert_eq!(kinds(line), ["anapest", "anapest"]);
    }

    #[test]
    fn secondary_stress_does_not_carry_the_beat() {
        // Counting secondary stress as strong turns each of these from an anapest into a
        // trochee-plus-single, and makes anapestic verse unscannable. Guarded here rather than
        // only in the real-verse tests of another crate, where it was the sole thing catching it.
        let meter = scan_poem("intervene intervene\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(
            scansion(line),
            [false, false, true, false, false, true],
            "secondary stress reads as weak for scansion"
        );
        // The same syllables count as stressed for rhyme, which is why the two must differ.
        assert_eq!(
            stresses(line),
            [true, false, true, true, false, true],
            "secondary stress is still prominence, for rhyme"
        );
        assert_eq!(kinds(line), ["anapest", "anapest"]);
        // ...but the three-way value survives on the beat, for velocity mapping (SPEC.md §4.2).
        assert_eq!(line.beats()[0].stress, Stress::Secondary);
        assert!(
            line.beats()[0].stress.is_stressed(),
            "still stressed for rhyme"
        );
    }

    #[test]
    fn a_truncated_foot_lands_at_the_end_of_the_line() {
        // Seven syllables of trochaic verse: three trochees and a catalectic final foot, not a
        // stray syllable followed by iambs. Both readings cost zero mismatches, so only the
        // tie-break separates them — and without it every trochaic line reads as iambic.
        let meter = scan_poem("burning tender burning burn\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(
            scansion(line),
            [true, false, true, false, true, false, true]
        );
        assert_eq!(kinds(line), ["trochee", "trochee", "trochee", "single"]);
        assert_eq!(line.dominant_foot(), Some(FootKind::Trochee));
    }

    #[test]
    fn truncation_never_overrides_a_better_fitting_scansion() {
        // The tie-break is ranked below mismatch. An earlier version added it into the cost,
        // which let it prefer a one-mismatch dactyl over a perfect Single + trochee.
        let meter = scan_poem("heart break burning\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(scansion(line), [true, true, true, false]);
        // Whatever it chooses, it must not pay a mismatch to avoid a leading Single.
        let mismatches: usize = line
            .feet()
            .iter()
            .map(|foot| {
                let beats = line.beats_of(foot).unwrap();
                beats
                    .iter()
                    .zip(foot.kind().template())
                    .filter(|(b, want)| (b.stress == Stress::Primary) != **want)
                    .count()
            })
            .sum();
        assert_eq!(mismatches, 0, "{:?}", kinds(line));
    }

    #[test]
    fn a_syllable_knows_its_position_in_its_word() {
        let meter = scan_poem("burning tide\n");
        let line = line_of(&meter, 0, 0);
        let positions: Vec<usize> = line.beats().iter().map(|b| b.index).collect();
        // "burning" gives syllables 0 and 1; "tide" gives its syllable 0.
        assert_eq!(positions, [0, 1, 0]);
    }

    #[test]
    fn feet_may_share_a_word_that_straddles_them() {
        // "the BURN-ing TIDE": "burning" gives one syllable to each of two feet, so both feet's
        // spans contain it. That overlap is the poem's, not an ambiguity introduced here — and
        // per-syllable attribution stays exact through `beats_of`.
        let source = "the burning tide\n";
        let meter = scan_poem(source);
        let line = line_of(&meter, 0, 0);
        assert_eq!(line.feet().len(), 2);
        let first = &line.feet()[0];
        let second = &line.feet()[1];
        assert_eq!(first.span().text(source), Some("the burning"));
        assert_eq!(second.span().text(source), Some("burning tide"));
        // Exact attribution: the shared word gives syllable 0 to one foot and syllable 1 to the
        // other, and the beat ranges do not overlap.
        assert_eq!(first.beats().end, second.beats().start);
        assert_eq!(line.beats_of(first).unwrap()[1].index, 0);
        assert_eq!(line.beats_of(second).unwrap()[0].index, 1);
    }

    #[test]
    fn beats_of_declines_a_foot_from_another_line() {
        let long = scan_poem("the tide of dark to burn a night\n");
        let short = scan_poem("burn\n");
        let far = &line_of(&long, 0, 0).feet()[3];
        assert!(line_of(&short, 0, 0).beats_of(far).is_none());
    }

    #[test]
    fn a_pronunciation_with_no_syllables_is_recorded_like_any_other_failure() {
        // `Phonemiser` is the extension point for a second language, and nothing in the trait
        // forbids an `Ok` carrying no syllables. Such a word must not vanish from both lists.
        struct Silent;
        impl Phonemiser for Silent {
            fn inventory(&self) -> &'static Inventory {
                &ARPABET
            }
            fn pronounce(&self, _: &str) -> Result<Pronunciation, Unpronounceable> {
                Ok(Pronunciation::from_dictionary(Vec::new()))
            }
        }
        let source = "the silence sings\n";
        let poem = Poem::parse(source).unwrap();
        let meter = scan(&poem, &Silent);
        let line = &meter.stanzas()[0].lines()[0];
        assert_eq!(line.syllable_count().get(), 0);
        assert_eq!(line.unpronounced().len(), 3, "every word accounted for");
    }

    #[test]
    fn variance_equality_agrees_with_ordering() {
        // A derived `PartialEq` compares the stored fraction, so equal spreads at different
        // stanza lengths compared unequal while `cmp` called them equal — breaking the contract
        // on the exact comparison this type exists to make.
        let pairs = [
            (Variance::of(&[1, 3]), Variance::of(&[1, 1, 3, 3])),
            (Variance::of(&[]), Variance::of(&[2, 2])),
            (Variance::of(&[5, 5, 5]), Variance::of(&[9, 9])),
        ];
        for (a, b) in pairs {
            assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal, "{a:?} vs {b:?}");
            assert_eq!(a, b, "equality must agree with ordering");
        }
    }

    #[test]
    fn relative_variance_ignores_line_length() {
        // Raw variance grows with line length even when the raggedness does not, which would make
        // SPEC.md §4.1 call long-lined stanzas more aperiodic than short ones of equal regularity.
        let short = [4, 4, 4, 6];
        let long = [10, 10, 10, 15];
        assert!(
            Variance::of(&short) < Variance::of(&long),
            "raw variance is scale-dependent"
        );
        assert_eq!(
            Variance::relative(&short),
            Variance::relative(&long),
            "relative variance is not"
        );
        assert!(Variance::relative(&[4, 4, 4]).is_zero());
        assert!(Variance::relative(&[]).is_zero());
        assert!(
            Variance::relative(&[0, 0]).is_zero(),
            "a zero mean has no relative spread"
        );
    }

    #[test]
    fn a_spondee_is_recognised() {
        let meter = scan_poem("heart break\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(stresses(line), [true, true]);
        assert_eq!(kinds(line), ["spondee"]);
    }

    #[test]
    fn a_substituted_first_foot_does_not_derail_the_rest() {
        // BURN-ing the TIDE of DARK to BURN — a trochee, then iambs. A greedy left-to-right scan
        // reads the whole line wrong from the first foot onward.
        let meter = scan_poem("burning the tide of dark to burn\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(
            stresses(line),
            [true, false, false, true, false, true, false, true]
        );
        assert_eq!(kinds(line), ["trochee", "iamb", "iamb", "iamb"]);
        assert_eq!(line.dominant_foot(), Some(FootKind::Iamb));
    }

    #[test]
    fn an_odd_syllable_count_still_scans() {
        // Five syllables cannot be tiled by two- and three-syllable feet in every stress pattern,
        // so a single-syllable foot exists to finish the job.
        let meter = scan_poem("the tide of dark burn\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(line.syllable_count().get(), 5);
        assert!(line.foot_count().get() > 0);
        let covered: usize = line.feet().iter().map(|f| f.beats().len()).sum();
        assert_eq!(covered, 5, "every syllable belongs to exactly one foot");
    }

    #[test]
    fn every_syllable_is_attributable_to_its_word() {
        let source = "the tide of dark\n";
        let meter = scan_poem(source);
        let line = line_of(&meter, 0, 0);
        let words: Vec<&str> = line
            .beats()
            .iter()
            .filter_map(|b| b.word.text(source))
            .collect();
        assert_eq!(words, ["the", "tide", "of", "dark"]);
        // And every foot's span covers the words its syllables came from.
        for foot in line.feet() {
            for beat in &line.beats()[foot.beats()] {
                assert!(foot.span.contains(beat.word), "{foot:?} misses {beat:?}");
            }
        }
    }

    #[test]
    fn an_unpronounceable_word_is_recorded_not_dropped() {
        let source = "the krapplesnaffle tide\n";
        let meter = scan_poem(source);
        let line = line_of(&meter, 0, 0);
        // Only "the" and "tide" contributed syllables...
        assert_eq!(line.syllable_count().get(), 2);
        // ...and the line says so, rather than looking like a short line.
        assert_eq!(line.unpronounced().len(), 1);
        assert_eq!(line.unpronounced()[0].text(source), Some("krapplesnaffle"));
    }

    #[test]
    fn a_scansion_says_whether_it_rests_on_a_guess() {
        struct Guesser;
        impl Phonemiser for Guesser {
            fn inventory(&self) -> &'static Inventory {
                &ARPABET
            }
            fn pronounce(&self, _: &str) -> Result<Pronunciation, Unpronounceable> {
                let nucleus = ARPABET.phoneme("AH").unwrap();
                let syllable = Syllable::new(vec![], nucleus, vec![], Stress::Primary).unwrap();
                Ok(Pronunciation::guessed(vec![syllable]))
            }
        }
        let poem = Poem::parse("anything at all\n").unwrap();
        let meter = scan(&poem, &Guesser);
        let line = &meter.stanzas()[0].lines()[0];
        assert!(line.rests_on_a_guess());
        assert!(
            line.beats()
                .iter()
                .all(|b| b.provenance == Provenance::LetterToSound)
        );

        let dictionary = scan_poem("the tide\n");
        assert!(!line_of(&dictionary, 0, 0).rests_on_a_guess());
    }

    #[test]
    fn an_empty_line_has_no_feet() {
        // Every word unpronounceable: no syllables, so no scansion, and nothing pretends otherwise.
        let meter = scan_poem("krapplesnaffle\n");
        let line = line_of(&meter, 0, 0);
        assert_eq!(line.syllable_count().get(), 0);
        assert_eq!(line.foot_count().get(), 0);
        assert_eq!(line.dominant_foot(), None);
        assert_eq!(line.unpronounced().len(), 1);
    }

    #[test]
    fn a_regular_stanza_has_zero_variance() {
        // Two lines of four syllables each: perfectly periodic, which SPEC.md §4.1 reads as a
        // foundation layer.
        let meter = scan_poem("the tide of dark\nthe fire of night\n");
        let stanza = &meter.stanzas()[0];
        assert!(stanza.syllable_variance().is_zero());
        assert!(stanza.foot_variance().is_zero());
    }

    #[test]
    fn a_ragged_stanza_has_higher_variance_than_a_regular_one() {
        let regular = scan_poem("the tide of dark\nthe fire of night\n");
        let ragged = scan_poem("burn\nthe tide of dark to burn a night in fire\n");
        assert!(ragged.stanzas()[0].syllable_variance() > regular.stanzas()[0].syllable_variance());
    }

    #[test]
    fn variance_is_exact_and_needs_no_float() {
        // Population variance of [1, 3] is 1; of [2, 2] is 0; of [1, 2, 3] is 2/3.
        assert!(Variance::of(&[2, 2]).is_zero());
        assert!((Variance::of(&[1, 3]).as_f64() - 1.0).abs() < 1e-12);
        assert!((Variance::of(&[1, 2, 3]).as_f64() - 2.0 / 3.0).abs() < 1e-12);

        // Ordering is exact, including for values a float would round together.
        assert!(Variance::of(&[1, 2, 3]) < Variance::of(&[1, 3]));
        assert_eq!(Variance::of(&[5]), Variance::of(&[7]));
        assert!(Variance::of(&[]).is_zero());
    }

    #[test]
    fn variance_compares_across_different_stanza_lengths() {
        // Three even lines are more regular than two uneven ones, even though the sample sizes
        // differ — the comparison is a cross-multiplication, not a same-denominator shortcut.
        let even = Variance::of(&[4, 4, 4]);
        let uneven = Variance::of(&[2, 6]);
        assert!(even < uneven);
    }

    #[test]
    fn scanning_is_deterministic() {
        let source = "burning the tide of dark to burn\nin the dark in the dark\n";
        assert_eq!(scan_poem(source), scan_poem(source));
    }
}
