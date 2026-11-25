//! A note's envelope (`docs/player.md` §4): a linear attack, a linear decay to its sustain, held for
//! the note's length, then a linear release.
//!
//! Every segment is a straight line because Strudel's are, measured (`docs/player.md` §11): a
//! sine with `.release(0.3)` loses the same amplitude every millisecond until it is silent at
//! 0.3 s, and one with `.attack(0.02)` gains the same amplitude every millisecond until it is full
//! at 20 ms. A release starts from wherever the level is and reaches silence in exactly its length,
//! and the voice ends there.

/// Where an envelope is.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Rising: `frame` frames in of `of`.
    Attack { frame: u32, of: u32 },
    /// Falling from full level to the sustain: `frame` frames in of `of`.
    Decay { frame: u32, of: u32 },
    /// At the sustain level until released.
    Hold,
    /// Falling to silence, `left` frames to go, by `step` a frame.
    Release { left: u32, step: f64 },
    /// Finished: silent for good.
    Done,
}

/// A running envelope.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Envelope {
    stage: Stage,
    level: f64,
    /// The decay's length in frames.
    decay: u32,
    /// The level it holds at.
    sustain: f64,
    /// The release's length in frames, fixed when the note starts.
    release: u32,
}

impl Envelope {
    /// An envelope rising over `attack` frames to full level, held there, and once released,
    /// falling over `release` frames.
    #[must_use]
    pub const fn new(attack: u32, release: u32) -> Self {
        Self::with_decay(attack, 0, 1.0, release)
    }

    /// An envelope rising over `attack` frames, falling over `decay` frames to `sustain` (0..=1),
    /// held there, and once released falling over `release` frames.
    #[must_use]
    pub const fn with_decay(attack: u32, decay: u32, sustain: f64, release: u32) -> Self {
        let sustain = sustain.clamp(0.0, 1.0);
        let (stage, level) = if attack > 0 {
            (
                Stage::Attack {
                    frame: 0,
                    of: attack,
                },
                0.0,
            )
        } else if decay > 0 {
            (
                Stage::Decay {
                    frame: 0,
                    of: decay,
                },
                1.0,
            )
        } else {
            (Stage::Hold, sustain)
        };
        Self {
            stage,
            level,
            decay,
            sustain,
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
        self.stage = Stage::Release {
            left: self.release,
            step: self.level / f64::from(self.release),
        };
    }

    /// After the attack: into the decay, or straight to the hold.
    const fn after_attack(&mut self) {
        if self.decay > 0 {
            self.stage = Stage::Decay {
                frame: 0,
                of: self.decay,
            };
            self.level = 1.0;
        } else {
            self.stage = Stage::Hold;
            self.level = self.sustain;
        }
    }

    /// The level for this frame, then a step on.
    pub fn next_level(&mut self) -> f64 {
        let level = self.level;
        match self.stage {
            Stage::Attack { frame, of } => {
                let next = frame + 1;
                if next >= of {
                    self.after_attack();
                } else {
                    self.stage = Stage::Attack { frame: next, of };
                    self.level = f64::from(next) / f64::from(of);
                }
            }
            Stage::Decay { frame, of } => {
                let next = frame + 1;
                if next >= of {
                    self.stage = Stage::Hold;
                    self.level = self.sustain;
                } else {
                    self.stage = Stage::Decay { frame: next, of };
                    self.level = 1.0 - (1.0 - self.sustain) * f64::from(next) / f64::from(of);
                }
            }
            Stage::Hold | Stage::Done => {}
            Stage::Release { left, step } => {
                if left <= 1 {
                    self.stage = Stage::Done;
                    self.level = 0.0;
                } else {
                    self.stage = Stage::Release {
                        left: left - 1,
                        step,
                    };
                    self.level = (self.level - step).max(0.0);
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
            Stage::Attack { .. } | Stage::Decay { .. } | Stage::Hold => None,
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
    fn the_release_is_a_straight_line_to_silence_in_its_length() {
        // 1 000 frames of release from full level: a thousandth lost each frame, so half after
        // 500, and the voice ends on the 1 000th.
        let mut envelope = Envelope::new(0, 1_000);
        assert!((envelope.next_level() - 1.0).abs() < 1e-12);
        envelope.release();
        let levels: Vec<f64> = (0..1_001).map(|_| envelope.next_level()).collect();
        assert!((levels[0] - 1.0).abs() < 1e-12);
        assert!((levels[500] - 0.5).abs() < 1e-9, "{}", levels[500]);
        assert!((levels[999] - 0.001).abs() < 1e-9);
        assert!(levels[1_000].abs() < 1e-12);
        assert!(envelope.done());
    }

    #[test]
    fn the_decay_falls_in_a_straight_line_to_the_sustain() {
        // Strudel's envelope with nothing set, measured: up at once, down to 0.6 over 50 ms. In
        // frames: no attack, 100 frames of decay to 0.6 — 0.8 halfway, 0.6 from the 100th on —
        // and a release from 0.6 takes its 10 frames, 0.06 a frame.
        let mut envelope = Envelope::with_decay(0, 100, 0.6, 10);
        let levels: Vec<f64> = (0..150).map(|_| envelope.next_level()).collect();
        assert!((levels[0] - 1.0).abs() < 1e-12);
        assert!((levels[50] - 0.8).abs() < 1e-12);
        assert!((levels[100] - 0.6).abs() < 1e-12);
        assert!((levels[149] - 0.6).abs() < 1e-12);
        envelope.release();
        assert!((envelope.next_level() - 0.6).abs() < 1e-12);
        assert!((envelope.next_level() - 0.54).abs() < 1e-12);
        for _ in 0..8 {
            envelope.next_level();
        }
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
