//! The straight stack: a stacked line fitted to its host's subdivision of each beat.
//!
//! §4.2 divides a beat by its foot's syllables — "syllable → subdivision within the foot" — and
//! §4.6's lanes stack lines in one bar: a line held until its rhyme is answered, a refrain looping
//! in its lane, a call, a loop yielding to a newcomer or carrying on under the other layer. Where a
//! stacked line's foot has three syllables and the host's two, the beat carries three against two
//! (Wilde's Theocritus: 38 of its 126 beats; `prosody-score`'s `cross_rhythm`). That is the piece as
//! specified, and it stays the default ([`Variant::Crossing`](crate::mapping::Variant::Crossing)).
//!
//! [`Variant::Straight`](crate::mapping::Variant::Straight) (*decided by the user as a variant to compare, `PLAN.md`*) fits the
//! stacked line to the host instead, in exactly the beats where the two would cross. The host —
//! the slot's first placement, the line the bar is built around — keeps its own feet; so does every
//! line in a beat where it already agrees with the host.
//!
//! # The rule
//!
//! A beat's *grid*, for one line, is the least common multiple of its onsets' denominators in the
//! beat, reduced: `[x x]` is 2, `[x x x]` 3, a lone onset on the beat 1 — the measure
//! `prosody-score`'s `cross_rhythm` uses, so the variant removes exactly what the metric counts
//! against the host. Two grids cross when neither divides the other. Where a stacked line of *n*
//! syllables crosses a host grid of *h*:
//!
//! - **More syllables than the host has parts** (*n* > *h*: a triplet over a pair): the first
//!   *h* − 1 syllables take one of the host's parts each, and the rest share the last part
//!   equally. Three into two is `[a@2 b c]` — onsets at 0, ½ and ¾, a long-short-short. The first
//!   syllable keeps the host's downbeat, the second lands on the host's second onset, and the
//!   third is a passing subdivision of it.
//! - **Fewer** (*n* < *h*: a pair over a triplet): the first syllable holds the parts the line has
//!   no syllable for, and the rest take one each. Two into three is `[a@2 b]` — onsets at 0 and ⅔,
//!   the long-short of a swung pair.
//! - **As many**: one part each, on the host's onsets.
//!
//! Either way "the first takes the long": the syllable that opens the beat is held, whatever its
//! stress (stress stays the gain, §4.2), and the crowding happens at the end of the beat, where the
//! next beat's onset already marks time. Both results lie on the host's
//! grid or on a subdivision of it, so the fitted line never crosses its host (`fit` is tested to).
//!
//! **Every syllable stays its own event** (I4): nothing is merged, dropped or added; a fitted
//! syllable sounds in the same beat of the same slot, traced to the same word, at another point in
//! the beat and — the long one — for longer ([`crate::voice::Subdivision::parts`]).
//!
//! # What is left alone
//!
//! - **The host**, and every line in a beat where the host is silent (a host shorter than its
//!   bar rests in its last beats): there is no host grid to fit to.
//! - **A placement off the slot's grid** ([`Slot::on_grid`]): a loop squeezed into a shorter bar
//!   or stretched across a longer one has beats that fall between the host's, which no fitting
//!   within one beat can straighten.
//! - **Two stacked lines with each other.** Each is fitted to the host only; where two lines cross
//!   each other but neither the host (four syllables and three over a host's pair fit to grids
//!   of 6 and 4), they still cross.
//!
//! # Where it lives, and why here
//!
//! In the core, applied slot by slot by [`Score::placed_in`], the one function every consumer
//! reads a placement through. The Strudel pattern and the page's trace (one walk), the OSC
//! schedule and `prosody-score`'s timeline all ask it for each event's position in its beat, so
//! they agree by construction; an emitter-side fit would leave the scorer measuring the crossing
//! the listener no longer hears. It does not change [`Score::events`]: a line's material is its own
//! (I2), and only where it sounds *under another line* does that line's beat shape its timing.
//!
//! **Disclosed against I2** (`SPEC.md` §4.6): under `straight`, a stacked line's onsets in a slot
//! depend on the host's feet in that slot, and the host can belong to another stanza — a call or
//! refrain from stanza *m* looping under stanza *n*'s line. So an edit to stanza *n* that changes a
//! host's feet re-times, within the beat, stanza *m*'s line where it sounds under that host. Its
//! events, beats, voice and seed are untouched; every I2 property is about those and still holds.
//! Under the default, `crossing`, nothing crosses a line's boundary.

