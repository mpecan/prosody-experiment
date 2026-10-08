//! Arrangement: which lines sound in which bar of the piece, and why.
//!
//! Lowering places every syllable of every line once, line by line — the poem read aloud. This
//! pass decides how that material is laid out in time, and the rule is the one `SPEC.md` §4.1 was
//! reaching for when it called the foundation "drums, bass, ostinato": **a stanza, once stated,
//! keeps looping until something takes its place.** Two things can:
//!
//! - **A competing stanza.** There is one loop per [`Layer`]. A stanza arriving in a layer that
//!   already loops *transforms* the old loop into itself, one bar per pass: the first pass swaps
//!   the old loop's first bar for the newcomer's first line, the second pass its first two, and so
//!   on until the loop is entirely the new stanza. Meanwhile the other layer's loop carries on
//!   underneath. A stanza arriving in an empty layer is simply stated, once, over whatever the
//!   other layer is playing.
//! - **A refrain stanza.** One made entirely of refrain lines (§4.3's refrain row) overrules
//!   everything: every loop stops, and it is stated alone — a chorus dropping in. It then loops
//!   like any other stanza.
//!
//! The piece ends when the last stanza has fully arrived.
//!
//! # The call lane: §4.1's second tier
//!
//! §4.1 suggests "a second tier where a line-internal caesura splits a voice". Here it is a third
//! lane beside the two layers. The half-line before a line's first internal caesura — its *call*,
//! "Tyger Tyger," of "Tyger Tyger, burning bright" — is lifted out the first time its line is
//! heard, and from the next bar on it loops under everything, fitted to each bar like any loop,
//! until the next line with a caesura is first heard and its call takes over. A refrain stanza
//! stops it with every other loop. A line is "first heard" once: a line re-hosting a pass of a
//! transformation does not restart the lane.
//!
//! This is what gives a poem whose stanzas all share one layer something underneath it: caesurae
//! are sparse, but each one leaves a figure that persists.
//!
//! # The held lane: a rhyme left open keeps sounding
//!
//! §4.5 makes rhyme melody; this makes it suspension. A line whose rhyme is answered later *in its
//! own stanza* is held — it keeps sounding, fitted to each bar, from the bar after it is first heard
//! until the bar its answer is first heard, where it stops: the answer resolves it. In ABAB the A
//! line holds under the B line, then the B line under the second A; in ABBA the A line holds under
//! both Bs. Several lines can be open at once, and each is its own placement.
//!
//! An answer has to come within the stanza. A rhyme class is poem-wide (§4.5, so a refrain can
//! return to its pitch), but a line answered thirty lines later is not a suspension anyone hears
//! as one, and holding it that long would bury everything else. A line in a class of one is never
//! answered, so never held.
//!
//! # The refrain lane: §4.3's "literal loop of that voice"
//!
//! A refrain line — one whose text occurs more than once — loops in a lane of its own from the bar
//! after it is first heard, until the next refrain line is first heard and takes the lane over.
//! It is the call lane's rule with a whole repeated line in place of a half-line.
//!
//! No lane doubles what already sounds: a held or refrain line whose voice is already in the slot
//! — as its host, or in a layer's loop — is left out of it.
//!
//! A refrain stanza stops the held and refrain lanes along with every other loop, and is stated
//! alone.
//!
//! # Bar-locked, to the stanza's period
//!
//! Every [`Slot`] is one bar of the piece, and its length is its *host's stanza's period*: the
//! longest line of the stanza the host belongs to. A stanza is a loop, and a loop has one length —
//! so every bar of it lasts the same, and the meter only changes where the stanza does. Odd meters
//! survive (`SPEC.md` §4.2): a stanza of trimeter is a loop of three. (*Machine cycle 2, pending
//! review*: until then a bar took its host line's own length, which changed the meter nearly every
//! bar.)
//!
//! Inside the bar, a placement keeps its own pace when it fits — a line shorter than the bar plays
//! at one beat per foot and rests for the remainder — and is squeezed to the bar when it is longer
//! ([`Slot::room_for`]). A loop from a longer stanza sits under a shorter one that way, rather than
//! drifting against it. A backend renders a placement's events at their position *within their
//! own line*, scaled into that room.
//!
//! # What this does to the invariants
//!
//! - **I4 holds.** A slot holds nothing but [`Placement`]s of whole lines the lowering already
//!   produced, and every event of those carries its span. A looped syllable is attributable to the
//!   same word its first statement was.
//! - **I2 holds for material, not for duration.** Each line's events are exactly what lowering
//!   made of that line alone. But *how long a stanza keeps sounding* depends on when the next
//!   stanza of its layer arrives, so an edit that moves a later stanza between layers re-times an
//!   earlier one. That is what "loops until something takes its place" means, and it was decided
//!   as such rather than discovered; `prosody-score`'s edit test measures how often it happens.
//! - **I3 holds.** Nothing here reads a style. Looping is the poem's structure, which settles
//!   `SPEC.md` §9's open question about looping-as-style: it is not style.

