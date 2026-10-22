//! The bass: `SPEC.md` §4.6, "The bass", and its walking variants.
//!
//! §4.1 calls the foundation "drums, bass, ostinato". [`Mapping::Bass`](crate::mapping::Mapping)
//! is a selector with three ways to give a foundation line its bass, from `PLAN.md`'s proposal
//! "the structural half of jazz" (the user approved W1 and W2 as candidates to compare by ear):
//!
//! | Variant | Notes on | Each note is |
//! |---|---|---|
//! | [`Variant::Height`] (default) | every primary-stressed syllable not in a rhyme word | its vowel's height: open root, mid third, close fifth (*machine cycle 1*) |
//! | [`Variant::Syllables`] (W1) | every syllable of the line | a stressed one keeps its height note; an unstressed one between two stressed ones is a scale step from the first towards the second; the last unstressed one before a stressed one is a semitone below that one's note |
//! | [`Variant::Feet`] (W2) | the first syllable of every foot — on the beat | the chord tone the foot's stressed vowel's height chooses (root, third, fifth of the bar's chord, or of the scale with no chord); where that would repeat the walk's last note, a scale step instead, up if the next foot's vowel is higher and down otherwise |
//!
//! # Why a bass note is not a [`Pitch`](crate::voice::Pitch)
//!
//! Until the walking variants, the bass and the melody shared [`Event::pitch`] and the melody won
//! on a rhyme word, so a rhyme word had no bass. A walk that falls silent on every line's last word
//! is not a walk, and W2's note on a foot's first syllable would often *be* the rhyme word. So a
//! bass note is its own field, [`Event::bass`]: one event can carry a melody note and a bass note,
//! and the two layers play both. [`Variant::Height`] keeps today's rule — no bass on a rhyme word —
//! so the default piece is the piece it was.
//!
//! # Every bass note traces to the poem (I4)
//!
//! A height or W1 note's span is its syllable's word (as far as attribution can reach, see
//! [`Event::span`]); a W2 note's span is its *foot* ([`Foot::span`](crate::meter::Foot::span)),
//! coarser than a syllable, as the proposal said it would be. Nothing here crosses a line: a walk
//! starts and ends inside its line, so an edit to another line can change a line's bass only
//! through what it already depended on — whether its stanza is foundation (§4.1), whether a word
//! is a rhyme word (height only), and, under W2 with the harmony on, the bar's chord (§4.6, whose
//! roots are §4.5's poem-wide degree numbering).
//!
//! # Structure here, register in the emitter
//!
//! A note is a [`ScaleStep`] above a root [`ScaleDegree`] — the bar's chord's root under W2, 0
//! otherwise — and possibly a semitone below it ([`Chromatic::SemitoneBelow`], W1's approach
//! tone). Which scale those steps index, from which root note, in which octave, is palette: the
//! emitter folds the chord's root into one octave as the chord lane does, and a step below the root
//! sounds below it.

use crate::harmony::{Chord, Harmony};
use crate::lower::Analyses;
use crate::mapping::Variant;
use crate::phoneme::Phoneme;
use crate::role::Layer;
use crate::span::Span;
use crate::voice::{Event, ScaleDegree, Velocity};

/// A signed number of scale steps from a root: 2 is the third above it, −1 the step below it.
///
/// Signed because a walk passes below its root (W1's tail, W2's step down from the root), and
/// steps rather than semitones because which scale is palette (`crate::voice`'s "why nothing here
/// has a unit").
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScaleStep(i32);

impl ScaleStep {
    /// The number of steps; negative below the root.
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    /// One step further in `direction`.
    const fn moved(self, direction: Direction) -> Self {
        match direction {
            Direction::Up => Self(self.0.saturating_add(1)),
            Direction::Down => Self(self.0.saturating_sub(1)),
        }
    }

    /// `steps` steps further, signed.
    const fn plus(self, steps: i32) -> Self {
        Self(self.0.saturating_add(steps))
    }
}

