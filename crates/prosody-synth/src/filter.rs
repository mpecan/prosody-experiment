//! The note's low-pass (`lpf`, `docs/player.md` §4): a two-pole biquad, from Robert
//! Bristow-Johnson's "Cookbook formulae for audio EQ biquad filter coefficients".
//!
//! Q is fixed at 1/√2, the Butterworth response: flat below the cutoff, −3 dB at it, no resonant
//! peak. Strudel's resonance at the text's default is to be matched by measuring its output in
//! step 3 (`docs/player.md` §7); this is where that number goes.

use crate::math;

/// The Butterworth Q: −3 dB at the cutoff, maximally flat below it.
pub const BUTTERWORTH_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

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
    use super::{BUTTERWORTH_Q, Biquad, Coefficients};

    const RATE: f64 = 48_000.0;

    /// The steady-state amplitude of a unit sine at `hertz` through a low-pass at `cutoff`: its
    /// peak after the filter has settled (a second in, measured over the next tenth).
    fn gain_at(hertz: f64, cutoff: f64) -> f64 {
        let c = Coefficients::low_pass(cutoff, BUTTERWORTH_Q, RATE);
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