use std::collections::{BTreeMap, BTreeSet};

use crate::parse::Poem;
use crate::repetition::VoiceId;
use crate::rhyme::PoemRhyme;
use crate::role::{Layer, LineBasis, PoemRoles};
use crate::span::Span;
use crate::voice::{Bar, Score};

/// Why a line sounds in a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cause {
    /// Its stanza's first statement, in a layer that had no loop.
    Stated,
    /// Its stanza's statement as a refrain stanza, which stopped every loop.
    Overruling,
    /// Its stanza arriving into a layer that was already looping: this line has replaced the old
    /// loop's bar at its position. `pass` counts from 1.
    Arriving {
        /// Which pass of the transformation, from 1.
        pass: usize,
    },
    /// Part of the old loop, not yet replaced on this pass of a transformation.
    Yielding {
        /// Which pass of the transformation, from 1.
        pass: usize,
    },
    /// A loop carrying on in its layer under another layer's arrival.
    Looping,
    /// The half-line before this line's first caesura, looping in the call lane.
    Calling,
    /// A line whose rhyme its stanza has not answered yet, held in the held lane.
    Holding,
    /// A refrain line, looping in the refrain lane.
    Refraining,
}

/// Which of the piece's loops a placement belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// A layer's loop of whole stanzas (§4.1).
    Layer(Layer),
    /// The call lane: one half-line, looping.
    Call,
    /// The held lane: lines waiting for their rhyme.
    Held,
    /// The refrain lane: one refrain line, looping.
    Refrain,
}

/// How much of its line a placement sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    /// All of it.
    Line,
    /// Only the half-line before its first caesura — this span of it.
    Call(Span),
}

impl Part {
    /// Whether an event with this span belongs to the part.
    #[must_use]
    pub const fn covers(self, span: Span) -> bool {
        match self {
            Self::Line => true,
            Self::Call(call) => call.contains(span),
        }
    }
}

/// One line — or the call of one — sounding in one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Placement {
    bar: Bar,
    lane: Lane,
    cause: Cause,
    part: Part,
}

impl Placement {
    /// The line, by its bar in the [`Score`] — one bar per line, in reading order.
    #[must_use]
    pub const fn bar(self) -> Bar {
        self.bar
    }

    /// Which loop it belongs to here.
    #[must_use]
    pub const fn lane(self) -> Lane {
        self.lane
    }

    /// How much of the line sounds.
    #[must_use]
    pub const fn part(self) -> Part {
        self.part
    }

    /// Why it sounds.
    #[must_use]
    pub const fn cause(self) -> Cause {
        self.cause
    }
}

/// One bar of the piece.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    beats: usize,
    placements: Vec<Placement>,
}

impl Slot {
    /// Everything sounding in it. The first is the host, whose length the slot takes.
    #[must_use]
    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }

    /// The line the slot is built around.
    #[must_use]
    pub fn host(&self) -> Option<Placement> {
        self.placements.first().copied()
    }

    /// How many beats the slot lasts: its host's stanza's period — the longest line of that
    /// stanza — or, when the stanza has no pronounceable syllables, the longest line sounding with
    /// it; never less than one.
    #[must_use]
    pub const fn beats(&self) -> usize {
        self.beats
    }

    /// How many of the slot's beats a placement of `beats` beats fills: all of its own when it
    /// fits, at one beat per foot, and the whole slot when it is longer and must be squeezed in.
    ///
    /// Every consumer fits a placement into this, so they agree on where each syllable falls.
    #[must_use]
    pub fn room_for(&self, beats: usize) -> usize {
        beats.clamp(1, self.beats.max(1))
    }
}

