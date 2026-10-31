//! A compressor (`docs/player.md` §4): a channel's (`.compressor`), and the master's glue.
//!
//! The design is the feed-forward compressor of Giannoulis, Massberg and Reiss, "Digital Dynamic
//! Range Compressor Design — A Tutorial and Analysis" (JAES 60(6), 2012): a gain computer with a
//! quadratic soft knee, its gain reduction smoothed in decibels by a branching one-pole (fast down,
//! slow back). Its makeup gain is the one the Web Audio specification gives its
//! `DynamicsCompressorNode` — `(1 / curve(0 dBFS))^0.6` — because Strudel's compressor *is* that
//! node, and the page's glue was one too.
//!
//! **Matched by measurement** (`docs/player.md` §11). Strudel's per-layer compressor measured as
//! Chrome's node with a 10 dB knee, 3 ms attack and 20 ms release (its steady output for a 110 Hz
//! sine matched Chrome's at those settings to 0.0001; Strudel's source was not read). Chrome's own
//! node was then measured offline over 24 attack/release pairs, and the smoothing's time constants
//! were fitted to its steady levels: [`ATTACK_SCALE`] and [`RELEASE_SCALE`] times the stated
//! seconds, within 0.17 dB rms. Its knee is not Chrome's exact curve (which the specification
//! leaves to the browser): the quadratic knee reduces up to 0.4 dB less, and the makeup computed
//! from it is 0.4 dB less, so the two nearly cancel on a signal near the threshold.
//!
//! The decibels come from the synth's own `log2` and `exp2` (`math`), so the result is the same bits
//! everywhere (I1).

use crate::math;

/// The smoothing's attack time constant, as a share of the stated attack (fitted, see above).
pub const ATTACK_SCALE: f64 = 0.2;
/// The smoothing's release time constant, as a multiple of the stated release (fitted, see above).
pub const RELEASE_SCALE: f64 = 6.0;
/// Below this a sample counts as −100 dB: the detector does not take the logarithm of silence.
const FLOOR: f64 = 1e-5;

/// A compressor's full setting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Setting {
    /// Where it starts, dBFS.
    pub threshold: f64,
    /// `n` for `n`:1 above the knee.
    pub ratio: f64,
    /// How many decibels above the threshold the knee bends over.
    pub knee: f64,
    /// Its attack, seconds.
    pub attack: f64,
    /// Its release, seconds.
    pub release: f64,
}

impl Setting {
    /// A layer's compressor at `threshold` and `ratio`, the rest as Strudel's measured: a 10 dB knee,
    /// 3 ms attack, 20 ms release.
    #[must_use]
    pub const fn layer(threshold: f64, ratio: f64) -> Self {
        Self {
            threshold,
            ratio,
            knee: 10.0,
            attack: 0.003,
            release: 0.02,
        }
    }

    /// The master's glue, as the page set its `DynamicsCompressorNode` (`web/src/strudel/index.ts`):
    /// −6 dB, 8:1, a 6 dB knee, 3 ms attack, 150 ms release.
    pub const MASTER: Self = Self {
        threshold: -6.0,
        ratio: 8.0,
        knee: 6.0,
        attack: 0.003,
        release: 0.15,
    };

    /// The static curve: the level out, in dB, for a level in, in dB. Unity below the threshold;
    /// over the knee, from the threshold to `knee` above it, a parabola that leaves with slope
    /// 1/ratio; above that a straight line at 1/ratio (Giannoulis et al., eq. 4, with the knee
    /// starting at the threshold as Web Audio's does rather than centred on it).
    #[must_use]
    pub fn curve(&self, x: f64) -> f64 {
        let over = x - self.threshold;
        let slope = 1.0 / self.ratio - 1.0;
        if over <= 0.0 {
            x
        } else if self.knee > 0.0 && over < self.knee {
            x + slope * over * over / (2.0 * self.knee)
        } else {
            x + slope * (over - self.knee / 2.0)
        }
    }

    /// The makeup gain, in dB: Web Audio's `(1 / curve(1.0))^0.6`.
    #[must_use]
    pub fn makeup(&self) -> f64 {
        -0.6 * self.curve(0.0)
    }
}