/// A vowel's height, the property the bass reads: close vowels carry a higher intrinsic pitch
/// than open ones (Lehiste 1970), so a higher vowel is a higher note.
///
/// Ordered open < mid < close, so "the next vowel is higher" is `>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Height {
    /// `AA AE AO AW AY`: the root.
    Open,
    /// `AH EH ER EY OW OY`: the third.
    Mid,
    /// `IH IY UH UW`: the fifth.
    Close,
}

impl Height {
    /// An `ARPAbet` vowel's height, diphthongs by their starting point. `None` for a vowel of
    /// another inventory, which therefore gets no bass note rather than a guess.
    #[must_use]
    pub fn of(vowel: Phoneme) -> Option<Self> {
        match vowel.symbol() {
            "AA" | "AE" | "AO" | "AW" | "AY" => Some(Self::Open),
            "AH" | "EH" | "ER" | "EY" | "OW" | "OY" => Some(Self::Mid),
            "IH" | "IY" | "UH" | "UW" => Some(Self::Close),
            _ => None,
        }
    }

    /// The chord tone it chooses, in steps above the root: root 0, third 2, fifth 4. Three
    /// heights, three tones, because a machine bass is a riff of a few notes (`SPEC.md` §4.6).
    #[must_use]
    pub const fn tone(self) -> ScaleStep {
        match self {
            Self::Open => ScaleStep(0),
            Self::Mid => ScaleStep(2),
            Self::Close => ScaleStep(4),
        }
    }

    /// Its name in the debug view.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Mid => "mid",
            Self::Close => "close",
        }
    }

    /// The chord tone's name in the debug view.
    #[must_use]
    pub const fn tone_name(self) -> &'static str {
        match self {
            Self::Open => "root",
            Self::Mid => "third",
            Self::Close => "fifth",
        }
    }
}

/// Whether a note sits on its scale step or a semitone below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Chromatic {
    /// On the step.
    Diatonic,
    /// A semitone below the step: W1's chromatic approach tone.
    SemitoneBelow,
}

/// Which way a step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Direction {
    /// Up a step.
    Up,
    /// Down a step.
    Down,
}

/// Why a bass note is the note it is: which rule of which variant decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassReason {
    /// A primary-stressed syllable, by its vowel's [`Height`]: the height bass, and W1's stressed
    /// syllables.
    Height(Phoneme),
    /// W1: an unstressed syllable between two stressed ones, a scale step on the way from the
    /// first's note towards the second's — holding one step short of it, or on the upper
    /// neighbour when the two are the same note.
    Passing {
        /// The previous stressed syllable's note.
        from: ScaleStep,
        /// The next stressed syllable's note.
        toward: ScaleStep,
    },
    /// W1: the last unstressed syllable before a stressed one, a semitone below that one's note.
    Approach {
        /// The stressed syllable's note it leads into.
        onto: ScaleStep,
    },
    /// W1: an unstressed syllable before the line's first stressed one (and not the approach), a
    /// scale step below where it leads, climbing into it.
    LeadIn {
        /// The first stressed syllable's note.
        toward: ScaleStep,
    },
    /// W1: an unstressed syllable after the line's last stressed one, stepping down from it.
    Tail {
        /// The last stressed syllable's note.
        from: ScaleStep,
    },
    /// W2: the foot's stressed vowel's height chose this tone of the bar's chord — or of the scale,
    /// when the bar has no chord.
    ChordTone {
        /// The foot's stressed vowel.
        vowel: Phoneme,
        /// The bar's chord, if it has one.
        chord: Option<Chord>,
    },
    /// W2: the foot's chord tone is the note the walk is already on, so it steps instead — up if
    /// the next foot's vowel is higher, down if it is lower, the same or there is no next foot.
    Step {
        /// The foot's stressed vowel, whose tone would have repeated.
        vowel: Phoneme,
        /// The bar's chord, if it has one.
        chord: Option<Chord>,
        /// Which way it stepped.
        direction: Direction,
        /// The next foot's stressed vowel, which chose the direction, if there is a next foot.
        next: Option<Phoneme>,
    },
}