/// The whole piece, slot by slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Arrangement {
    slots: Vec<Slot>,
}

impl Arrangement {
    /// Every bar of the piece, in playing order.
    #[must_use]
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }
}

/// A layer's loop: its lines, and how far through them it has played.
#[derive(Debug, Clone)]
struct Loop {
    bars: Vec<usize>,
    position: usize,
}

impl Loop {
    const fn new(bars: Vec<usize>) -> Self {
        Self { bars, position: 0 }
    }

    /// The loop's next bar, advancing it — one per slot it accompanies.
    fn next(&mut self) -> Option<usize> {
        let len = self.bars.len();
        if len == 0 {
            return None;
        }
        let bar = self.bars.get(self.position % len).copied();
        self.position += 1;
        bar
    }
}

/// The loops currently running, one per layer.
#[derive(Debug, Clone, Default)]
struct Loops {
    foundation: Option<Loop>,
    flourish: Option<Loop>,
}

impl Loops {
    const fn of(&mut self, layer: Layer) -> &mut Option<Loop> {
        match layer {
            Layer::Foundation => &mut self.foundation,
            Layer::Flourish => &mut self.flourish,
        }
    }
}

const fn other(layer: Layer) -> Layer {
    match layer {
        Layer::Foundation => Layer::Flourish,
        Layer::Flourish => Layer::Foundation,
    }
}

/// Arranges a lowered poem (see the module documentation for the rule).
///
/// `roles` and `rhyme` must describe `poem`; `score` must be its lowering. Lowering calls this
/// itself, having already checked all three.
#[must_use]
pub fn run(poem: &Poem, roles: &PoemRoles, rhyme: &PoemRhyme, score: &Score) -> Arrangement {
    let mut piece = Piece {
        score,
        periods: periods(poem, score),
        calls: calls(poem, score),
        answers: answers(rhyme),
        refrains: refrains(roles),
        slots: Vec::new(),
        loops: Loops::default(),
        lane: None,
        held: BTreeSet::new(),
        refrain: None,
        heard: BTreeSet::new(),
    };
    let mut first_bar = 0;
    for (index, stanza) in poem.stanzas().iter().enumerate() {
        let bars: Vec<usize> = (first_bar..first_bar + stanza.lines().len()).collect();
        first_bar += stanza.lines().len();
        let Some(role) = roles.stanzas().get(index) else {
            continue;
        };
        if bars.is_empty() {
            continue;
        }
        let layer = role.layer();
        let refrain = role
            .lines()
            .iter()
            .all(|line| matches!(line.basis(), LineBasis::Refrain(_)));

        if refrain {
            piece.loops = Loops::default();
            piece.lane = None;
            piece.held.clear();
            piece.refrain = None;
            for &bar in &bars {
                piece.push(placement(bar, layer, Cause::Overruling), Company::Alone);
            }
        } else if let Some(old) = piece.loops.of(layer).take() {
            for (pass, line_up) in transformation(&old.bars, &bars) {
                for (bar, arriving) in line_up {
                    let cause = if arriving {
                        Cause::Arriving { pass }
                    } else {
                        Cause::Yielding { pass }
                    };
                    piece.push(placement(bar, layer, cause), Company::Loops);
                }
            }
        } else {
            for &bar in &bars {
                piece.push(placement(bar, layer, Cause::Stated), Company::Loops);
            }
        }
        *piece.loops.of(layer) = Some(Loop::new(bars));
    }
    Arrangement { slots: piece.slots }
}

/// Every line's call: the half-line before its first internal caesura, if it has one that makes
/// a sound. A trailing comma divides nothing, so a line needs two segments to have a call.
fn calls(poem: &Poem, score: &Score) -> BTreeMap<usize, Span> {
    poem.stanzas()
        .iter()
        .flat_map(crate::parse::Stanza::lines)
        .enumerate()
        .filter_map(|(bar, line)| {
            let segments = line.segments();
            let call = *segments.first()?;
            let sounds = score
                .events()
                .iter()
                .any(|event| event.bar().get() == bar && call.contains(event.span()));
            (segments.len() > 1 && sounds).then_some((bar, call))
        })
        .collect()
}