/// A running compressor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compressor {
    setting: Setting,
    attack: f64,
    release: f64,
    makeup: f64,
    /// The smoothed gain reduction, dB, never above 0.
    reduction: f64,
}

impl Compressor {
    /// A compressor set to `setting` at `rate` frames a second, at rest.
    #[must_use]
    pub fn new(setting: Setting, rate: f64) -> Self {
        let coefficient = |seconds: f64| {
            if seconds > 0.0 {
                math::exp(-1.0 / (seconds * rate))
            } else {
                0.0
            }
        };
        Self {
            setting,
            attack: coefficient(setting.attack * ATTACK_SCALE),
            release: coefficient(setting.release * RELEASE_SCALE),
            makeup: setting.makeup(),
            reduction: 0.0,
        }
    }

    /// Its setting.
    #[must_use]
    pub const fn setting(&self) -> Setting {
        self.setting
    }

    /// The gain for a frame whose level (its largest side, as an amplitude) is `level`: the
    /// smoothed reduction and the makeup, as a factor.
    pub fn gain(&mut self, level: f64) -> f64 {
        let x = math::decibels(level.abs().max(FLOOR));
        let wanted = self.setting.curve(x) - x;
        let a = if wanted < self.reduction {
            self.attack
        } else {
            self.release
        };
        self.reduction = a * self.reduction + (1.0 - a) * wanted;
        math::from_decibels(self.reduction + self.makeup)
    }
}

#[cfg(test)]
mod tests {
    use super::{Compressor, Setting};

    #[test]
    fn the_curve_is_unity_below_bends_over_the_knee_and_then_falls_at_the_ratio() {
        let s = Setting::layer(-24.0, 7.0);
        assert!((s.curve(-40.0) + 40.0).abs() < 1e-12);
        // 4 dB into the 10 dB knee: −20 + (1/7 − 1)·16/20 = −20.6857.
        assert!((s.curve(-20.0) + 20.685_714).abs() < 1e-5);
        // At the knee's end the slope is 1/7, and on from there: −14 + (1/7 − 1)·5 = −18.2857;
        // 14 dB further, at 0 dBFS, 2 dB more: −16.2857.
        assert!((s.curve(-14.0) + 18.285_714).abs() < 1e-5);
        assert!((s.curve(0.0) + 16.285_714).abs() < 1e-5);
    }

    #[test]
    fn the_makeup_is_web_audios() {
        // 0.6 × 16.2857 = 9.771 dB for a layer at −24 dB, 7:1 (Chrome measured 10.16); the master's
        // curve at 0 dBFS is −6 + 3 + 3/8 = −2.625, so 1.575 dB (Chrome measured 1.96).
        assert!((Setting::layer(-24.0, 7.0).makeup() - 9.771_43).abs() < 1e-4);
        assert!((Setting::MASTER.makeup() - 1.575).abs() < 1e-9);
    }

    #[test]
    fn below_the_threshold_it_is_the_makeup_alone() {
        // A steady −40 dB is untouched by the curve: the gain is 10^(9.7714/20) = 3.0801.
        let mut c = Compressor::new(Setting::layer(-24.0, 7.0), 48_000.0);
        let mut g = 0.0;
        for _ in 0..48_000 {
            g = c.gain(0.01);
        }
        assert!((g - 3.080_1).abs() < 1e-4, "{g}");
    }

    #[test]
    fn a_loud_step_is_caught_fast_and_let_go_slowly() {
        // Held at 0 dBFS the reduction settles at the curve's 16.2857 dB; the attack's time constant
        // is 0.2 × 3 ms = 0.6 ms (29 frames), so within 5 ms (240 frames, 8 time constants) it is
        // there to 0.01%. Let go to silence, the release's is 6 × 20 ms = 120 ms: after 120 ms
        // the reduction has recovered all but e^-1 of it (5.99 dB).
        let mut c = Compressor::new(Setting::layer(-24.0, 7.0), 48_000.0);
        for _ in 0..240 {
            c.gain(1.0);
        }
        assert!((c.reduction + 16.285_714).abs() < 0.01, "{}", c.reduction);
        for _ in 0..5_760 {
            c.gain(0.0);
        }
        assert!(
            (c.reduction + 16.285_714 * (-1.0_f64).exp()).abs() < 0.01,
            "{}",
            c.reduction
        );
    }
}
