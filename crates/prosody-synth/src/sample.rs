//! Recorded sounds: drum samples, and soundfont instruments made of zones (`docs/player.md` §4).
//!
//! The synth does no decoding. A sample arrives as PCM — decoded by the browser's
//! `decodeAudioData` in the page — with the rate it was decoded at. A soundfont instrument is a set
//! of zones as the `SoundFont` 2 specification (§7.10, §8.1.2) describes them: each covers a range of
//! keys, plays its sample at its root key's pitch, and may loop a stretch of the sample for as long
//! as the note lasts.

use crate::SynthError;

/// The most instruments a bank holds. Instruments are numbered by whoever loads them (the page
/// numbers the sounds a Performance uses from 0); the cap keeps a bad number from allocating
/// without bound.
pub const MAX_INSTRUMENTS: u32 = 4_096;

/// Decoded PCM: one or two channels at a rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    left: Vec<f32>,
    /// The right channel, or `None` for a mono sample, which plays the same in both.
    right: Option<Vec<f32>>,
    rate: f64,
}

impl Sample {
    /// A sample of `left` (and `right`, for stereo) at `rate` frames a second.
    ///
    /// # Errors
    ///
    /// [`SynthError::Sample`] if the channels differ in length, it is empty, or the rate is not a
    /// positive finite number.
    pub fn new(left: Vec<f32>, right: Option<Vec<f32>>, rate: f64) -> Result<Self, SynthError> {
        if left.is_empty() {
            return Err(SynthError::Sample("it has no frames"));
        }
        if right
            .as_ref()
            .is_some_and(|right| right.len() != left.len())
        {
            return Err(SynthError::Sample("its channels differ in length"));
        }
        if !(rate.is_finite() && rate > 0.0) {
            return Err(SynthError::Sample("its rate is not a positive number"));
        }
        Ok(Self { left, right, rate })
    }

    /// How many frames it has.
    #[must_use]
    pub fn frames(&self) -> usize {
        self.left.len()
    }

    /// The rate it was decoded at.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// The sample at fractional frame `at`, both channels, by four-point cubic (Catmull–Rom)
    /// interpolation; silence beyond either end.
    #[must_use]
    pub fn at(&self, at: f64) -> (f64, f64) {
        let left = interpolate(&self.left, at);
        let right = self
            .right
            .as_deref()
            .map_or(left, |right| interpolate(right, at));
        (left, right)
    }
}

/// `channel` at fractional frame `at`, from the frames either side of it.
fn interpolate(channel: &[f32], at: f64) -> f64 {
    let whole = at.floor();
    let t = at - whole;
    #[allow(clippy::cast_possible_truncation)]
    // a frame number of a sample far shorter than 2^63 frames; a negative one reads as silence
    let index = whole as i64;
    let frame = |offset: i64| -> f64 {
        usize::try_from(index + offset)
            .ok()
            .and_then(|i| channel.get(i))
            .map_or(0.0, |x| f64::from(*x))
    };
    let (ym1, y0, y1, y2) = (frame(-1), frame(0), frame(1), frame(2));
    let c1 = 0.5 * (y1 - ym1);
    let c2 = ym1 - 2.5 * y0 + 2.0 * y1 - 0.5 * y2;
    let c3 = 0.5 * (y2 - ym1) + 1.5 * (y0 - y1);
    ((c3 * t + c2) * t + c1) * t + y0
}

/// A stretch of a sample that repeats while the note lasts, in the sample's frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loop {
    start: f64,
    end: f64,
}

impl Loop {
    /// The loop from frame `start` to frame `end` of a sample `frames` long, or `None` if it is not
    /// one: shorter than two frames, or not inside the sample. A soundfont's loop points were set
    /// on the original recording; the page's decoded copy may be a few frames longer (an MP3
    /// encoder's padding), which is why a loop is checked rather than trusted.
    #[must_use]
    pub fn new(start: f64, end: f64, frames: usize) -> Option<Self> {
        #[allow(clippy::cast_precision_loss)] // a sample's length, far below 2^52 frames
        let length = frames as f64;
        (start.is_finite()
            && end.is_finite()
            && start >= 0.0
            && end - start >= 2.0
            && end <= length)
            .then_some(Self { start, end })
    }