/// Every line's stanza period: the most beats any line of its stanza takes.
fn periods(poem: &Poem, score: &Score) -> BTreeMap<usize, usize> {
    let mut periods = BTreeMap::new();
    let mut first_bar = 0;
    for stanza in poem.stanzas() {
        let bars = first_bar..first_bar + stanza.lines().len();
        first_bar = bars.end;
        let period = bars
            .clone()
            .map(|bar| score.beats_in(bar))
            .max()
            .unwrap_or(0);
        periods.extend(bars.map(|bar| (bar, period)));
    }
    periods
}

/// Each line whose rhyme is answered later in its stanza, and the line that answers it: the next
/// one in the stanza of the same class.
fn answers(rhyme: &PoemRhyme) -> BTreeMap<usize, usize> {
    let mut answers = BTreeMap::new();
    let mut first_bar = 0;
    for stanza in rhyme.stanzas() {
        let lines = stanza.lines();
        for (index, line) in lines.iter().enumerate() {
            let Some(class) = line.class() else {
                continue;
            };
            let answer = lines
                .iter()
                .enumerate()
                .skip(index + 1)
                .find(|(_, later)| later.class() == Some(class));
            if let Some((later, _)) = answer {
                answers.insert(first_bar + index, first_bar + later);
            }
        }
        first_bar += lines.len();
    }
    answers
}

/// Every refrain line, by bar, with the voice it repeats.
fn refrains(roles: &PoemRoles) -> BTreeMap<usize, VoiceId> {
    roles
        .lines()
        .enumerate()
        .filter_map(|(bar, (_, line))| match line.basis() {
            LineBasis::Refrain(voice) => Some((bar, voice)),
            LineBasis::Inherited(_) => None,
        })
        .collect()
}

/// What may sound with a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Company {
    /// The other layer's loop and the call, held and refrain lanes.
    Loops,
    /// Nothing: a refrain stanza is stated alone.
    Alone,
}

/// The arrangement being built.
struct Piece<'a> {
    score: &'a Score,
    periods: BTreeMap<usize, usize>,
    calls: BTreeMap<usize, Span>,
    answers: BTreeMap<usize, usize>,
    refrains: BTreeMap<usize, VoiceId>,
    slots: Vec<Slot>,
    loops: Loops,
    /// The call lane: whose call is looping, if any.
    lane: Option<(usize, Span)>,
    /// The held lane: lines waiting for their answer.
    held: BTreeSet<usize>,
    /// The refrain lane: which refrain line is looping, if any.
    refrain: Option<usize>,
    /// Lines already heard as a host, so re-hosting one does not restart a lane.
    heard: BTreeSet<usize>,
}

impl Piece<'_> {
    /// Whether `bar`'s line — or another line of its voice — already sounds among `placements`.
    /// A lane never doubles what is already playing.
    fn sounding(&self, placements: &[Placement], bar: usize) -> bool {
        let voice = self.refrains.get(&bar);
        placements.iter().any(|placement| {
            let other = placement.bar.get();
            other == bar || (voice.is_some() && self.refrains.get(&other) == voice)
        })
    }

    /// Adds a slot for `host`, with whatever loops under it.
    fn push(&mut self, host: Placement, company: Company) {
        let bar = host.bar.get();
        let first = !self.heard.contains(&bar);
        if first {
            // An answer resolves its line in the bar it arrives in: the two do not sound together.
            let answers = &self.answers;
            self.held.retain(|opener| answers.get(opener) != Some(&bar));
        }

        let mut placements = vec![host];
        if company == Company::Loops {
            if let Lane::Layer(layer) = host.lane
                && let Some(running) = self.loops.of(other(layer))
                && let Some(bar) = running.next()
            {
                placements.push(placement(bar, other(layer), Cause::Looping));
            }
            if let Some((bar, call)) = self.lane {
                placements.push(Placement {
                    bar: Bar::new(bar),
                    lane: Lane::Call,
                    cause: Cause::Calling,
                    part: Part::Call(call),
                });
            }
            if let Some(refrain) = self.refrain
                && !self.sounding(&placements, refrain)
            {
                placements.push(lane(refrain, Lane::Refrain, Cause::Refraining));
            }
            for &opener in &self.held {
                if !self.sounding(&placements, opener) {
                    placements.push(lane(opener, Lane::Held, Cause::Holding));
                }
            }
        }
        let period = self.periods.get(&bar).copied().unwrap_or(0);
        self.slots.push(slot(self.score, period, placements));

        // Lanes change *after* the bar their line is first heard in, so a line is heard in place
        // before it is heard underneath.
        if first {
            self.heard.insert(bar);
            if let Some(&call) = self.calls.get(&bar) {
                self.lane = Some((bar, call));
            }
            if self.refrains.contains_key(&bar) {
                self.refrain = Some(bar);
            }
            if self.answers.contains_key(&bar) {
                self.held.insert(bar);
            }
        }
    }
}

