//! Mixing the stems to stereo, through the master's ceiling: for a render to a file.
//!
//! On the page the master stage runs in the audio worklet (`web/src/player/worklet.ts`), where mute
//! and solo are stem gains (`docs/player.md` §4). A file needs the same last stage, so the CLI's
//! render goes through this one. The soft clip is the page's own (`web/src/strudel/index.ts`
//! `softClipCurve`, our code): transparent below [`CLIP_KNEE`], bending smoothly towards
//! [`CLIP_CEILING`] above it, flat beyond [`CLIP_RANGE`]. The glue compressor before it arrives
//! with step 3, with the rest of the master (`docs/player.md` §7).

use crate::math;
use crate::note::Channel;
use crate::stems::{STEMS, Stems};

/// Where the soft clip starts to bend.
pub const CLIP_KNEE: f64 = 0.6;
/// What it bends towards and never passes.
pub const CLIP_CEILING: f64 = 0.95;
/// Beyond this the curve is flat at its last value, as the page's curve is clamped at its ends.
pub const CLIP_RANGE: f64 = 4.0;

/// The soft clip of one sample.
#[must_use]
pub fn soft_clip(x: f64) -> f64 {
    let a = x.abs().min(CLIP_RANGE);
    let bend = CLIP_CEILING - CLIP_KNEE;
    let shaped = if a <= CLIP_KNEE {
        a
    } else {
        CLIP_KNEE + bend * math::tanh((a - CLIP_KNEE) / bend)
    };
    if x < 0.0 { -shaped } else { shaped }
}

/// The stems summed, each at its gain, through the soft clip, appended to `out` interleaved
/// left, right.
pub fn mix(stems: &Stems, gains: [f64; STEMS], out: &mut Vec<f32>) {
    for frame in 0..stems.frames() {
        let mut left = 0.0;
        let mut right = 0.0;
        for (channel, gain) in Channel::ALL.iter().zip(gains) {
            left += gain
                * stems
                    .side(*channel, false)
                    .get(frame)
                    .map_or(0.0, |x| f64::from(*x));
            right += gain
                * stems
                    .side(*channel, true)
                    .get(frame)
                    .map_or(0.0, |x| f64::from(*x));
        }
        #[allow(clippy::cast_possible_truncation)] // within ±0.95 after the clip
        {
            out.push(soft_clip(left) as f32);
            out.push(soft_clip(right) as f32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CLIP_CEILING, CLIP_KNEE, mix, soft_clip};
    use crate::note::Channel;
    use crate::stems::Stems;

    #[test]
    fn transparent_below_the_knee_and_never_past_the_ceiling() {
        assert!((soft_clip(0.5) - 0.5).abs() < 1e-15);
        assert!((soft_clip(-CLIP_KNEE) + CLIP_KNEE).abs() < 1e-15);
        // Just above the knee the curve still has slope 1: tanh's slope at 0.
        let above = soft_clip(CLIP_KNEE + 1e-6);
        assert!((above - (CLIP_KNEE + 1e-6)).abs() < 1e-9);
        // knee + bend·tanh(1) at one bend above the knee: 0.6 + 0.35 × 0.761594 = 0.866558.
        assert!(
            (soft_clip(0.95) - 0.866_558).abs() < 1e-6,
            "{}",
            soft_clip(0.95)
        );
        for x in [1.0, 2.0, 4.0, 100.0] {
            assert!(soft_clip(x) < CLIP_CEILING && soft_clip(-x) > -CLIP_CEILING);
        }
        assert!((soft_clip(100.0) - soft_clip(4.0)).abs() < 1e-15);
    }

    #[test]
    fn stems_sum_at_their_gains() {
        let mut stems = Stems::new(2);
        stems.sides_mut(Channel::Drums, 0, 2).unwrap().0.fill(0.25);
        stems.sides_mut(Channel::Bass, 0, 2).unwrap().1.fill(0.5);
        let mut out = Vec::new();
        mix(&stems, [1.0, 0.5, 1.0, 1.0], &mut out);
        // Left: the drums' 0.25; right: half the bass's 0.5.
        assert_eq!(out, vec![0.25, 0.25, 0.25, 0.25]);
    }
}