    /// Where it starts, in the sample's frames.
    #[must_use]
    pub const fn start(self) -> f64 {
        self.start
    }

    /// Where it ends and goes back to its start.
    #[must_use]
    pub const fn end(self) -> f64 {
        self.end
    }
}

/// One zone of a soundfont instrument.
#[derive(Debug, Clone, PartialEq)]
pub struct Zone {
    sample: Sample,
    /// The pitch the sample sounds at when played at its own rate, as a MIDI note: the zone's root
    /// key with its coarse and fine tuning folded in.
    root: f64,
    low: u8,
    high: u8,
    looped: Option<Loop>,
}

impl Zone {
    /// A zone playing `sample` for keys `low..=high`, sounding `root` (a MIDI note, fractional for a
    /// fine tuning) at its own rate, looping `looped` while held.
    ///
    /// # Errors
    ///
    /// [`SynthError::Zone`] if the key range is reversed or past 127, or the root is not a note.
    pub fn new(
        sample: Sample,
        root: f64,
        low: u8,
        high: u8,
        looped: Option<Loop>,
    ) -> Result<Self, SynthError> {
        if low > high || high > 127 || !(root.is_finite() && (-64.0..=191.0).contains(&root)) {
            return Err(SynthError::Zone { low, high });
        }
        Ok(Self {
            sample,
            root,
            low,
            high,
            looped,
        })
    }

    /// Its sample.
    #[must_use]
    pub const fn sample(&self) -> &Sample {
        &self.sample
    }

    /// The pitch its sample sounds at its own rate.
    #[must_use]
    pub const fn root(&self) -> f64 {
        self.root
    }

    /// Its loop, if it has one.
    #[must_use]
    pub const fn looped(&self) -> Option<Loop> {
        self.looped
    }
}

/// A sound that can be loaded.
#[derive(Debug, Clone, PartialEq)]
pub enum Instrument {
    /// A drum: one sample, played once at its own pitch.
    Drum(Sample),
    /// A soundfont instrument: zones over the keys.
    Font(Vec<Zone>),
}

impl Instrument {
    /// The zone that plays `pitch` (rounded to a key): the first whose range holds it, or else the
    /// nearest by key, so a note outside every range still sounds, from the closest recording.
    #[must_use]
    pub fn zone(&self, pitch: f64) -> Option<(usize, &Zone)> {
        let Self::Font(zones) = self else {
            return None;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // clamped to 0..=127 first
        let key = pitch.round().clamp(0.0, 127.0) as u8;
        zones
            .iter()
            .enumerate()
            .find(|(_, zone)| (zone.low..=zone.high).contains(&key))
            .or_else(|| {
                zones.iter().enumerate().min_by_key(|(index, zone)| {
                    let distance = if key < zone.low {
                        zone.low - key
                    } else {
                        key.saturating_sub(zone.high)
                    };
                    (distance, *index)
                })
            })
    }
}

/// The sounds loaded, by number.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bank {
    instruments: Vec<Option<Instrument>>,
}

impl Bank {
    /// An empty bank.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instruments: Vec::new(),
        }
    }

    /// Loads a drum as instrument `id`, in place of whatever had that number.
    ///
    /// # Errors
    ///
    /// [`SynthError::Instrument`] for a number past [`MAX_INSTRUMENTS`].
    pub fn set_drum(&mut self, id: u32, sample: Sample) -> Result<(), SynthError> {
        *self.slot(id)? = Some(Instrument::Drum(sample));
        Ok(())
    }

    /// Adds a zone to soundfont instrument `id`, making it a soundfont if it was not one.
    ///
    /// # Errors
    ///
    /// [`SynthError::Instrument`] for a number past [`MAX_INSTRUMENTS`].
    pub fn add_zone(&mut self, id: u32, zone: Zone) -> Result<(), SynthError> {
        let slot = self.slot(id)?;
        match slot {
            Some(Instrument::Font(zones)) => zones.push(zone),
            _ => *slot = Some(Instrument::Font(vec![zone])),
        }
        Ok(())
    }

    /// Instrument `id`, if it is loaded.
    #[must_use]
    pub fn get(&self, id: u32) -> Option<&Instrument> {
        usize::try_from(id)
            .ok()
            .and_then(|index| self.instruments.get(index))
            .and_then(Option::as_ref)
    }

    fn slot(&mut self, id: u32) -> Result<&mut Option<Instrument>, SynthError> {
        if id >= MAX_INSTRUMENTS {
            return Err(SynthError::Instrument(id));
        }
        let index = usize::try_from(id).map_err(|_| SynthError::Instrument(id))?;
        if self.instruments.len() <= index {
            self.instruments.resize(index + 1, None);
        }
        self.instruments
            .get_mut(index)
            .ok_or(SynthError::Instrument(id))
    }
}

