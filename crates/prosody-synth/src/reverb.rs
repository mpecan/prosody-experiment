//! A channel's reverb (`.room`, `.roomsize`, `docs/player.md` §4): a feedback delay network.
//!
//! Jot and Chaigne, "Digital delay networks for designing artificial reverberators" (AES 90th
//! convention, 1991): eight delay lines of mutually prime lengths, fed back through an orthogonal
//! matrix (here the 8×8 Hadamard matrix, scaled by 1/√8, applied as a fast Walsh–Hadamard
//! transform), each line attenuated so that a sound falls 60 dB in the RT60 asked for:
//! `g = 10^(−3·length / (RT60·rate))`. The left side feeds and is read from the lines with one row
//! of signs, the right with another, so the two sides of the tail are uncorrelated.
//!
//! **Matched by measurement** (`docs/player.md` §11). Strudel's `roomsize` measured as the RT60 in
//! seconds (a burst into `roomsize` 1, 2.5 and 5 decayed 60 dB in 1.05, 2.3–2.6 and 4.8–5.0 s), its
//! `room` as a linear send (half the send, 6 dB less tail) with the dry sound untouched, its tail
//! uncorrelated left to right and about flat in colour. Its level is set by [`WET`], fitted so a
//! short burst's tail starts where Strudel's does. Strudel's source was not read.

use crate::math;

/// How many delay lines.
const LINES: usize = 8;

/// Their lengths in frames at 48 kHz: mutually prime, from 29 to 59 ms, spread so that no two
/// lines' echoes pile up on the same frames (Jot and Chaigne's condition for a smooth tail).
const LENGTHS: [usize; LINES] = [1_399, 1_601, 1_867, 2_053, 2_251, 2_399, 2_617, 2_833];

/// Rows of the Hadamard matrix used as the sides' signs: the left side goes in with one and comes
/// out with another, the right with two more, all four orthogonal.
const LEFT_IN: [f64; LINES] = [1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0];
const RIGHT_IN: [f64; LINES] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
const LEFT_OUT: [f64; LINES] = [1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0, 1.0];
const RIGHT_OUT: [f64; LINES] = [1.0, 1.0, 1.0, 1.0, -1.0, -1.0, -1.0, -1.0];

/// The tail's level: what the lines' sum is scaled by on the way out (fitted, see above).
pub const WET: f64 = 0.079;

/// A running reverb.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reverb {
    lines: Vec<Vec<f32>>,
    at: [usize; LINES],
    gains: [f64; LINES],
    /// Its RT60, seconds.
    size: f64,
    rate: f64,
    /// Frames since anything was sent: past [`Self::tail`] it is silent and is not run.
    quiet: u64,
    tail: u64,
}

