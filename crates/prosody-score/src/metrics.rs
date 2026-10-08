//! What each metric counts. Every one is a plain count over a [`Timeline`], so each can be checked
//! against a hand-worked example; the bands that turn a count into a judgement live in the crate
//! root, separately, because the counts are facts and the bands are opinions.

use std::collections::{BTreeMap, BTreeSet};

use crate::timeline::Timeline;

/// Mean number of distinct voices sounding in a beat, over the beats where anything sounds.
///
/// `SPEC.md` §4.1 calls the foundation "drums, bass, ostinato" and the flourish "fills, stabs,
/// one-shots, ornament" — a description of layers, which only exist if more than one thing plays
/// at once. 1.0 means the piece is a single line of sound.
pub(crate) fn layering(timeline: &Timeline) -> f64 {
    let mut by_slot: BTreeMap<u64, BTreeSet<usize>> = BTreeMap::new();
    for hit in &timeline.hits {
        by_slot.entry(hit.slot).or_default().insert(hit.voice);
    }
    let pairs: usize = by_slot.values().map(BTreeSet::len).sum();
    ratio(pairs, by_slot.len())
}

/// Share of what sounds that has sounded before: over every (bar, voice) pair, the ones whose
/// voice already played in an earlier bar.
///
/// Electronic music is built from loops, and §4.3 says exact repetition "gives the loop". A piece
/// where every bar is new material is through-composed, whatever its rhythm; one where nothing is
/// new has stopped going anywhere. Counted per voice rather than per bar, so a bar where a new
/// line arrives over a running loop is half familiar — not, as a per-bar count would have it,
/// entirely so.
pub(crate) fn recurrence(timeline: &Timeline) -> f64 {
    let mut by_bar: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for hit in &timeline.hits {
        by_bar.entry(hit.bar).or_default().insert(hit.voice);
    }
    let mut seen = BTreeSet::new();
    let mut returning = 0;
    let mut pairs = 0;
    for voices in by_bar.values() {
        pairs += voices.len();
        returning += voices.iter().filter(|voice| seen.contains(*voice)).count();
        seen.extend(voices.iter().copied());
    }
    ratio(returning, pairs)
}

/// Share of events that carry a pitch.
pub(crate) fn pitched(timeline: &Timeline) -> f64 {
    let with = timeline
        .hits
        .iter()
        .filter(|hit| hit.degree.is_some())
        .count();
    ratio(with, timeline.hits.len())
}

/// Shannon entropy, in bits, of the scale degrees the pitched events use.
///
/// The simplest stand-in for melodic information content (Pearce's `IDyOM` measures the same thing
/// conditioned on context). Zero bits is a drone or no melody at all; a few bits is a tune.
/// Summed in degree order, so the result is the same on every run (`CLAUDE.md` §4).
pub(crate) fn pitch_entropy(timeline: &Timeline) -> f64 {
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    for degree in timeline.hits.iter().filter_map(|hit| hit.degree) {
        *counts.entry(degree).or_default() += 1;
    }
    let total: usize = counts.values().sum();
    counts
        .values()
        .map(|&count| {
            let p = ratio(count, total);
            -p * p.log2()
        })
        .sum()
}

/// Longuet-Higgins & Lee syncopation, per beat, with accents as the notes.
///
/// LHL counts a syncopation wherever a note on a weak position is followed — before the next
/// note — by a silent position that is stronger, and weighs it by the difference in strength.
/// Every syllable here sounds, so with onsets as the notes there would never be a silent position;
/// what the ear hears as the rhythm is the *accents* (§4.2: "stress → accent"), and an iamb puts
/// its accent on the offbeat with the next beat unaccented. That is the syncopation this counts.
///
/// Witek et al. (2014) found the pleasure of groove peaks at *moderate* syncopation, which is why
/// the band for this in the crate root has an upper edge.
pub(crate) fn syncopation(timeline: &Timeline) -> f64 {
    let grid = timeline.grid();
    let mut total = 0_usize;
    for (index, note) in grid.iter().enumerate() {
        if !note.accented {
            continue;
        }
        let deepest_rest = grid
            .iter()
            .skip(index + 1)
            .take_while(|position| !position.accented)
            .map(|rest| (note.depth as usize).saturating_sub(rest.depth as usize))
            .max()
            .unwrap_or(0);
        total += deepest_rest;
    }
    ratio(total, usize::try_from(timeline.beats).unwrap_or(usize::MAX))
}

/// `numerator / denominator`, or zero for an empty denominator.
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    as_f64(numerator) / as_f64(denominator)
}

