//! A channel's echo (`.delay`, `docs/player.md` §4): a feedback delay line, the same both sides.
//!
//! Matched to Strudel's output (`docs/player.md` §11): a burst sent at 0.5 with `.delaytime(0.25)`
//! and `.delayfeedback(0.35)` came back 0.25 s later at half its level, then at 0.35 of that every
//! 0.25 s — 0.150, 0.0525, 0.0184, 0.0064 for a 0.3 burst — identical on both sides. So the line
//! returns only its repeats (the dry note is the voice's own), the first at the send's level.

/// A running echo.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct EchoLine {
    left: Vec<f32>,
    right: Vec<f32>,
    at: usize,
    feedback: f64,
    /// Frames since anything was sent: past [`Self::tail`] it is silent and is not run.
    quiet: u64,
    tail: u64,
}

impl EchoLine {
    /// An echo `frames` behind (at least one), each repeat `feedback` of the one before.
    pub(crate) fn new(frames: usize, feedback: f64) -> Self {
        let frames = frames.max(1);
        let mut line = Self {
            left: vec![0.0; frames],
            right: vec![0.0; frames],
            at: 0,
            feedback,
            quiet: u64::MAX,
            tail: 0,
        };
        line.tail = line.tail_frames();
        line
    }

    /// How long it rings after the last send: until a repeat is a millionth of the first (−120 dB),
    /// `ln(1e-6)/ln(feedback)` repeats, plus the first.
    fn tail_frames(&self) -> u64 {
        let length = u64::try_from(self.left.len()).unwrap_or(u64::MAX);
        let mut repeats = 1_u64;
        let mut level = 1.0;
        while level > 1e-6 && repeats < 1_000 {
            level *= self.feedback;
            repeats += 1;
        }
        length.saturating_mul(repeats)
    }

    /// Whether it is set to `frames` behind at `feedback`.
    pub(crate) fn is(&self, frames: usize, feedback: f64) -> bool {
        self.left.len() == frames.max(1) && self.feedback.to_bits() == feedback.to_bits()
    }

    /// Whether it has gone quiet: nothing sent for longer than its tail (or nothing ever sent), so
    /// with nothing more sent it will only ever return silence.
    pub(crate) const fn is_quiet(&self) -> bool {
        self.quiet >= self.tail
    }

    /// One frame: `x`, `y` in, the repeats out.
    pub(crate) fn process(&mut self, x: f64, y: f64) -> (f64, f64) {
        if x != 0.0 || y != 0.0 {
            self.quiet = 0;
        } else if self.quiet >= self.tail {
            // Gone quiet: what is left in the line is under a millionth, and is cleared once so it
            // cannot come back when something is next sent.
            if self.quiet == self.tail {
                self.left.fill(0.0);
                self.right.fill(0.0);
                self.quiet += 1;
            }
            return (0.0, 0.0);
        } else {
            self.quiet += 1;
        }
        let (Some(left), Some(right)) = (self.left.get_mut(self.at), self.right.get_mut(self.at))
        else {
            return (0.0, 0.0);
        };
        let out = (f64::from(*left), f64::from(*right));
        #[allow(clippy::cast_possible_truncation)] // a sample, kept as f32 like the stems
        {
            *left = (x + self.feedback * out.0) as f32;
            *right = (y + self.feedback * out.1) as f32;
        }
        self.at += 1;
        if self.at >= self.left.len() {
            self.at = 0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::EchoLine;

    #[test]
    fn repeats_come_back_every_delay_each_feedback_of_the_last() {
        // A unit impulse into a 10-frame line at 0.35: 1 at frame 10, 0.35 at 20, 0.1225 at 30,
        // nothing between, and nothing at frame 0 (the dry note is not the line's).
        let mut line = EchoLine::new(10, 0.35);
        let out: Vec<f64> = (0..35)
            .map(|n| line.process(if n == 0 { 1.0 } else { 0.0 }, 0.0).0)
            .collect();
        assert!(out[0].abs() < 1e-12);
        assert!((out[10] - 1.0).abs() < 1e-7);
        assert!((out[20] - 0.35).abs() < 1e-7);
        assert!((out[30] - 0.1225).abs() < 1e-7);
        assert!(out[15].abs() < 1e-12 && out[25].abs() < 1e-12);
    }

    #[test]
    fn it_goes_quiet_after_its_tail() {
        // At 0.35, 0.35^14 = 4.1e-7 is the first repeat under a millionth: 15 repeats of 10 frames.
        let line = EchoLine::new(10, 0.35);
        assert_eq!(line.tail, 150);
    }

    #[test]
    fn once_quiet_it_is_silent_and_nothing_left_comes_back() {
        // A unit impulse at frame 0, then silence. At the 150th silent frame the 15th repeat
        // (0.35^14 = 4.1e-7) comes out and goes back into the line as 0.35^15 = 1.4e-7; that frame
        // reaches the tail, so the line counts as quiet from then on. The next silent frame is
        // silence, and clears the line. A new impulse of 0.5 then returns 0.5 ten frames later and
        // 0.175 ten after that, and exactly nothing anywhere else — without the clearing, the old
        // 1.4e-7 would come round in its slot.
        let mut line = EchoLine::new(10, 0.35);
        line.process(1.0, 1.0);
        for _ in 0..149 {
            line.process(0.0, 0.0);
        }
        assert!(!line.is_quiet());
        line.process(0.0, 0.0);
        assert!(line.is_quiet());
        assert!(line.left.iter().any(|x| *x != 0.0));
        assert_eq!(line.process(0.0, 0.0), (0.0, 0.0));
        assert!(line.left.iter().chain(&line.right).all(|x| *x == 0.0));
        let out: Vec<(f64, f64)> = (0..30)
            .map(|n| line.process(if n == 0 { 0.5 } else { 0.0 }, 0.0))
            .collect();
        for (n, (l, r)) in out.iter().enumerate() {
            let expected = match n {
                10 => 0.5,
                20 => f64::from(0.175_f32),
                _ => 0.0,
            };
            assert_eq!((*l, *r), (expected, 0.0), "frame {n}");
        }
        assert!(!line.is_quiet());
    }
}
