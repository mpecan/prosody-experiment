//! A note's envelope (`docs/player.md` §4): a linear attack, held for the note's length, then an
//! exponential release.
//!
//! Linear up, because the text's attacks are short and a linear ramp is what "attack" means in
//! Strudel's documentation; exponential down, because that is how a struck or plucked sound dies
//! and how a listener hears an even fade. The release falls to −60 dB (one thousandth) in exactly
//! its length and the voice ends there, so a release of 0.6 s takes 0.6 s whatever level the note
//! was released from.

use crate::math;

/// Where an envelope is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Rising: `frame` frames in of `of`.
    Attack { frame: u32, of: u32 },
    /// At full level until released.
    Hold,
    /// Falling, `left` frames to go, by `factor` a frame.
    Release { left: u32, factor: f64 },
    /// Finished: silent for good.
    Done,
}

/// A running envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    stage: Stage,
    level: f64,
    /// The release's length in frames, fixed when the note starts.
    release: u32,
}

impl Envelope {
    /// An envelope rising over `attack` frames and, once released, falling over `release` frames.
    #[must_use]
    pub const fn new(attack: u32, release: u32) -> Self {
        let stage = if attack == 0 {
            Stage::Hold
        } else {
            Stage::Attack {
                frame: 0,
                of: attack,
            }
        };
        let level = if attack == 0 { 1.0 } else { 0.0 };
        Self {
            stage,
            level,
            release,
        }
    }

    /// Lets go: the release starts now, from wherever the level is.
    pub fn release(&mut self) {
        if matches!(self.stage, Stage::Done | Stage::Release { .. }) {
            return;
        }
        if self.release == 0 {
            self.stage = Stage::Done;
            self.level = 0.0;
            return;
        }
        let factor = math::exp2(math::LOG2_THOUSANDTH / f64::from(self.release));
        self.stage = Stage::Release {
            left: self.release,
            factor,
        };
    }

    /// The level for this frame, then a step on.
    pub fn next_level(&mut self) -> f64 {
        let level = self.level;
        match self.stage {
            Stage::Attack { frame, of } => {
                let next = frame + 1;
                if next >= of {
                    self.stage = Stage::Hold;
                    self.level = 1.0;
                } else {
                    self.stage = Stage::Attack { frame: next, of };
                    self.level = f64::from(next) / f64::from(of);
                }
            }
            Stage::Hold | Stage::Done => {}
            Stage::Release { left, factor } => {
                if left <= 1 {
                    self.stage = Stage::Done;
                    self.level = 0.0;
                } else {
                    self.stage = Stage::Release {
                        left: left - 1,
                        factor,
                    };
                    self.level *= factor;
                }
            }
        }
        level
    }

    /// Whether it has finished: silent from here on.
    #[must_use]
    pub const fn done(&self) -> bool {
        matches!(self.stage, Stage::Done)
    }

    /// Frames until it finishes, if it has been released (or has finished); `None` while it is
    /// still rising or held. What the voice pool steals by (`pool`): the voice nearest its end.
    #[must_use]
    pub const fn frames_left(&self) -> Option<u32> {
        match self.stage {
            Stage::Release { left, .. } => Some(left),
            Stage::Done => Some(0),
            Stage::Attack { .. } | Stage::Hold => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Envelope;

    #[test]
    fn the_attack_is_a_straight_line_up() {
        // 100 frames: 0 at the start, ½ halfway, 1 from the 100th frame on.
        let mut envelope = Envelope::new(100, 10);
        let levels: Vec<f64> = (0..120).map(|_| envelope.next_level()).collect();
        assert!(levels[0].abs() < 1e-12);
        assert!((levels[50] - 0.5).abs() < 1e-12);
        assert!((levels[99] - 0.99).abs() < 1e-12);
        assert!((levels[100] - 1.0).abs() < 1e-12);
        assert!((levels[119] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn the_release_reaches_a_thousandth_in_its_length_then_ends() {
        // 1 000 frames of release from full level: after 500, √0.001 ≈ 0.0316 (half the decibels,
        // −30 dB); the voice ends on the 1 000th.
        let mut envelope = Envelope::new(0, 1_000);
        assert!((envelope.next_level() - 1.0).abs() < 1e-12);
        envelope.release();
        let levels: Vec<f64> = (0..1_001).map(|_| envelope.next_level()).collect();
        assert!((levels[0] - 1.0).abs() < 1e-12);
        assert!(
            (levels[500] - 0.001_f64.sqrt()).abs() < 1e-9,
            "{}",
            levels[500]
        );
        assert!((levels[999] - 0.001 / (0.001_f64).powf(1.0 / 1000.0)).abs() < 1e-9);
        assert!(levels[1_000].abs() < 1e-12);
        assert!(envelope.done());
    }

    #[test]
    fn a_note_released_mid_attack_falls_from_where_it_got_to() {
        // Released at a quarter of the way up: the release starts from 0.25, and still lasts its
        // length.
        let mut envelope = Envelope::new(100, 100);
        for _ in 0..25 {
            envelope.next_level();
        }
        envelope.release();
        assert!((envelope.next_level() - 0.25).abs() < 1e-12);
        assert_eq!(envelope.frames_left(), Some(99));
        for _ in 0..99 {
            envelope.next_level();
        }
        assert!(envelope.done());
    }

    #[test]
    fn a_held_note_has_no_end_in_sight() {
        let mut envelope = Envelope::new(10, 10);
        assert_eq!(envelope.frames_left(), None);
        for _ in 0..50 {
            envelope.next_level();
        }
        assert_eq!(envelope.frames_left(), None);
        envelope.release();
        assert_eq!(envelope.frames_left(), Some(10));
    }
}
