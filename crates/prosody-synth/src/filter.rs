//! The note's low-pass (`lpf`) and the formant filter's band-passes (`vowel`, `docs/player.md` §4):
//! two-pole biquads, from Robert Bristow-Johnson's "Cookbook formulae for audio EQ biquad filter
//! coefficients".
//!
//! The low-pass's Q is [`LOW_PASS_Q`], matched to Strudel by measurement (`docs/player.md` §11): a
//! sawtooth through Strudel's `lpf` peaks +2 dB about 0.77 of the way to the cutoff, is +1 dB at
//! it and falls 12 dB an octave above it — a single two-pole stage at Q 1.12.

use crate::math;

/// The Butterworth Q: −3 dB at the cutoff, maximally flat below it.
pub const BUTTERWORTH_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// The low-pass's Q, `10^(1/20)` = 1.122: +1 dB at the cutoff. Measured from Strudel's output
/// (`docs/player.md` §11) — it is Web Audio's default lowpass resonance, which that API states in
/// decibels (1 dB).
pub const LOW_PASS_Q: f64 = 1.122_018_454_301_963_3;

/// The coefficients of a biquad, normalised so `a0` is 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coefficients {
    /// A low-pass at `cutoff` Hz with quality `q`, sampled `rate` times a second. A cutoff at or
    /// above 0.45 of the rate is held there: past it the bilinear transform's warping makes the
    /// filter mean nothing a listener could hear as a cutoff, and at the rate's half it is undefined.
    #[must_use]
    pub fn low_pass(cutoff: f64, q: f64, rate: f64) -> Self {
        let cutoff = cutoff.clamp(10.0, 0.45 * rate);
        let w0 = std::f64::consts::TAU * cutoff / rate;
        let (sin, cos) = math::sin_cos(w0);
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let b1 = (1.0 - cos) / a0;
        Self {
            b0: b1 / 2.0,
            b1,
            b2: b1 / 2.0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    /// A band-pass centred on `centre` Hz with quality `q`, 0 dB at its centre (the cookbook's
    /// "constant 0 dB peak gain" form), sampled `rate` times a second; the centre is held inside
    /// 10 Hz..0.45 of the rate as the low-pass's cutoff is.
    #[must_use]
    pub fn band_pass(centre: f64, q: f64, rate: f64) -> Self {
        let centre = centre.clamp(10.0, 0.45 * rate);
        let w0 = std::f64::consts::TAU * centre / rate;
        let (sin, cos) = math::sin_cos(w0);
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b1: 0.0,
            b2: -alpha / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
        }
    }
}

/// One channel's filter state: transposed direct form II, which needs two numbers of memory and is
/// well behaved in floating point.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Biquad {
    s1: f64,
    s2: f64,
}

impl Biquad {
    /// Filters one sample.
    pub fn process(&mut self, c: &Coefficients, x: f64) -> f64 {
        let y = c.b0 * x + self.s1;
        self.s1 = c.b1 * x - c.a1 * y + self.s2;
        self.s2 = c.b2 * x - c.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::{BUTTERWORTH_Q, Biquad, Coefficients, LOW_PASS_Q};

    const RATE: f64 = 48_000.0;

    /// The steady-state amplitude of a unit sine at `hertz` through a low-pass at `cutoff`: its
    /// peak after the filter has settled (a second in, measured over the next tenth).
    fn gain_at(hertz: f64, cutoff: f64) -> f64 {
        through(Coefficients::low_pass(cutoff, BUTTERWORTH_Q, RATE), hertz)
    }

    /// The steady-state amplitude of a unit sine at `hertz` through `c`.
    fn through(c: Coefficients, hertz: f64) -> f64 {
        let mut filter = Biquad::default();
        let mut peak = 0.0_f64;
        for n in 0..52_800 {
            let x = (std::f64::consts::TAU * hertz * f64::from(n) / RATE).sin();
            let y = filter.process(&c, x);
            if n >= 48_000 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn minus_three_decibels_at_the_cutoff() {
        // Butterworth: |H(fc)| = 1/√2 exactly, the bilinear transform being prewarped to fc.
        let gain = gain_at(1_000.0, 1_000.0);
        assert!(
            (gain - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.005,
            "{gain}"
        );
    }

    #[test]
    fn unity_well_below_and_twelve_decibels_an_octave_above() {
        // A tenth of the cutoff passes untouched; two-pole roll-off is −12 dB an octave, so three
        // octaves above is about −36 dB (0.016), and the bilinear transform only steepens it.
        assert!((gain_at(100.0, 1_000.0) - 1.0).abs() < 0.005);
        let far = gain_at(8_000.0, 1_000.0);
        assert!(far < 0.017, "{far}");
        assert!(far > 0.008, "{far}");
    }

    #[test]
    fn the_measured_q_is_one_decibel_up_at_the_cutoff_and_peaks_below_it() {
        // A two-pole low-pass is |H| = Q at its cutoff: 10^(1/20), +1 dB. Its peak is
        // Q/√(1 − 1/4Q²) = 1.122/0.8952 = 1.2533 (+1.96 dB) at fc·√(1 − 1/2Q²) = 0.777·fc, where
        // Strudel's sawtooth measured +2.0 dB at 770 Hz under a 1 kHz cutoff.
        let at = through(Coefficients::low_pass(1_000.0, LOW_PASS_Q, RATE), 1_000.0);
        assert!((at - LOW_PASS_Q).abs() < 0.005, "{at}");
        let peak = through(Coefficients::low_pass(1_000.0, LOW_PASS_Q, RATE), 777.0);
        assert!((peak - 1.2533).abs() < 0.005, "{peak}");
    }

    #[test]
    fn a_band_pass_is_unity_at_its_centre_and_falls_away_either_side() {
        // Constant 0 dB peak: |H(f0)| = 1. With Q = 10 a sine an octave off falls to
        // 1/√(1 + Q²(2 − 1/2)²) = 1/√226 = 0.0665 above and the same below.
        let c = Coefficients::band_pass(1_000.0, 10.0, RATE);
        assert!((through(c, 1_000.0) - 1.0).abs() < 0.005);
        let above = through(c, 2_000.0);
        let below = through(c, 500.0);
        assert!((above - 0.0665).abs() < 0.003, "{above}");
        assert!((below - 0.0665).abs() < 0.003, "{below}");
    }

    #[test]
    fn a_constant_passes_at_full_level() {
        // DC gain is (b0 + b1 + b2)/(1 + a1 + a2) = 1 for a low-pass.
        let c = Coefficients::low_pass(500.0, BUTTERWORTH_Q, RATE);
        let mut filter = Biquad::default();
        let mut y = 0.0;
        for _ in 0..10_000 {
            y = filter.process(&c, 1.0);
        }
        assert!((y - 1.0).abs() < 1e-9);
    }
}
