//! The formant filter (`vowel`, `docs/player.md` §4): five band-passes in parallel, each at a
//! formant of the vowel, each at its own level — the classic formant synthesis bank (Klatt,
//! "Software for a cascade/parallel formant synthesizer", JASA 1980, its parallel branch).
//!
//! Where the numbers come from: the five bands started from the tenor formants of the Csound
//! manual's formant table, and were then fitted to Strudel's *output* (`docs/player.md` §11) —
//! white noise and a 110 Hz sawtooth played through each of its vowels, the response read off their
//! spectra, and the centres, Qs and levels below fitted to it by least squares (weighted towards
//! the peaks, which is what a listener hears; 1.2–1.6 dB rms over 0–8 kHz). Strudel's source was
//! not read. Its bands are much narrower than a voice's, so a note's harmonics ring out where they
//! fall on a formant and are cut almost away where they fall between.

use crate::effects::Vowel;
use crate::filter::{Biquad, Coefficients};
use crate::math;

/// One band: its centre in Hz, its Q, its level in dB.
type Band = (f64, f64, f64);

/// The bands of each vowel, fitted to Strudel's output (see the module's documentation).
const BANDS: [[Band; 5]; 5] = [
    // a
    [
        (661.0, 74.6, 18.1),
        (1_120.0, 65.9, 10.2),
        (2_749.0, 104.8, -5.3),
        (2_998.0, 107.3, -7.1),
        (3_344.0, 114.7, -22.9),
    ],
    // e
    [
        (440.0, 62.2, 18.0),
        (1_799.0, 64.1, 3.0),
        (2_699.0, 81.7, -0.8),
        (2_999.0, 98.8, -2.9),
        (3_297.0, 107.9, -2.4),
    ],
    // i
    [
        (270.0, 19.4, 12.6),
        (1_848.0, 71.5, -7.2),
        (2_899.0, 82.0, -6.9),
        (3_344.0, 117.0, -18.1),
        (3_583.0, 102.2, -19.4),
    ],
    // o
    [
        (429.0, 29.7, 16.5),
        (816.0, 56.0, 5.7),
        (2_698.0, 85.8, -8.5),
        (2_999.0, 98.6, -4.9),
        (3_293.0, 109.5, -17.6),
    ],
    // u
    [
        (370.0, 28.6, 16.1),
        (620.0, 33.1, -7.2),
        (2_749.0, 87.3, -5.3),
        (2_996.0, 102.1, -13.2),
        (3_396.0, 108.4, -16.5),
    ],
];

/// A vowel's five bands, ready to run on both sides of a voice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Formants {
    coefficients: [Coefficients; 5],
    levels: [f64; 5],
    left: [Biquad; 5],
    right: [Biquad; 5],
}

impl Formants {
    /// The bank for `vowel` at `rate` frames a second.
    pub(crate) fn new(vowel: Vowel, rate: f64) -> Self {
        let bands = match vowel {
            Vowel::A => BANDS[0],
            Vowel::E => BANDS[1],
            Vowel::I => BANDS[2],
            Vowel::O => BANDS[3],
            Vowel::U => BANDS[4],
        };
        Self {
            coefficients: bands.map(|(centre, q, _)| Coefficients::band_pass(centre, q, rate)),
            levels: bands.map(|(_, _, level)| math::from_decibels(level)),
            left: [Biquad::default(); 5],
            right: [Biquad::default(); 5],
        }
    }

    /// One frame through the bank, both sides.
    pub(crate) fn process(&mut self, x: f64, y: f64) -> (f64, f64) {
        let mut out = (0.0, 0.0);
        for (((c, level), left), right) in self
            .coefficients
            .iter()
            .zip(self.levels)
            .zip(&mut self.left)
            .zip(&mut self.right)
        {
            out.0 += level * left.process(c, x);
            out.1 += level * right.process(c, y);
        }
        out
    }

    /// One frame through the left side's bands alone: [`Self::process`]'s left output, the same
    /// bits, for a voice whose sides are the same.
    pub(crate) fn process_mono(&mut self, x: f64) -> f64 {
        let mut out = 0.0;
        for ((c, level), left) in self
            .coefficients
            .iter()
            .zip(self.levels)
            .zip(&mut self.left)
        {
            out += level * left.process(c, x);
        }
        out
    }

    /// The right side's bands given the left's state: where they would be had they been run on
    /// the same frames.
    pub(crate) fn level_right(&mut self) {
        self.right = self.left;
    }
}

#[cfg(test)]
mod tests {
    use super::Formants;
    use crate::effects::Vowel;

    /// The steady amplitude of a unit sine at `hertz` through the bank.
    fn gain(vowel: Vowel, hertz: f64) -> f64 {
        let mut bank = Formants::new(vowel, 48_000.0);
        let mut peak = 0.0_f64;
        // Q 75 at 661 Hz rings for about Q/(π·f) = 36 ms; two seconds is long settled.
        for n in 0..100_000 {
            let x = (std::f64::consts::TAU * hertz * f64::from(n) / 48_000.0).sin();
            let (y, _) = bank.process(x, x);
            if n >= 96_000 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn a_vowel_rings_at_its_first_formant_at_that_bands_level() {
        // At /a/'s first centre, 661 Hz, the first band passes at its level, 18.1 dB (×8.04); the
        // next nearest, Q 65.9 at 1120 Hz, adds at most 1/(Q·|r − 1/r|) = 1/(65.9 × 1.104) = 0.0137
        // of its 10.2 dB (×3.24): 0.044. So 8.04 ± 0.05.
        let at = gain(Vowel::A, 661.0);
        assert!((at - 8.04).abs() < 0.06, "{at}");
    }

    #[test]
    fn between_formants_almost_nothing_passes() {
        // Halfway between /a/'s first two formants, 890 Hz: the first band gives
        // 8.04/(74.6 × |1.346 − 0.743|) = 0.179 and the second 3.24/(65.9 × |0.795 − 1.258|) =
        // 0.106, of opposite phase (one above its centre, one below), so at most 0.29 — 25 dB
        // under the peak.
        let between = gain(Vowel::A, 890.0);
        assert!(between < 0.29, "{between}");
    }

    #[test]
    fn silence_stays_silent() {
        let mut bank = Formants::new(Vowel::U, 48_000.0);
        for _ in 0..1_000 {
            assert_eq!(bank.process(0.0, 0.0), (0.0, 0.0));
        }
    }
}
