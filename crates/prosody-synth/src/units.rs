//! The units a note is described in, each its own type (`CLAUDE.md` §5): a length in seconds is
//! not a cutoff in hertz is not a pitch, and a function taking three `f64`s cannot tell them apart.
//!
//! Each is checked once, where it is made: a value that is not a finite number in range is refused
//! there, so nothing downstream re-checks it (`CLAUDE.md` §5, "parse, don't validate").

use crate::SynthError;

/// A time in seconds: finite and not negative.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Seconds(f64);

impl Seconds {
    /// `value` seconds, or `None` if it is negative or not a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    /// The seconds.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// The nearest whole frame at `rate`.
    #[must_use]
    pub fn frames(self, rate: SampleRate) -> u64 {
        let frames = (self.0 * f64::from(rate.get())).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // not negative by construction; a time past 2^64 frames saturates, which no piece reaches
        let whole = frames as u64;
        whole
    }
}

/// A pitch as a MIDI note number, fractional for a slant rhyme's detuning (`docs/player.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Midi(f64);

impl Midi {
    /// The note `value`, or `None` outside 0..=127 or not a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && (0.0..=127.0).contains(&value)).then_some(Self(value))
    }

    /// The note number.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }

    /// Its frequency in equal temperament, A4 = MIDI 69 = 440 Hz.
    #[must_use]
    pub fn hertz(self) -> f64 {
        440.0 * crate::math::exp2((self.0 - 69.0) / 12.0)
    }
}

/// A frequency in hertz: finite and positive.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Hertz(f64);

impl Hertz {
    /// `value` Hz, or `None` if it is not a finite positive number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }

    /// The hertz.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// A linear amplitude factor: finite and not negative. 1 leaves a signal as it is.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Gain(f64);

impl Gain {
    /// A factor of `value`, or `None` if it is negative or not a finite number.
    #[must_use]
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    /// The factor.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

/// Frames a second, at least 8 000 (below that no cutoff the text sets is representable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SampleRate(u32);

impl SampleRate {
    /// The rate `value`, or an error naming it.
    ///
    /// # Errors
    ///
    /// [`SynthError::Rate`] below 8 000 or above 384 000 frames a second.
    pub const fn new(value: u32) -> Result<Self, SynthError> {
        if value < 8_000 || value > 384_000 {
            return Err(SynthError::Rate(value));
        }
        Ok(Self(value))
    }

    /// Frames a second.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Midi, SampleRate, Seconds};

    #[test]
    fn units_refuse_what_they_cannot_mean() {
        assert!(Seconds::new(-0.1).is_none());
        assert!(Seconds::new(f64::NAN).is_none());
        assert!(Midi::new(128.0).is_none());
        assert!(SampleRate::new(4_000).is_err());
    }

    #[test]
    fn a_time_is_its_nearest_frame() {
        let rate = SampleRate::new(48_000).unwrap();
        assert_eq!(Seconds::new(0.5).unwrap().frames(rate), 24_000);
        // 1/3 s at 48 kHz is 16 000 exactly; 1e-5 s is 0.48 of a frame, so frame 0.
        assert_eq!(Seconds::new(1.0 / 3.0).unwrap().frames(rate), 16_000);
        assert_eq!(Seconds::new(1e-5).unwrap().frames(rate), 0);
    }

    #[test]
    fn a4_is_440_and_an_octave_doubles() {
        assert!((Midi::new(69.0).unwrap().hertz() - 440.0).abs() < 1e-12);
        assert!((Midi::new(81.0).unwrap().hertz() - 880.0).abs() < 1e-12);
        // Middle C, 261.6256 Hz.
        assert!((Midi::new(60.0).unwrap().hertz() - 261.625_565_300_598_6).abs() < 1e-9);
    }
}