/// The passes that turn loop `old` into stanza `new`, each a list of `(bar, is_new)`.
///
/// Pass `p` (from 1) is the new stanza's first `p` lines followed by whatever of the old loop lies
/// beyond position `p`; the last pass is the new stanza exactly, so none of the old loop outlives
/// the transformation even when it was the longer of the two.
fn transformation(old: &[usize], new: &[usize]) -> Vec<(usize, Vec<(usize, bool)>)> {
    (1..=new.len())
        .map(|pass| {
            let incoming = new.iter().take(pass).map(|&bar| (bar, true));
            let line_up = if pass == new.len() {
                incoming.collect()
            } else {
                incoming
                    .chain(old.iter().skip(pass).map(|&bar| (bar, false)))
                    .collect()
            };
            (pass, line_up)
        })
        .collect()
}

const fn placement(bar: usize, layer: Layer, cause: Cause) -> Placement {
    Placement {
        bar: Bar::new(bar),
        lane: Lane::Layer(layer),
        cause,
        part: Part::Line,
    }
}

const fn lane(bar: usize, lane: Lane, cause: Cause) -> Placement {
    Placement {
        bar: Bar::new(bar),
        lane,
        cause,
        part: Part::Line,
    }
}

fn slot(score: &Score, period: usize, placements: Vec<Placement>) -> Slot {
    let host = period;
    let longest = placements
        .iter()
        .map(|placement| score.placed(*placement).beats())
        .max()
        .unwrap_or(0);
    let beats = if host > 0 { host } else { longest.max(1) };
    Slot { beats, placements }
}

#[cfg(test)]
mod tests {
    use super::transformation;

    #[test]
    fn a_loop_becomes_the_new_stanza_one_bar_per_pass() {
        // The handover the user chose, by hand: old A = 0..4, new B = 4..8.
        // |B1 A2 A3 A4|B1 B2 A3 A4|B1 B2 B3 A4|B1 B2 B3 B4|
        let passes = transformation(&[0, 1, 2, 3], &[4, 5, 6, 7]);
        let bars: Vec<Vec<usize>> = passes
            .iter()
            .map(|(_, line_up)| line_up.iter().map(|(bar, _)| *bar).collect())
            .collect();
        assert_eq!(
            bars,
            vec![
                vec![4, 1, 2, 3],
                vec![4, 5, 2, 3],
                vec![4, 5, 6, 3],
                vec![4, 5, 6, 7],
            ]
        );
        let first: Vec<bool> = passes[0].1.iter().map(|(_, new)| *new).collect();
        assert_eq!(first, vec![true, false, false, false]);
        let numbers: Vec<usize> = passes.iter().map(|(pass, _)| *pass).collect();
        assert_eq!(numbers, vec![1, 2, 3, 4]);
    }

    #[test]
    fn a_shorter_stanza_does_not_leave_the_old_loop_behind() {
        // Old loop of four, new stanza of two: pass 1 keeps the old tail, pass 2 is the new
        // stanza alone.
        let passes = transformation(&[0, 1, 2, 3], &[4, 5]);
        let bars: Vec<Vec<usize>> = passes
            .iter()
            .map(|(_, line_up)| line_up.iter().map(|(bar, _)| *bar).collect())
            .collect();
        assert_eq!(bars, vec![vec![4, 1, 2, 3], vec![4, 5]]);
    }

    #[test]
    fn a_longer_stanza_grows_the_loop() {
        // Old loop of two, new stanza of three: |B1 A2|B1 B2|B1 B2 B3|.
        let passes = transformation(&[0, 1], &[2, 3, 4]);
        let bars: Vec<Vec<usize>> = passes
            .iter()
            .map(|(_, line_up)| line_up.iter().map(|(bar, _)| *bar).collect())
            .collect();
        assert_eq!(bars, vec![vec![2, 1], vec![2, 3], vec![2, 3, 4]]);
    }
}
