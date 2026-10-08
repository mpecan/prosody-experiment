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
//! # Bar-locked
//!
//! Every [`Slot`] is one bar of the piece, and its length is its *host*'s: the line arriving in
//! it. Whatever accompanies the host is fitted into that bar, however many beats its own line
//! has — the way a loop sits under a phrase in a track, rather than drifting against it. A
//! backend renders a placement's events at their position *within their own line*, scaled to the
//! slot.
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

use crate::parse::Poem;
use crate::role::{Layer, LineBasis, PoemRoles};
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
}

/// One line sounding in one slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Placement {
    bar: Bar,
    layer: Layer,
    cause: Cause,
}

impl Placement {
    /// The line, by its bar in the [`Score`] — one bar per line, in reading order.
    #[must_use]
    pub const fn bar(self) -> Bar {
        self.bar
    }

    /// Which layer's loop it belongs to here.
    #[must_use]
    pub const fn layer(self) -> Layer {
        self.layer
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

    /// How many beats the slot lasts: its host line's foot count, or — when the host has no
    /// pronounceable syllables — the longest line sounding with it, and never less than one.
    #[must_use]
    pub const fn beats(&self) -> usize {
        self.beats
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
/// `roles` must describe `poem`; `score` must be its lowering. Lowering calls this itself, having
/// already checked both.
#[must_use]
pub fn run(poem: &Poem, roles: &PoemRoles, score: &Score) -> Arrangement {
    let mut slots = Vec::new();
    let mut loops = Loops::default();
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
            loops = Loops::default();
            for &bar in &bars {
                slots.push(slot(score, vec![placement(bar, layer, Cause::Overruling)]));
            }
        } else if let Some(old) = loops.of(layer).take() {
            for (pass, line_up) in transformation(&old.bars, &bars) {
                for (bar, arriving) in line_up {
                    let cause = if arriving {
                        Cause::Arriving { pass }
                    } else {
                        Cause::Yielding { pass }
                    };
                    slots.push(accompanied(score, &mut loops, placement(bar, layer, cause)));
                }
            }
        } else {
            for &bar in &bars {
                let host = placement(bar, layer, Cause::Stated);
                slots.push(accompanied(score, &mut loops, host));
            }
        }
        *loops.of(layer) = Some(Loop::new(bars));
    }
    Arrangement { slots }
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

/// A slot for `host`, with the other layer's loop under it if one is running.
fn accompanied(score: &Score, loops: &mut Loops, host: Placement) -> Slot {
    let mut placements = vec![host];
    if let Some(running) = loops.of(other(host.layer))
        && let Some(bar) = running.next()
    {
        placements.push(placement(bar, other(host.layer), Cause::Looping));
    }
    slot(score, placements)
}

const fn placement(bar: usize, layer: Layer, cause: Cause) -> Placement {
    Placement {
        bar: Bar::new(bar),
        layer,
        cause,
    }
}

fn slot(score: &Score, placements: Vec<Placement>) -> Slot {
    let host = placements
        .first()
        .map_or(0, |host| score.beats_in(host.bar.get()));
    let longest = placements
        .iter()
        .map(|placement| score.beats_in(placement.bar.get()))
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