use std::collections::BTreeMap;

use crate::arrange::{Placement, Slot};
use crate::voice::{Event, Placed, Score, Straightened, Subdivision};

/// Fits `placed` — what `placement` sounds — to the host of `slot`, beat by beat, where the two
/// would cross. Called by [`Score::placed_in`] under the straight stack, and by nothing else.
pub(crate) fn straighten(
    score: &Score,
    slot: &Slot,
    placement: Placement,
    placed: &mut Placed<'_>,
) {
    let Some(host) = slot.host() else {
        return;
    };
    if host == placement || !slot.on_grid(placed.beats()) {
        return;
    }
    let hosted = score.placed(host);
    if !slot.on_grid(hosted.beats()) {
        return;
    }
    let host_grids: BTreeMap<usize, usize> = by_beat(hosted.first_beat(), hosted.onsets())
        .into_iter()
        .map(|(beat, onsets)| (beat, grid(onsets.iter().map(|(_, sub)| *sub))))
        .collect();
    for (beat, onsets) in by_beat(placed.first_beat(), placed.onsets()) {
        let Some(&host_grid) = host_grids.get(&beat) else {
            continue;
        };
        let own = grid(onsets.iter().map(|(_, sub)| *sub));
        if crosses(own, host_grid) {
            let fitted: Vec<(usize, Subdivision)> = onsets
                .iter()
                .map(|(position, _)| *position)
                .zip(fit(onsets.len(), host_grid))
                .collect();
            placed.fit(&fitted, Straightened::new(beat, own, host_grid));
        }
    }
}

/// A placement's onsets by the beat they fall in, counted from its first beat, each with its
/// position among the placement's events — in playing order within the beat.
fn by_beat<'a>(
    first_beat: usize,
    onsets: impl Iterator<Item = (&'a Event, Subdivision)>,
) -> BTreeMap<usize, Vec<(usize, Subdivision)>> {
    let mut beats: BTreeMap<usize, Vec<(usize, Subdivision)>> = BTreeMap::new();
    for (position, (event, subdivision)) in onsets.enumerate() {
        beats
            .entry(event.beat().get().saturating_sub(first_beat))
            .or_default()
            .push((position, subdivision));
    }
    for onsets in beats.values_mut() {
        // Total and stable: by where each sounds, then by position (already the playing order).
        onsets.sort_by(|(a_at, a), (b_at, b)| {
            (a.index() * b.of().max(1))
                .cmp(&(b.index() * a.of().max(1)))
                .then(a_at.cmp(b_at))
        });
    }
    beats
}

/// A line's grid in one beat: the least common multiple of its onsets' reduced denominators.
fn grid(onsets: impl Iterator<Item = Subdivision>) -> usize {
    onsets.fold(1, |grid, onset| {
        let of = onset.of().max(1);
        lcm(grid, of / gcd(onset.index(), of).max(1))
    })
}

/// Whether two grids cross: neither divides the other. 2 and 4 are one grid subdivided; 2 and 3
/// are two.
const fn crosses(a: usize, b: usize) -> bool {
    a != 0 && b != 0 && !a.is_multiple_of(b) && !b.is_multiple_of(a)
}

/// Where `n` syllables sound in a beat the host divides into `parts`: the rule in the module
/// documentation, each as a [`Subdivision`] of one shared divisor.
fn fit(n: usize, parts: usize) -> Vec<Subdivision> {
    if n == 0 || parts == 0 {
        return Vec::new();
    }
    if n >= parts {
        // The first `parts − 1` take a host part each — `share` sub-parts long — and the last
        // `share` syllables split the last host part between them.
        let share = n - parts + 1;
        let of = parts * share;
        (0..n)
            .map(|syllable| {
                if syllable + 1 < parts {
                    Subdivision::lasting(syllable * share, of, share)
                } else {
                    Subdivision::lasting((parts - 1) * share + (syllable + 1 - parts), of, 1)
                }
            })
            .collect()
    } else {
        // The first holds every part the line has no syllable for; the rest take one each.
        let long = parts - n + 1;
        (0..n)
            .map(|syllable| {
                if syllable == 0 {
                    Subdivision::lasting(0, parts, long)
                } else {
                    Subdivision::lasting(long + syllable - 1, parts, 1)
                }
            })
            .collect()
    }
}

