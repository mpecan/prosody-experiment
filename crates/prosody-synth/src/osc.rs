//! The four oscillator waves the text can ask for (`docs/player.md` §4): sine, sawtooth, square and
//! triangle.
//!
//! A naive sawtooth or square jumps between two values in one sample, and the harmonics of that
//! jump above half the sample rate fold back down as inharmonic tones — audible on a high note.
//! Each jump is smoothed with a polynomial band-limited step (`PolyBLEP`: Välimäki and Huovilainen,
//! "Antialiasing Oscillators in Subtractive Synthesis", IEEE Signal Processing Magazine, 2007), and
//! each corner of the triangle with its integral, the band-limited ramp (`PolyBLAMP`: Esqueda,
//! Välimäki and Bilbao, "Rounding Corners with BLAMP", `DAFx` 2016). Two samples either side of each
//! discontinuity are corrected; everywhere else the wave is the naive one.

use crate::math;

/// An oscillator's waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Wave {
    /// A sine.
    Sine,
    /// A sawtooth, rising.
    Sawtooth,
    /// A square, high for the first half of each period.
    Square,
    /// A triangle, from −1 at the start of a period to 1 halfway.
    Triangle,
}

/// The polynomial band-limited step's residual: what to add to a naive unit step at phase 0 to
/// smooth it, for a phase `t` in 0..1 advancing `dt` a sample.
fn blep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// The band-limited ramp's residual: the integral of [`blep`], for a corner at phase 0 whose slope
/// rises by one per sample.
fn blamp(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt - 1.0;
        -x * x * x / 3.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt + 1.0;
        x * x * x / 3.0
    } else {
        0.0
    }
}

/// A running oscillator: its wave, and where it is in its period.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oscillator {
    wave: Wave,
    /// The phase, in periods: 0..1.
    phase: f64,
    /// How far the phase moves a sample: the frequency over the sample rate, at most 0.5.
    step: f64,
}

impl Oscillator {
    /// An oscillator at `hertz`, sampled `rate` times a second, starting at the beginning of a
    /// period. Above half the sample rate a wave cannot be represented, so the step is held there.
    #[must_use]
    pub fn new(wave: Wave, hertz: f64, rate: f64) -> Self {
        let step = (hertz / rate).clamp(0.0, 0.5);
        Self {
            wave,
            phase: 0.0,
            step,
        }
    }

