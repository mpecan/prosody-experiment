//! The piece as positions in time, which is the only shape the metrics need.
//!
//! A [`Score`] places an event by bar, beat and subdivision, and bars differ in length (`SPEC.md`
//! §4.2: "Don't quantise to 4/4"). The metrics care about *when* things sound relative to each
//! other, so this flattens that into a global beat slot plus an exact fraction within the beat —
//! rationals, never floats, because positions are compared and merged (`CLAUDE.md` §4).
//!
//! It walks the arrangement the way both backends do — slot by slot, each line fitted into its room
//! in the slot — minus §4.3's downbeat anchoring, which the Strudel backend cannot express, so this is the
//! timeline both backends agree on.

use std::cmp::Ordering;

use prosody_core::voice::{Score, Velocity};

/// One sounding event, reduced to what the metrics read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Hit {
    /// Which beat of the whole piece, counted from zero.
    pub(crate) slot: u64,
    /// Which bar of the piece — a slot of the arrangement.
    pub(crate) bar: usize,
    /// Which subdivision of the beat, and of how many.
    pub(crate) within: (u64, u64),
    /// Which instrument.
    pub(crate) voice: usize,
    /// Struck at full force: a primary stress (`SPEC.md` §4.2's "stress → accent").
    pub(crate) accent: bool,
    /// Its note, if it is a rhyme word's syllable (§4.5), a bass note (§4.6), or a chord's root.
    pub(crate) degree: Option<Tone>,
    /// A chord of the harmony layer (§4.6) rather than a syllable: it sounds on its own
    /// instrument, not its line's voice.
    pub(crate) chord: bool,
}

/// A note as the score names it, before any scale: a number of scale steps, and whether it sits a
/// semitone below that step (the walking bass's approach tone, `prosody_core::bass`).
///
/// A rhyme degree *d* and a bass note *d* steps above degree 0 are the same tone here, as they
/// were when the bass was a [`Pitch`](prosody_core::Pitch). A walking bass note under a chord
/// (W2) is keyed by its step above the chord's root, not by root + step: the root is the
/// harmony's, numbered poem-wide and folded into one octave by the backend — a fold that needs
/// the scale, which this crate cannot see — and the step-5 review left chord roots out of pitch
/// entropy for exactly that reason. Counting it here would bring the miscount back through the
/// bass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Tone {
    pub(crate) step: i64,
    pub(crate) semitone_below: bool,
}

impl Tone {
    /// A scale degree: a rhyme note or a chord's root.
    pub(crate) fn degree(degree: usize) -> Self {
        Self {
            step: i64::try_from(degree).unwrap_or(i64::MAX),
            semitone_below: false,
        }
    }

    /// A bass note, by its step above its root.
    fn bass(note: prosody_core::BassNote) -> Self {
        Self {
            step: i64::from(note.step().get()),
            semitone_below: note.chromatic() == prosody_core::Chromatic::SemitoneBelow,
        }
    }
}

/// How deep in the metrical hierarchy a position sits. Shallower is stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Depth {
    /// The first beat of a bar.
    Downbeat = 0,
    /// The start of any other beat.
    Beat = 1,
    /// Anywhere inside a beat.
    Offbeat = 2,
}

/// A point on the piece's metrical grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Position {
    /// Time in beats, as `numerator / denominator`.
    pub(crate) at: (u64, u64),
    pub(crate) depth: Depth,
    /// Whether an accented event sounds here.
    pub(crate) accented: bool,
}

/// The whole piece, flattened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Timeline {
    pub(crate) hits: Vec<Hit>,
    /// Where each bar starts, in beats; one entry per bar.
    pub(crate) bar_starts: Vec<u64>,
    /// How many beats the piece lasts.
    pub(crate) beats: u64,
}