const fn gcd(a: usize, b: usize) -> usize {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let rest = a % b;
        a = b;
        b = rest;
    }
    a
}

const fn lcm(a: usize, b: usize) -> usize {
    if a == 0 || b == 0 {
        return 0;
    }
    a / gcd(a, b) * b
}

#[cfg(test)]
mod tests {
    use super::{crosses, fit, grid};
    use crate::voice::Subdivision;

    /// `(index, of, parts)` of each fitted syllable.
    fn shape(n: usize, parts: usize) -> Vec<(usize, usize, usize)> {
        fit(n, parts)
            .into_iter()
            .map(|sub| (sub.index(), sub.of(), sub.parts()))
            .collect()
    }

    #[test]
    fn a_triplet_over_a_pair_is_a_long_short_short() {
        // By hand: two host parts; the first syllable takes the first whole (2 of 4), the other
        // two share the second (¼ each). Onsets 0, ½, ¾.
        assert_eq!(shape(3, 2), vec![(0, 4, 2), (2, 4, 1), (3, 4, 1)]);
    }

    #[test]
    fn a_pair_over_a_triplet_is_a_long_short() {
        // By hand: three host parts and two syllables; the first holds two of them. Onsets 0, ⅔.
        assert_eq!(shape(2, 3), vec![(0, 3, 2), (2, 3, 1)]);
    }

    #[test]
    fn more_crowded_beats_share_only_the_last_part() {
        // Four over two: one long, three sharing the last half in sixths — 0, ½, ⅔, ⅚.
        assert_eq!(
            shape(4, 2),
            vec![(0, 6, 3), (3, 6, 1), (4, 6, 1), (5, 6, 1)]
        );
        // Four over three: two on the host's first two parts, two sharing the third — 0, ⅓, ⅔, ⅚.
        assert_eq!(
            shape(4, 3),
            vec![(0, 6, 2), (2, 6, 2), (4, 6, 1), (5, 6, 1)]
        );
        // Three over four: the first holds two quarters — 0, ½, ¾.
        assert_eq!(shape(3, 4), vec![(0, 4, 2), (2, 4, 1), (3, 4, 1)]);
        // As many syllables as parts: one each, on the host's onsets.
        assert_eq!(shape(3, 3), vec![(0, 3, 1), (1, 3, 1), (2, 3, 1)]);
    }

    #[test]
    fn every_syllable_keeps_an_onset_of_its_own_and_the_beat_is_filled_once() {
        for parts in 1..=7 {
            for n in 1..=9 {
                let fitted = fit(n, parts);
                assert_eq!(fitted.len(), n, "{n} into {parts}");
                // Distinct onsets, in order, filling the beat exactly: each ends where the next
                // begins, and the last at the end of the beat.
                let mut at = 0;
                for sub in &fitted {
                    assert_eq!(sub.index(), at, "{n} into {parts}");
                    at += sub.parts();
                }
                assert_eq!(Some(at), fitted.first().map(|sub| sub.of()));
                // And never crossing the host it was fitted to.
                assert!(
                    !crosses(grid(fitted.iter().copied()), parts),
                    "{n} into {parts}"
                );
            }
        }
    }

    #[test]
    fn a_grid_is_the_onsets_reduced_denominators() {
        let at = |index, of| Subdivision::new(index, of);
        assert_eq!(grid([at(0, 2), at(1, 2)].into_iter()), 2);
        assert_eq!(grid([at(0, 3), at(1, 3), at(2, 3)].into_iter()), 3);
        // A lone onset on the beat is no subdivision at all; two of four on the halves are two.
        assert_eq!(grid([at(0, 3)].into_iter()), 1);
        assert_eq!(grid([at(0, 4), at(2, 4)].into_iter()), 2);
        assert!(crosses(3, 2));
        assert!(!crosses(2, 4));
        assert!(!crosses(1, 3));
        assert!(crosses(4, 6));
    }
}