/// One note of the bass line, carried by the event it sounds with ([`Event::bass`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BassNote {
    span: Span,
    root: ScaleDegree,
    step: ScaleStep,
    chromatic: Chromatic,
    velocity: Velocity,
    why: BassReason,
}

impl BassNote {
    /// What it traces to (I4): its syllable's word, or under W2 its whole foot.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// The degree its steps count from: the bar's chord's root under W2, 0 otherwise. Counted like
    /// a chord's, so it may run past the scale's length; the emitter folds it.
    #[must_use]
    pub const fn root(self) -> ScaleDegree {
        self.root
    }

    /// How many scale steps above [`BassNote::root`] it sits.
    #[must_use]
    pub const fn step(self) -> ScaleStep {
        self.step
    }

    /// On the step, or a semitone below it.
    #[must_use]
    pub const fn chromatic(self) -> Chromatic {
        self.chromatic
    }

    /// How hard it is struck: its syllable's stress, or under W2 its foot's stressed syllable's
    /// (`SPEC.md` §4.2's stress → accent).
    #[must_use]
    pub const fn velocity(self) -> Velocity {
        self.velocity
    }

    /// Which rule chose it.
    #[must_use]
    pub const fn why(self) -> BassReason {
        self.why
    }
}

/// Gives the foundation lines their bass under `variant` — nothing at all when it is
/// [`Variant::Off`].
///
/// `nuclei` is each event's vowel, index for index; `harmony` the harmony layer's entry per line
/// (empty when it is off), which only [`Variant::Feet`] reads.
pub(crate) fn run(
    events: &mut [Event],
    nuclei: &[Phoneme],
    analyses: Analyses<'_>,
    harmony: &[Harmony],
    variant: Variant,
) {
    let foundation: Vec<bool> = analyses
        .roles
        .lines()
        .map(|(_, line)| line.layer() == Layer::Foundation)
        .collect();
    let feet: Vec<Vec<Span>> = analyses
        .meter
        .stanzas()
        .iter()
        .flat_map(crate::meter::StanzaMeter::lines)
        .map(|line| line.feet().iter().map(crate::meter::Foot::span).collect())
        .collect();
    let chord_of = |bar: usize| {
        harmony
            .iter()
            .find(|entry| entry.bar().get() == bar)
            .and_then(|entry| entry.chord())
    };

    // The events of one line are contiguous: the rhythm pass emits them bar by bar.
    let mut start = 0;
    while start < events.len() {
        let bar = events.get(start).map_or(0, |event| event.bar().get());
        let end = events
            .iter()
            .skip(start)
            .position(|event| event.bar().get() != bar)
            .map_or(events.len(), |offset| start + offset);
        let in_foundation = foundation.get(bar).copied().unwrap_or(false);
        if let (true, Some(line), Some(vowels)) = (
            in_foundation,
            events.get_mut(start..end),
            nuclei.get(start..end),
        ) {
            match variant {
                Variant::Syllables => syllables(line, vowels),
                Variant::Feet => by_foot(
                    line,
                    vowels,
                    feet.get(bar).map_or(&[][..], Vec::as_slice),
                    chord_of(bar),
                ),
                Variant::Off => {}
                // `Mapping::choose` stores only the bass's own variants, so anything else is the
                // default: the height bass.
                _ => height(line, vowels),
            }
        }
        start = end;
    }
}

