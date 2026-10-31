//! What a note asks of the effects (`docs/player.md` §4): its colour — `vowel`, `crush`, `shape` —
//! and what it sends to its channel's buses — `room`, `delay` — and the channel's compressor.
//!
//! Each value is checked where it is made (`CLAUDE.md` §5, "parse, don't validate"); a note with
//! none of them is [`Effects::DRY`].

use crate::units::{Gain, Seconds};

/// A formant filter's vowel (`.vowel`): the five the text writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Vowel {
    /// /a/.
    A,
    /// /e/.
    E,
    /// /i/.
    I,
    /// /o/.
    O,
    /// /u/.
    U,
}

impl Vowel {
    /// Every vowel, in the order the page numbers them.
    pub const ALL: [Self; 5] = [Self::A, Self::E, Self::I, Self::O, Self::U];

    /// The vowel numbered `index` (0 a … 4 u).
    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::A),
            1 => Some(Self::E),
            2 => Some(Self::I),
            3 => Some(Self::O),
            4 => Some(Self::U),
            _ => None,
        }
    }
}

/// A bit depth (`.crush`): from 1 (two levels) to 16 (as good as untouched).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Crush(f64);

impl Crush {
    /// `bits` of depth, or `None` outside 1..=16 or not a finite number.
    #[must_use]
    pub fn new(bits: f64) -> Option<Self> {
        (bits.is_finite() && (1.0..=16.0).contains(&bits)).then_some(Self(bits))
    }

    /// The bits.
    #[must_use]
    pub const fn bits(self) -> f64 {
        self.0
    }
}

/// A waveshaper's amount (`.shape`): 0 (none) up to, not including, 1.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Shape(f64);

impl Shape {
    /// The most a shape may be: at 1 the curve's `k` is infinite, a hard square.
    pub const MAX: f64 = 0.99;

    /// An amount of `amount`, or `None` outside 0..=[`Self::MAX`] or not a finite number.
    #[must_use]
    pub fn new(amount: f64) -> Option<Self> {
        (amount.is_finite() && (0.0..=Self::MAX).contains(&amount)).then_some(Self(amount))
    }

    /// The amount.
    #[must_use]
    pub const fn amount(self) -> f64 {
        self.0
    }
}

/// A reverb send (`.room`, `.roomsize`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Room {
    send: Gain,
    size: Seconds,
}

impl Room {
    /// The longest reverb: Strudel's `roomsize` goes to 10.
    pub const MAX_SIZE: f64 = 10.0;

    /// `send` into a room whose tail falls 60 dB in `size` seconds (Strudel's `roomsize`, measured:
    /// `docs/player.md` §11), or `None` for a size that is not a positive time up to
    /// [`Self::MAX_SIZE`].
    #[must_use]
    pub fn new(send: Gain, size: Seconds) -> Option<Self> {
        (size.get() > 0.0 && size.get() <= Self::MAX_SIZE).then_some(Self { send, size })
    }

    /// How much is sent.
    #[must_use]
    pub const fn send(self) -> Gain {
        self.send
    }

    /// Its RT60, in seconds.
    #[must_use]
    pub const fn size(self) -> Seconds {
        self.size
    }
}

/// An echo send (`.delay`, `.delaytime`, `.delayfeedback`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Echo {
    send: Gain,
    time: Seconds,
    feedback: f64,
}

impl Echo {
    /// The longest echo the delay line holds: every echo the compiler can write. The melody's echo
    /// is a dotted eighth, `0.75 × 60 / tempo` seconds, and a style may set any tempo from 1 up,
    /// so 45 s at tempo 1. A shorter cap made a valid style (any tempo under 11.25) a piece that
    /// could not play at all. The line is allocated at the time asked for, so only such a style
    /// pays for a long one: 45 s at 48 kHz is 17 MB of `f32`.
    pub const MAX_TIME: f64 = 45.0;