impl Reverb {
    /// A reverb whose tail falls 60 dB in `size` seconds, at `rate` frames a second.
    pub(crate) fn new(size: f64, rate: f64) -> Self {
        let scale = rate / 48_000.0;
        let lines = LENGTHS
            .iter()
            .map(|length| {
                #[allow(clippy::cast_precision_loss)] // lengths of a few thousand frames
                let scaled = (*length as f64 * scale).round().max(1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // positive, and a few thousand frames at most at the synth's rates
                let frames = scaled as usize;
                vec![0.0_f32; frames]
            })
            .collect();
        let mut reverb = Self {
            lines,
            at: [0; LINES],
            gains: [0.0; LINES],
            size: 0.0,
            rate,
            quiet: u64::MAX,
            tail: 0,
        };
        reverb.resize(size);
        reverb
    }

    /// Whether its RT60 is `size`.
    pub(crate) fn is(&self, size: f64) -> bool {
        self.size.to_bits() == size.to_bits()
    }

    /// A new RT60, keeping what is ringing.
    pub(crate) fn resize(&mut self, size: f64) {
        self.size = size;
        for (gain, line) in self.gains.iter_mut().zip(&self.lines) {
            #[allow(clippy::cast_precision_loss)] // a few thousand frames
            let length = line.len() as f64;
            *gain = math::from_decibels(-60.0 * length / (size * self.rate));
        }
        // Silent once a sound has fallen 120 dB: two RT60s, and the longest line once more.
        let longest = self.lines.iter().map(Vec::len).max().unwrap_or(0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // positive, and at most 2 × 10 s of frames
        let frames = (2.0 * size * self.rate).ceil() as u64;
        self.tail = frames + u64::try_from(longest).unwrap_or(0);
    }

    /// One frame: `x`, `y` sent in, the tail out.
    pub(crate) fn process(&mut self, x: f64, y: f64) -> (f64, f64) {
        if x != 0.0 || y != 0.0 {
            self.quiet = 0;
        } else if self.quiet >= self.tail {
            // Gone quiet: what is left is under −120 dB, and is cleared once so it cannot come
            // back when something is next sent.
            if self.quiet == self.tail {
                for line in &mut self.lines {
                    line.fill(0.0);
                }
                self.quiet += 1;
            }
            return (0.0, 0.0);
        } else {
            self.quiet += 1;
        }
        let mut out = [0.0; LINES];
        for ((value, line), at) in out.iter_mut().zip(&self.lines).zip(self.at) {
            *value = line.get(at).map_or(0.0, |s| f64::from(*s));
        }
        let (mut left, mut right) = (0.0, 0.0);
        for (value, (l, r)) in out.iter().zip(LEFT_OUT.iter().zip(RIGHT_OUT)) {
            left += l * value;
            right += r * value;
        }
        let mut mixed = out;
        hadamard(&mut mixed);
        for (k, ((line, at), value)) in self
            .lines
            .iter_mut()
            .zip(self.at.iter_mut())
            .zip(mixed)
            .enumerate()
        {
            let gain = self.gains.get(k).copied().unwrap_or(0.0);
            let feed = LEFT_IN.get(k).copied().unwrap_or(0.0) * x
                + RIGHT_IN.get(k).copied().unwrap_or(0.0) * y;
            if let Some(slot) = line.get_mut(*at) {
                #[allow(clippy::cast_possible_truncation)] // a sample, kept as f32 like the stems
                {
                    *slot = (gain * value + feed) as f32;
                }
            }
            *at += 1;
            if *at >= line.len() {
                *at = 0;
            }
        }
        (WET * left, WET * right)
    }
}

/// The 8-point Walsh–Hadamard transform, scaled by 1/√8 so it is orthogonal (energy kept).
fn hadamard(v: &mut [f64; LINES]) {
    let mut span = 1;
    while span < LINES {
        let mut start = 0;
        while start < LINES {
            for i in start..start + span {
                if let (Some(&a), Some(&b)) = (v.get(i), v.get(i + span)) {
                    if let Some(slot) = v.get_mut(i) {
                        *slot = a + b;
                    }
                    if let Some(slot) = v.get_mut(i + span) {
                        *slot = a - b;
                    }
                }
            }
            start += 2 * span;
        }
        span *= 2;
    }
    // 1/√8.
    for x in v.iter_mut() {
        *x *= 0.353_553_390_593_273_8;
    }
}

#[cfg(test)]
mod tests {
    use super::{Reverb, hadamard};

    #[test]
    fn the_matrix_keeps_energy() {
        // Orthogonal: a unit vector stays a unit vector; [1, 0, …] spreads to 1/√8 everywhere.
        let mut v = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        hadamard(&mut v);
        for x in v {
            assert!((x - 0.353_553_390_593_273_8).abs() < 1e-15);
        }
        let mut w = [0.3, -0.1, 0.7, 0.2, -0.5, 0.0, 0.1, 0.4];
        let before: f64 = w.iter().map(|x| x * x).sum();
        hadamard(&mut w);
        let after: f64 = w.iter().map(|x| x * x).sum();
        assert!((before - after).abs() < 1e-12);
    }

    /// The tail's energy in 100 ms windows, after an impulse on the left.
    fn energies(size: f64, windows: usize) -> Vec<f64> {
        let mut reverb = Reverb::new(size, 48_000.0);
        let mut out = vec![0.0; windows];
        for n in 0..windows * 4_800 {
            let (l, r) = reverb.process(if n == 0 { 1.0 } else { 0.0 }, 0.0);
            out[n / 4_800] += l * l + r * r;
        }
        out
    }

    #[test]
    fn the_tail_falls_sixty_decibels_in_its_size() {
        // Each line loses 60·length/(RT60·rate) dB a pass and the matrix keeps energy, so the
        // tail's energy falls 60 dB in RT60 seconds, 6 dB every tenth of an RT60. Measured between
        // the 0.2–0.3 s and 1.2–1.3 s windows of a 2 s reverb, a full second: 30 dB, ±1.5 dB for
        // the windows' fluctuation.
        let e = energies(2.0, 14);
        let fall = 10.0 * (e[2] / e[12]).log10();
        assert!((fall - 30.0).abs() < 1.5, "{fall}");
    }

    #[test]
    fn its_sides_are_uncorrelated() {
        // Fed on the left alone, the two sides' tails share little: |correlation| under 0.15, as
        // Strudel's four measured tails (from −0.04 to 0.14, `docs/player.md` §11).
        let mut reverb = Reverb::new(2.0, 48_000.0);
        let (mut lr, mut ll, mut rr) = (0.0, 0.0, 0.0);
        for n in 0..96_000 {
            let (l, r) = reverb.process(if n == 0 { 1.0 } else { 0.0 }, 0.0);
            if n > 4_800 {
                lr += l * r;
                ll += l * l;
                rr += r * r;
            }
        }
        let correlation: f64 = lr / (ll * rr).sqrt();
        assert!(correlation.abs() < 0.15, "{correlation}");
    }

    #[test]
    fn silence_after_the_tail_and_nothing_left_to_come_back() {
        // A 0.1 s reverb is quiet after 2 × 0.1 s and its longest line: 9 600 + 2 833 frames.
        let mut reverb = Reverb::new(0.1, 48_000.0);
        reverb.process(1.0, 1.0);
        for _ in 0..12_433 {
            reverb.process(0.0, 0.0);
        }
        assert_eq!(reverb.process(0.0, 0.0), (0.0, 0.0));
        assert!(
            reverb
                .lines
                .iter()
                .all(|line| line.iter().all(|x| *x == 0.0))
        );
    }
}