/// The note a primary-stressed syllable's vowel gives it, if its vowel has a height.
fn height_note(event: &Event, vowel: Phoneme) -> Option<BassNote> {
    let height = Height::of(vowel)?;
    (event.velocity() == Velocity::PRIMARY).then(|| BassNote {
        span: event.span(),
        root: ScaleDegree::new(0),
        step: height.tone(),
        chromatic: Chromatic::Diatonic,
        velocity: event.velocity(),
        why: BassReason::Height(vowel),
    })
}

/// The height bass (*machine cycle 1*): every primary-stressed syllable of a foundation line is
/// pitched by its vowel's height, except in a rhyme word, where §4.5's note sounds instead — the
/// rule from before the bass had a field of its own, kept so the default piece does not change.
fn height(line: &mut [Event], vowels: &[Phoneme]) {
    for (event, &vowel) in line.iter_mut().zip(vowels) {
        if event.pitch().is_some() {
            continue;
        }
        if let Some(note) = height_note(event, vowel) {
            *event = event.with_bass(note);
        }
    }
}

/// W1 — the syllables walk (`PLAN.md`'s proposal). Every syllable of the line sounds a note: the
/// stressed ones their height note, the ones between by step from one towards the next, the last
/// before each stressed one a chromatic approach a semitone below it. The walk never looks past its
/// line, and a line with no stressed syllable whose vowel has a height has nothing to walk between,
/// so it has no bass.
fn syllables(line: &mut [Event], vowels: &[Phoneme]) {
    let anchors: Vec<(usize, BassNote)> = line
        .iter()
        .zip(vowels)
        .enumerate()
        .filter_map(|(at, (event, &vowel))| Some((at, height_note(event, vowel)?)))
        .collect();
    if anchors.is_empty() {
        return;
    }
    for (at, event) in line.iter_mut().enumerate() {
        let previous = anchors.iter().rev().find(|(index, _)| *index <= at);
        let next = anchors.iter().find(|(index, _)| *index > at);
        let (step, chromatic, why) = match (previous, next) {
            (Some((index, note)), _) if *index == at => (note.step, Chromatic::Diatonic, note.why),
            // The last syllable before a stressed one, wherever it falls: a semitone below the
            // note it leads into — the jazz bassist's chromatic approach.
            (_, Some((index, note))) if *index == at + 1 => (
                note.step,
                Chromatic::SemitoneBelow,
                BassReason::Approach { onto: note.step },
            ),
            (Some((from_at, from)), Some((_, toward))) => (
                passing(from.step, toward.step, at - from_at),
                Chromatic::Diatonic,
                BassReason::Passing {
                    from: from.step,
                    toward: toward.step,
                },
            ),
            // Before the line's first stressed syllable: climbing into it a step at a time, so
            // the step two before it is one below it (and the approach, one before, a semitone).
            (None, Some((index, toward))) => (
                toward.step.plus(-signed(index - at - 1)),
                Chromatic::Diatonic,
                BassReason::LeadIn {
                    toward: toward.step,
                },
            ),
            // After the line's last stressed syllable: stepping down from it.
            (Some((from_at, from)), None) => (
                from.step.plus(-signed(at - from_at)),
                Chromatic::Diatonic,
                BassReason::Tail { from: from.step },
            ),
            (None, None) => continue,
        };
        let note = BassNote {
            span: event.span(),
            root: ScaleDegree::new(0),
            step,
            chromatic,
            velocity: event.velocity(),
            why,
        };
        *event = event.with_bass(note);
    }
}

/// The `nth` passing tone from `from` towards `toward`: a step at a time, holding one step short
/// of `toward` so the approach and the arrival stay distinct; between two equal notes, the upper
/// neighbour.
fn passing(from: ScaleStep, toward: ScaleStep, nth: usize) -> ScaleStep {
    let gap = toward.get().saturating_sub(from.get());
    let (direction, limit) = match gap {
        0 => (1, 1),
        gap if gap > 0 => (1, gap - 1),
        gap => (-1, gap.saturating_neg() - 1),
    };
    from.plus(direction * signed(nth).min(limit))
}