impl Timeline {
    /// Flattens a score, slot by slot through its arrangement.
    ///
    /// A line looping under a host of another length is fitted to the host's bar
    /// (`prosody_core::arrange`), so its syllables land between the host's beats. Positions are
    /// kept exact: a hit's beat is the whole part of its time, and `within` the remainder.
    pub(crate) fn of(score: &Score) -> Self {
        let mut hits = Vec::new();
        let mut bar_starts = Vec::new();
        let mut running = 0_u64;
        for (index, slot) in score.arrangement().slots().iter().enumerate() {
            bar_starts.push(running);
            let slot_beats = to_u64(slot.beats()).max(1);
            for placement in slot.placements() {
                // Where each event sounds in this slot — under the straight stack, fitted to the
                // host where they would cross (`Score::placed_in`), as both backends play it.
                let placed = score.placed_in(slot, *placement);
                // Under a walking bass, one walk a bar (`Score::sounds_bass`), as it is played.
                let bass_sounds = score.sounds_bass(slot, placement);
                let first = placed.first_beat();
                let line_beats = to_u64(placed.beats()).max(1);
                let room = to_u64(slot.room_for(placed.beats()));
                for (event, subdivision) in placed.onsets() {
                    let of = to_u64(subdivision.of()).max(1);
                    // Counted from the placement's first beat: a call need not open its line.
                    let position = to_u64(event.beat().get().saturating_sub(first)) * of
                        + to_u64(subdivision.index());
                    // `position / (of · line_beats)` of the line's room in the slot.
                    let scaled = position * room;
                    let denominator = of * line_beats;
                    hits.push(Hit {
                        slot: running + scaled / denominator,
                        bar: index,
                        within: reduce(scaled % denominator, denominator),
                        voice: event.voice().get(),
                        accent: event.velocity() == Velocity::PRIMARY,
                        // The melody's note where there is one. Under a walking bass a rhyme
                        // word carries a bass note too; this one hit counts the melody's.
                        degree: event.pitch().map_or_else(
                            || event.bass().filter(|_| bass_sounds).map(Tone::bass),
                            |pitch| Some(Tone::degree(pitch.degree().get())),
                        ),
                        chord: false,
                    });
                }
            }
            // The harmony layer (§4.6): one chord a slot, on the bar's first beat, held for the
            // bar. A timeline is onsets, so it is one hit — the held chord is not counted again in
            // the bar's later beats. A stanza's last bar under H2 is split: its own chord (if any)
            // from the start, the turnaround from the bar's half — `slot_beats / 2` beats in, as
            // the Strudel lane's `[a b]` places it.
            let harmony = score.harmony_in(slot);
            let chords = [
                harmony
                    .and_then(prosody_core::Harmony::chord)
                    .map(|chord| (chord, 0)),
                harmony
                    .and_then(prosody_core::Harmony::turnaround)
                    .map(|chord| (chord, slot_beats)),
            ];
            // `half` is in half-beats: the onset `half / 2` beats after the bar's start.
            for (chord, half) in chords.into_iter().flatten() {
                hits.push(Hit {
                    slot: running + half / 2,
                    bar: index,
                    within: reduce(half % 2, 2),
                    voice: 0,
                    accent: false,
                    degree: Some(Tone::degree(chord.root().get())),
                    chord: true,
                });
            }
            running += slot_beats;
        }
        Self {
            hits,
            bar_starts,
            beats: running,
        }
    }

    /// Every position on the grid, in time order: each beat's start, plus wherever anything
    /// sounds. A beat start with nothing on it is still a place an accent could have fallen, which
    /// is what syncopation is measured against.
    pub(crate) fn grid(&self) -> Vec<Position> {
        let mut positions: Vec<Position> = (0..self.beats)
            .map(|slot| Position {
                at: (slot, 1),
                depth: self.depth_of(slot, 0),
                accented: false,
            })
            .collect();
        positions.extend(self.hits.iter().map(|hit| Position {
            at: (hit.slot * hit.within.1 + hit.within.0, hit.within.1),
            depth: self.depth_of(hit.slot, hit.within.0),
            accented: hit.accent,
        }));
        positions.sort_by(|a, b| compare(a.at, b.at).then(a.depth.cmp(&b.depth)));

        // One position per instant: `0/2` and `0/1` are the same moment.
        let mut merged: Vec<Position> = Vec::with_capacity(positions.len());
        for position in positions {
            match merged.last_mut() {
                Some(last) if compare(last.at, position.at) == Ordering::Equal => {
                    last.accented |= position.accented;
                    last.depth = last.depth.min(position.depth);
                }
                _ => merged.push(position),
            }
        }
        merged
    }

    fn depth_of(&self, slot: u64, index: u64) -> Depth {
        if index != 0 {
            Depth::Offbeat
        } else if self.bar_starts.contains(&slot) {
            Depth::Downbeat
        } else {
            Depth::Beat
        }
    }
}

/// Compares two exact fractions without dividing.
fn compare((an, ad): (u64, u64), (bn, bd): (u64, u64)) -> Ordering {
    (u128::from(an) * u128::from(bd)).cmp(&(u128::from(bn) * u128::from(ad)))
}

/// A fraction in lowest terms; `0/n` becomes `0/1`.
fn reduce(numerator: u64, denominator: u64) -> (u64, u64) {
    let (mut a, mut b) = (numerator, denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    let divisor = a.max(1);
    (numerator / divisor, (denominator / divisor).max(1))
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Depth, Hit, Timeline};

    /// A hit, for hand-built timelines.
    pub(crate) const fn hit(slot: u64, bar: usize, within: (u64, u64), voice: usize) -> Hit {
        Hit {
            slot,
            bar,
            within,
            voice,
            accent: false,
            degree: None,
            chord: false,
        }
    }

    #[test]
    fn the_grid_has_every_beat_and_merges_equal_instants() {
        // One bar of two beats; a hit on beat 0 (written 0/2) and one halfway through beat 1.
        let timeline = Timeline {
            hits: vec![hit(0, 0, (0, 2), 0), hit(1, 0, (1, 2), 0)],
            bar_starts: vec![0],
            beats: 2,
        };
        let grid: Vec<_> = timeline.grid().iter().map(|p| (p.at, p.depth)).collect();
        assert_eq!(
            grid,
            vec![
                ((0, 1), Depth::Downbeat),
                ((1, 1), Depth::Beat),
                ((3, 2), Depth::Offbeat),
            ]
        );
    }
}