    /// The next sample, in −1..=1 (a corrected corner may overshoot by a little).
    pub fn next_sample(&mut self) -> f64 {
        let t = self.phase;
        let dt = self.step;
        let value = match self.wave {
            Wave::Sine => math::sin(std::f64::consts::TAU * t),
            Wave::Sawtooth => 2.0 * t - 1.0 - blep(t, dt),
            Wave::Square => {
                let naive = if t < 0.5 { 1.0 } else { -1.0 };
                naive + blep(t, dt) - blep((t + 0.5).fract(), dt)
            }
            Wave::Triangle => {
                // The slope is +4 a period from −1 to 1, then −4: it changes by 8 a period, 8·dt a
                // sample, up at phase 0 and down at phase ½.
                let naive = 1.0 - 4.0 * (t - 0.5).abs();
                naive + 8.0 * dt * (blamp(t, dt) - blamp((t + 0.5).fract(), dt))
            }
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{Oscillator, Wave};

    const RATE: f64 = 44_100.0;

    fn render(wave: Wave, hertz: f64, frames: usize) -> Vec<f64> {
        let mut oscillator = Oscillator::new(wave, hertz, RATE);
        (0..frames).map(|_| oscillator.next_sample()).collect()
    }

    /// The magnitude of the component at `hertz` in `signal`, by a single DFT bin: a sinusoid of
    /// amplitude A over a whole number of its periods measures A.
    fn component(signal: &[f64], hertz: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (n, x) in signal.iter().enumerate() {
            let angle = std::f64::consts::TAU * hertz * n as f64 / RATE;
            re += x * angle.cos();
            im -= x * angle.sin();
        }
        2.0 * (re * re + im * im).sqrt() / signal.len() as f64
    }

    #[test]
    fn every_wave_repeats_at_its_period() {
        // 441 Hz at 44.1 kHz is exactly 100 samples a period.
        for wave in [Wave::Sine, Wave::Sawtooth, Wave::Square, Wave::Triangle] {
            let samples = render(wave, 441.0, 300);
            for n in 0..200 {
                assert!(
                    (samples[n] - samples[n + 100]).abs() < 1e-9,
                    "{wave:?} at {n}"
                );
            }
        }
    }

    #[test]
    fn the_waves_have_their_shapes() {
        // 100 samples a period: the sine peaks a quarter in; the triangle starts at −1 and peaks
        // halfway; the square is high for the first half and low for the second; the saw climbs.
        let sine = render(Wave::Sine, 441.0, 100);
        assert!(sine[0].abs() < 1e-12);
        assert!((sine[25] - 1.0).abs() < 1e-12);
        assert!((sine[75] + 1.0).abs() < 1e-12);
        let triangle = render(Wave::Triangle, 441.0, 100);
        assert!((triangle[25] - 0.0).abs() < 1e-9);
        // The corner itself is rounded by 8·dt/3 ≈ 0.027 (the ramp residual at its centre is ⅓).
        assert!(
            (triangle[50] - (1.0 - 0.08 / 3.0)).abs() < 1e-9,
            "{}",
            triangle[50]
        );
        let square = render(Wave::Square, 441.0, 100);
        assert!((square[25] - 1.0).abs() < 1e-12);
        assert!((square[75] + 1.0).abs() < 1e-12);
        let saw = render(Wave::Sawtooth, 441.0, 100);
        // Away from the jump, 2t − 1: at t = 0.25, −0.5.
        assert!((saw[25] + 0.5).abs() < 1e-12);
        assert!(saw[10] < saw[20] && saw[20] < saw[90]);
    }

    #[test]
    fn the_fundamental_has_its_textbook_amplitude() {
        // Fourier series: a sine's is 1, a square's 4/π, a sawtooth's 2/π, a triangle's 8/π².
        // 441 Hz over 4 410 samples is ten whole periods.
        let pi = std::f64::consts::PI;
        for (wave, expected) in [
            (Wave::Sine, 1.0),
            (Wave::Square, 4.0 / pi),
            (Wave::Sawtooth, 2.0 / pi),
            (Wave::Triangle, 8.0 / (pi * pi)),
        ] {
            let measured = component(&render(wave, 441.0, 4_410), 441.0);
            assert!((measured - expected).abs() < 0.005, "{wave:?}: {measured}");
        }
    }

    #[test]
    fn the_band_limited_saw_folds_back_far_less_than_a_naive_one() {
        // A 3 kHz sawtooth over 4 410 samples (300 whole periods). Its harmonics above the
        // 22.05 kHz Nyquist fold back: the 8th (24 kHz) to 44.1 − 24 = 20.1 kHz, the 14th (42 kHz)
        // to 2.1 kHz, each with the naive wave's full 2/(kπ). PolyBLEP's correction is a short
        // smooth pulse, so it removes most of what folds far down and less of what folds to just
        // under Nyquist: at least 20 dB at 2.1 kHz, and
        // at least 6 dB at 20.1 kHz.
        let naive: Vec<f64> = (0..4_410_i32)
            .map(|n| 2.0 * ((f64::from(n) * 3_000.0 / RATE).fract()) - 1.0)
            .collect();
        let blep = render(Wave::Sawtooth, 3_000.0, 4_410);
        let pi = std::f64::consts::PI;
        for (hertz, harmonic, suppression) in [(2_100.0, 14.0, 10.0), (20_100.0, 8.0, 2.0)] {
            let folded_naive = component(&naive, hertz);
            assert!(
                (folded_naive - 2.0 / (harmonic * pi)).abs() < 0.01,
                "{folded_naive}"
            );
            let folded = component(&blep, hertz);
            assert!(
                folded < folded_naive / suppression,
                "{hertz} Hz: {folded} vs {folded_naive}"
            );
        }
    }
}