/// A syllable count as a signed number of steps. A line is never two billion syllables long.
fn signed(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

/// W2 — the feet walk (`PLAN.md`'s proposal). One note per foot, on its first syllable — on the
/// beat — spanning the foot. The foot's stressed vowel (its most stressed syllable's, the first on
/// a tie) chooses the chord tone by its height, from the bar's chord when the harmony layer gave
/// the bar one and from the scale's root, third and fifth otherwise. Chord tones sit in one octave
/// in height order, so a higher next vowel already makes the step to it go up; where the tone would
/// repeat the walk's last note, the foot steps off it instead, up if the next foot's vowel is
/// higher and down otherwise, so the line keeps moving. A foot whose vowel has no height has no
/// note.
fn by_foot(line: &mut [Event], vowels: &[Phoneme], feet: &[Span], chord: Option<Chord>) {
    // (first syllable, most stressed syllable) of each foot, in order.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (at, event) in line.iter().enumerate() {
        match groups.last_mut() {
            Some((first, strongest))
                if line.get(*first).map(|e| e.beat()) == Some(event.beat()) =>
            {
                if line
                    .get(*strongest)
                    .is_some_and(|best| event.velocity() > best.velocity())
                {
                    *strongest = at;
                }
            }
            _ => groups.push((at, at)),
        }
    }
    let vowel_of = |strongest: usize| vowels.get(strongest).copied();
    let root = ScaleDegree::new(chord.map_or(0, |chord| chord.root().get()));

    let mut last: Option<ScaleStep> = None;
    for (nth, &(first, strongest)) in groups.iter().enumerate() {
        let Some(vowel) = vowel_of(strongest) else {
            continue;
        };
        let Some(height) = Height::of(vowel) else {
            continue;
        };
        let (Some(event), Some(velocity)) = (
            line.get(first).copied(),
            line.get(strongest).map(|e| e.velocity()),
        ) else {
            continue;
        };
        let Some(&span) = feet.get(event.beat().get()) else {
            continue;
        };
        let tone = height.tone();
        let (step, why) = if last == Some(tone) {
            let next = groups
                .get(nth + 1)
                .and_then(|&(_, strongest)| vowel_of(strongest));
            let direction = match next.and_then(Height::of) {
                Some(higher) if higher > height => Direction::Up,
                _ => Direction::Down,
            };
            (
                tone.moved(direction),
                BassReason::Step {
                    vowel,
                    chord,
                    direction,
                    next,
                },
            )
        } else {
            (tone, BassReason::ChordTone { vowel, chord })
        };
        last = Some(step);
        let note = BassNote {
            span,
            root,
            step,
            chromatic: Chromatic::Diatonic,
            velocity,
            why,
        };
        if let Some(slot) = line.get_mut(first) {
            *slot = slot.with_bass(note);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScaleStep, passing};

    #[test]
    fn a_passing_tone_steps_towards_its_target_and_stops_one_short() {
        // Hand-worked: from the root (0) to the fifth (4) the passing tones are 1, 2, 3, then hold
        // at 3 — one short of 4, so the approach (a semitone below 4) and the arrival stay apart.
        let walk: Vec<i32> = (1..=5)
            .map(|nth| passing(ScaleStep(0), ScaleStep(4), nth).get())
            .collect();
        assert_eq!(walk, vec![1, 2, 3, 3, 3]);
        // Downwards from the fifth to the third: 3, then hold.
        let down: Vec<i32> = (1..=3)
            .map(|nth| passing(ScaleStep(4), ScaleStep(2), nth).get())
            .collect();
        assert_eq!(down, vec![3, 3, 3]);
        // Between two equal notes: the upper neighbour.
        assert_eq!(passing(ScaleStep(2), ScaleStep(2), 1).get(), 3);
        assert_eq!(passing(ScaleStep(2), ScaleStep(2), 4).get(), 3);
    }
}
