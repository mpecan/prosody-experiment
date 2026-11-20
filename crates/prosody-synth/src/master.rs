//! The master stage: the stems mixed to stereo, for a render to a file.
//!
//! On the page the master runs in the audio worklet (`web/src/player/mixer.ts`), where mute and solo
//! are stem gains (`docs/player.md` §4). A file needs the same last stage, so the CLI's render goes
//! through this one. Both are the chain the page built for Strudel (`web/src/strudel/index.ts`, our
//! own code), in its order:
//!
//! 1. each stem at its gain;
//! 2. the music — every stem but the reading's — at the duck, the level the block's duck lane gives
//!    each frame, so the reading is heard over it ([`crate::Stems`]);
//! 3. the glue compressor ([`Setting::MASTER`]), on the music and the reading together;
//! 4. the soft clip: transparent below [`CLIP_KNEE`], bending smoothly towards [`CLIP_CEILING`]
//!    above it, flat beyond [`CLIP_RANGE`]. The page fed its curve through a gain of 1/4 because a
//!    `WaveShaperNode`'s curve spans −1..1; here the curve is computed on the sample itself, so
//!    that quarter and the curve's ×4 domain cancel and neither appears.

use crate::dynamics::{Compressor, Setting};
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

/// The master: its glue compressor's state, kept from block to block.
#[derive(Debug, Clone, PartialEq)]
pub struct Master {
    glue: Compressor,
}

impl Master {
    /// A master at `rate` frames a second.
    #[must_use]
    pub fn new(rate: f64) -> Self {
        Self {
            glue: Compressor::new(Setting::MASTER, rate),
        }
    }

    /// The stems, each at its gain, the music ducked, through the glue and the soft clip,
    /// appended to `out` interleaved left, right.
    pub fn mix(&mut self, stems: &Stems, gains: [f64; STEMS], out: &mut Vec<f32>) {
        let duck = stems.duck();
        for frame in 0..stems.frames() {
            let (mut music, mut speech) = ((0.0, 0.0), (0.0, 0.0));
            for (channel, gain) in Channel::ALL.iter().zip(gains) {
                let at = |right| {
                    stems
                        .side(*channel, right)
                        .get(frame)
                        .map_or(0.0, |x| f64::from(*x))
                };
                let sum = if *channel == Channel::Speech {
                    &mut speech
                } else {
                    &mut music
                };
                sum.0 += gain * at(false);
                sum.1 += gain * at(true);
            }
            let ducked = duck.get(frame).map_or(1.0, |d| f64::from(*d));
            let left = ducked * music.0 + speech.0;
            let right = ducked * music.1 + speech.1;
            let glue = self.glue.gain(left.abs().max(right.abs()));
            #[allow(clippy::cast_possible_truncation)] // within ±0.95 after the clip
            {
                out.push(soft_clip(glue * left) as f32);
                out.push(soft_clip(glue * right) as f32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CLIP_CEILING, CLIP_KNEE, Master, soft_clip};
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
    fn stems_sum_at_their_gains_through_the_glues_makeup() {
        // Quiet stems, far below the glue's −6 dB threshold: only its makeup acts, 1.575 dB
        // (×1.19881). Left: the drums' 0.05; right: half the bass's 0.1 — both 0.05 × 1.19881 =
        // 0.059940.
        let mut stems = Stems::new(2);
        stems.sides_mut(Channel::Drums).unwrap().0.fill(0.05);
        stems.sides_mut(Channel::Bass).unwrap().1.fill(0.1);
        let mut out = Vec::new();
        Master::new(48_000.0).mix(&stems, [1.0, 0.5, 1.0, 1.0, 1.0], &mut out);
        for x in out {
            assert!((x - 0.059_940).abs() < 1e-5, "{x}");
        }
    }

    #[test]
    fn the_duck_lowers_the_music_and_not_the_reading() {
        // Duck at 0.6: the melody's 0.05 becomes 0.03, the reading's 0.05 stays; both then take the
        // glue's makeup, ×1.19881: 0.035964 + 0.059940 = 0.095905.
        let mut stems = Stems::new(1);
        stems.sides_mut(Channel::Melody).unwrap().0.fill(0.05);
        stems.sides_mut(Channel::Speech).unwrap().0.fill(0.05);
        stems.duck_mut()[0] = 0.6;
        let mut out = Vec::new();
        Master::new(48_000.0).mix(&stems, [1.0; 5], &mut out);
        assert!((out[0] - 0.095_905).abs() < 1e-5, "{}", out[0]);
    }
}