    /// `send` into an echo `time` behind, each repeat `feedback` of the one before; `None` for a
    /// time that is not a positive time up to [`Self::MAX_TIME`], or a feedback outside 0..1 (at 1
    /// it would never die away).
    #[must_use]
    pub fn new(send: Gain, time: Seconds, feedback: f64) -> Option<Self> {
        (time.get() > 0.0
            && time.get() <= Self::MAX_TIME
            && feedback.is_finite()
            && (0.0..1.0).contains(&feedback))
        .then_some(Self {
            send,
            time,
            feedback,
        })
    }

    /// How much is sent.
    #[must_use]
    pub const fn send(self) -> Gain {
        self.send
    }

    /// How far behind it repeats.
    #[must_use]
    pub const fn time(self) -> Seconds {
        self.time
    }

    /// How much of each repeat returns.
    #[must_use]
    pub const fn feedback(self) -> f64 {
        self.feedback
    }
}

/// A compressor's setting (`.compressor`, `.compressorRatio`): the rest of it is fixed
/// ([`crate::dynamics`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compression {
    threshold: f64,
    ratio: f64,
}

impl Compression {
    /// A threshold of `threshold` dB (−100..=0) at `ratio`:1 (1..=20) — Web Audio's ranges, which
    /// Strudel's compressor is — or `None` outside them.
    #[must_use]
    pub fn new(threshold: f64, ratio: f64) -> Option<Self> {
        (threshold.is_finite()
            && (-100.0..=0.0).contains(&threshold)
            && ratio.is_finite()
            && (1.0..=20.0).contains(&ratio))
        .then_some(Self { threshold, ratio })
    }

    /// The threshold, in dB.
    #[must_use]
    pub const fn threshold(self) -> f64 {
        self.threshold
    }

    /// The ratio, `n` for `n`:1.
    #[must_use]
    pub const fn ratio(self) -> f64 {
        self.ratio
    }
}

/// Everything a note asks of the effects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Effects {
    /// Its bit depth.
    pub crush: Option<Crush>,
    /// Its formant vowel.
    pub vowel: Option<Vowel>,
    /// Its waveshaper.
    pub shape: Option<Shape>,
    /// Its reverb send.
    pub room: Option<Room>,
    /// Its echo send.
    pub echo: Option<Echo>,
    /// Its channel's compressor.
    pub compression: Option<Compression>,
}

impl Effects {
    /// None of them.
    pub const DRY: Self = Self {
        crush: None,
        vowel: None,
        shape: None,
        room: None,
        echo: None,
        compression: None,
    };
}

#[cfg(test)]
mod tests {
    use super::{Compression, Crush, Echo, Room, Shape, Vowel};
    use crate::units::{Gain, Seconds};

    #[test]
    fn effects_refuse_what_they_cannot_mean() {
        assert!(Crush::new(0.5).is_none() && Crush::new(17.0).is_none());
        assert!(Crush::new(4.0).is_some());
        assert!(Shape::new(1.0).is_none() && Shape::new(-0.1).is_none());
        let send = Gain::new(0.5).unwrap();
        assert!(Room::new(send, Seconds::new(0.0).unwrap()).is_none());
        assert!(Room::new(send, Seconds::new(2.5).unwrap()).is_some());
        assert!(Echo::new(send, Seconds::new(0.25).unwrap(), 1.0).is_none());
        assert!(Echo::new(send, Seconds::new(0.0).unwrap(), 0.35).is_none());
        assert!(Echo::new(send, Seconds::new(45.001).unwrap(), 0.35).is_none());
        // Every echo a valid style makes: the slowest tempo, 1, puts the dotted eighth at
        // 0.75 × 60 / 1 = 45 s; tempo 10 at 4.5 s, which a 4 s line refused, so the piece would
        // not play at all.
        assert!(Echo::new(send, Seconds::new(45.0).unwrap(), 0.35).is_some());
        assert!(Echo::new(send, Seconds::new(4.5).unwrap(), 0.35).is_some());
        assert!(Compression::new(6.0, 4.0).is_none() && Compression::new(-24.0, 0.5).is_none());
        assert_eq!(Vowel::from_index(2), Some(Vowel::I));
        assert_eq!(Vowel::from_index(5), None);
    }
}