fn as_f64(count: usize) -> f64 {
    // Counts are bounded by the number of syllables in a poem; `u32` holds any of them exactly.
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::{layering, pitch_entropy, pitched, recurrence, syncopation};
    use crate::timeline::tests::hit;
    use crate::timeline::{Hit, Timeline};

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    const fn accented(mut hit: Hit) -> Hit {
        hit.accent = true;
        hit
    }

    const fn on(mut hit: Hit, degree: usize) -> Hit {
        hit.degree = Some(degree);
        hit
    }

    /// One bar of two iambs: `da DUM da DUM`, each foot one beat of two syllables.
    fn iambs(bars: usize) -> Timeline {
        let mut hits = Vec::new();
        for bar in 0..bars {
            let start = 2 * bar as u64;
            for beat in 0..2 {
                hits.push(hit(start + beat, bar, (0, 2), 0));
                hits.push(accented(hit(start + beat, bar, (1, 2), 0)));
            }
        }
        Timeline {
            hits,
            bar_starts: (0..bars as u64).map(|bar| 2 * bar).collect(),
            beats: 2 * bars as u64,
        }
    }

    #[test]
    fn a_single_line_of_sound_has_layering_one() {
        close(layering(&iambs(2)), 1.0);
    }

    #[test]
    fn layering_counts_voices_per_sounding_beat() {
        // Beat 0: voices 0 and 1. Beat 1: voice 0. Beat 2: silent, so not counted.
        // (2 + 1) / 2 sounding beats = 1.5.
        let timeline = Timeline {
            hits: vec![
                hit(0, 0, (0, 1), 0),
                hit(0, 0, (0, 1), 1),
                hit(1, 0, (0, 2), 0),
                hit(1, 0, (1, 2), 0),
            ],
            bar_starts: vec![0],
            beats: 3,
        };
        close(layering(&timeline), 1.5);
    }

    #[test]
    fn recurrence_counts_what_sounds_again() {
        // Voices by bar: {0}, {1}, {0}, {2, 1} — five (bar, voice) pairs. Voice 0 in bar 2 and
        // voice 1 in bar 3 have played before: 2 of 5.
        let timeline = Timeline {
            hits: vec![
                hit(0, 0, (0, 1), 0),
                hit(1, 1, (0, 1), 1),
                hit(2, 2, (0, 1), 0),
                hit(3, 3, (0, 1), 2),
                hit(3, 3, (0, 1), 1),
            ],
            bar_starts: vec![0, 1, 2, 3],
            beats: 4,
        };
        close(recurrence(&timeline), 0.4);
    }

    #[test]
    fn pitch_share_and_entropy() {
        // Four events, three pitched, on degrees 0, 0, 1: p = (2/3, 1/3).
        // H = -(2/3·log2(2/3) + 1/3·log2(1/3)) = log2(3) - 2/3 ≈ 0.918 bits.
        let timeline = Timeline {
            hits: vec![
                hit(0, 0, (0, 1), 0),
                on(hit(1, 0, (0, 1), 0), 0),
                on(hit(2, 0, (0, 1), 0), 0),
                on(hit(3, 0, (0, 1), 0), 1),
            ],
            bar_starts: vec![0],
            beats: 4,
        };
        close(pitched(&timeline), 0.75);
        close(pitch_entropy(&timeline), 3_f64.log2() - 2.0 / 3.0);
    }

    #[test]
    fn no_melody_is_zero_bits() {
        close(pitch_entropy(&iambs(1)), 0.0);
    }

    #[test]
    fn an_iamb_syncopates_against_the_next_beat() {
        // Grid of one bar: 0 (downbeat) · ½ (offbeat, accent) · 1 (beat) · 1½ (offbeat, accent).
        // The first accent sits at depth 2 and is followed by the unaccented beat at depth 1:
        // one unit. The second is followed by nothing. 1 unit over 2 beats.
        close(syncopation(&iambs(1)), 0.5);
        // Add a second bar: the last accent of bar 0 now precedes bar 1's unaccented downbeat at
        // depth 0, worth two units. Bar 0: 1 + 2, bar 1: 1 + 0. 4 units over 4 beats.
        close(syncopation(&iambs(2)), 1.0);
    }

    #[test]
    fn a_trochee_does_not_syncopate() {
        // `DUM da DUM da`: accents on the beat, rests only on weaker offbeats.
        let timeline = Timeline {
            hits: vec![
                accented(hit(0, 0, (0, 2), 0)),
                hit(0, 0, (1, 2), 0),
                accented(hit(1, 0, (0, 2), 0)),
                hit(1, 0, (1, 2), 0),
            ],
            bar_starts: vec![0],
            beats: 2,
        };
        close(syncopation(&timeline), 0.0);
    }

    #[test]
    fn an_empty_piece_measures_zero_everywhere() {
        let empty = Timeline {
            hits: vec![],
            bar_starts: vec![],
            beats: 0,
        };
        for value in [
            layering(&empty),
            recurrence(&empty),
            pitched(&empty),
            pitch_entropy(&empty),
            syncopation(&empty),
        ] {
            close(value, 0.0);
        }
    }
}