#[cfg(test)]
mod tests {
    use super::{Bank, Instrument, Loop, Sample, Zone};

    fn ramp(frames: usize) -> Sample {
        Sample::new((0..frames).map(|n| n as f32).collect(), None, 48_000.0).unwrap()
    }

    #[test]
    fn cubic_interpolation_is_exact_on_a_straight_line() {
        // Catmull–Rom reproduces any straight line: halfway between frames 3 and 4 of a ramp is 3.5.
        let sample = ramp(10);
        assert!((sample.at(3.5).0 - 3.5).abs() < 1e-12);
        assert!((sample.at(6.25).1 - 6.25).abs() < 1e-12);
        // On a frame, the frame.
        assert!((sample.at(5.0).0 - 5.0).abs() < 1e-12);
    }

    #[test]
    fn silence_beyond_either_end() {
        let sample = ramp(4);
        assert!(sample.at(-3.0).0.abs() < 1e-12);
        assert!(sample.at(10.0).0.abs() < 1e-12);
    }

    #[test]
    fn a_sample_must_be_one() {
        assert!(Sample::new(vec![], None, 48_000.0).is_err());
        assert!(Sample::new(vec![0.0; 3], Some(vec![0.0; 2]), 48_000.0).is_err());
        assert!(Sample::new(vec![0.0; 3], None, 0.0).is_err());
    }

    #[test]
    fn a_loop_must_lie_inside_its_sample() {
        assert!(Loop::new(10.0, 90.0, 100).is_some());
        assert!(Loop::new(10.0, 101.0, 100).is_none());
        assert!(Loop::new(10.0, 11.0, 100).is_none());
        assert!(Loop::new(-1.0, 50.0, 100).is_none());
    }

    #[test]
    fn the_zone_for_a_key_is_the_one_whose_range_holds_it_else_the_nearest() {
        let zone = |low, high| Zone::new(ramp(4), f64::from(low), low, high, None).unwrap();
        let font = Instrument::Font(vec![zone(30, 40), zone(41, 60), zone(70, 80)]);
        assert_eq!(font.zone(35.0).map(|(i, _)| i), Some(0));
        assert_eq!(font.zone(40.6).map(|(i, _)| i), Some(1)); // rounds to key 41
        assert_eq!(font.zone(64.0).map(|(i, _)| i), Some(1)); // 4 above 60, 6 below 70
        assert_eq!(font.zone(66.0).map(|(i, _)| i), Some(2)); // 6 above 60, 4 below 70
        assert_eq!(font.zone(10.0).map(|(i, _)| i), Some(0));
        assert!(Instrument::Drum(ramp(4)).zone(60.0).is_none());
    }

    #[test]
    fn a_bank_numbers_its_sounds() {
        let mut bank = Bank::new();
        bank.set_drum(3, ramp(4)).unwrap();
        assert!(matches!(bank.get(3), Some(Instrument::Drum(_))));
        assert!(bank.get(0).is_none());
        assert!(bank.get(99).is_none());
        bank.add_zone(5, Zone::new(ramp(4), 60.0, 0, 127, None).unwrap())
            .unwrap();
        bank.add_zone(5, Zone::new(ramp(4), 72.0, 0, 127, None).unwrap())
            .unwrap();
        assert!(matches!(bank.get(5), Some(Instrument::Font(zones)) if zones.len() == 2));
        assert!(bank.set_drum(super::MAX_INSTRUMENTS, ramp(4)).is_err());
        assert!(Zone::new(ramp(4), 60.0, 70, 60, None).is_err());
    }
}
