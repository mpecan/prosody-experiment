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
        self.read(at, None)
    }

    /// The sample at fractional frame `at` of a note looping `looped`, `wrapped` once it has gone
    /// round the loop at least once. The four frames the interpolation reads are taken from the
    /// sound as it plays rather than from the buffer: past the loop's end comes its start again,
    /// and once it has wrapped, before its start comes its end. Read from the buffer instead, the
    /// frames past the end are silence (or whatever follows the loop) and a looped note dips or
    /// overshoots once each time round.
    #[must_use]
    pub fn at_looped(&self, at: f64, looped: Loop, wrapped: bool) -> (f64, f64) {
        self.read(at, Some((looped, wrapped)))
    }

    fn read(&self, at: f64, looped: Option<(Loop, bool)>) -> (f64, f64) {
        let left = interpolate(&self.left, at, looped);
        let right = self
            .right
            .as_deref()
            .map_or(left, |right| interpolate(right, at, looped));
        (left, right)
    }
}

/// Frame `index` of `channel`, or silence outside it.
fn frame_of(channel: &[f32], index: i64) -> f64 {
    usize::try_from(index)
        .ok()
        .and_then(|i| channel.get(i))
        .map_or(0.0, |x| f64::from(*x))
}

/// `channel` at a fractional position, between its two nearest frames. A loop moved to the
/// decoded rate (`samples.ts` scales it) can have fractional ends, so the frame a loop maps a
/// neighbour to may fall between two; for whole-frame loop points it is exactly a frame.
fn linear(channel: &[f32], position: f64) -> f64 {
    let whole = position.floor();
    let t = position - whole;
    #[allow(clippy::cast_possible_truncation)]
    // a frame number of a sample far shorter than 2^63 frames
    let index = whole as i64;
    let here = frame_of(channel, index);
    here + t * (frame_of(channel, index + 1) - here)
}

/// `channel` at fractional frame `at`, from the frames either side of it, as the note plays them:
/// with `looped`, a neighbour past the loop's end is the frame as far past its start, and once the
/// note has wrapped, one before its start is as far before its end.
fn interpolate(channel: &[f32], at: f64, looped: Option<(Loop, bool)>) -> f64 {
    let whole = at.floor();
    let t = at - whole;
    #[allow(clippy::cast_possible_truncation)]
    // a frame number of a sample far shorter than 2^63 frames; a negative one reads as silence
    let index = whole as i64;
    let frame = |offset: i64| -> f64 {
        let i = index + offset;
        let Some((span, wrapped)) = looped else {
            return frame_of(channel, i);
        };
        #[allow(clippy::cast_precision_loss)] // a frame number, far below 2^52
        let position = i as f64;
        if position >= span.end() {
            linear(channel, position - span.length())
        } else if wrapped && position < span.start() {
            linear(channel, position + span.length())
        } else {
            frame_of(channel, i)
        }
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

    /// How long it is, in the sample's frames.
    #[must_use]
    pub fn length(self) -> f64 {
        self.end - self.start
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
    /// A soundfont instrument: zones over the keys, each with its position among its file's
    /// zones, in the order they were loaded.
    Font(Vec<(u32, Zone)>),
}

impl Instrument {
    /// The zone that plays `pitch` (rounded to a key), and its index here: of the zones whose range
    /// holds the key below it, the first in its file; or else the nearest by key, ties to the first
    /// in the file, so a note outside every range still sounds, from the closest recording.
    ///
    /// **The key below**, not the key itself, because that is what Strudel plays, measured
    /// (`docs/player.md` §11): the lowest key of every zone's range sounds the zone beneath it —
    /// `gm_epiano1:1` at key 64 (zone 64–69) was the recording of zone 58–63 to −46 dB, key 70 that
    /// of 64–69, `gm_acoustic_bass:1` at 37 that of 30–36 and at 29 that of 0–28 — and every other
    /// key the zone holding it. The `SoundFont` 2 specification reads a range as inclusive; following
    /// it would give one key in five or six another recording than the Strudel text plays, pitched
    /// the other way. Which recording sounds is timbre, not structure (`SPEC.md` §6, I3), so the
    /// player matches the text's sound.
    ///
    /// The choice goes by the file's order, not the order zones were loaded in. The page loads a
    /// font's zones as pieces need them, so which arrived first depends on what played before;
    /// choosing by load order would make the same poem sound a different recording depending on
    /// its edit history (`SPEC.md` §6, I1). By file order the choice among the zones loaded is the
    /// choice among all of them, as long as that one is loaded (`samples.ts` `zonesFor` loads it).
    #[must_use]
    pub fn zone(&self, pitch: f64) -> Option<(usize, &Zone)> {
        let Self::Font(zones) = self else {
            return None;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // clamped to 0..=127 first
        let key = (pitch.round().clamp(0.0, 127.0) as u8).saturating_sub(1);
        let holding = zones
            .iter()
            .enumerate()
            .filter(|(_, (_, zone))| (zone.low..=zone.high).contains(&key))
            .min_by_key(|(_, (position, _))| *position);
        holding
            .or_else(|| {
                zones
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, (position, zone))| {
                        let distance = if key < zone.low {
                            zone.low - key
                        } else {
                            key.saturating_sub(zone.high)
                        };
                        (distance, *position)
                    })
            })
            .map(|(index, (_, zone))| (index, zone))
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

    /// Adds zone `position` of its file to soundfont instrument `id`, making it a soundfont if it
    /// was not one, or replaces the zone it already has at that position. A zone already loaded
    /// keeps its index, so a voice playing it is not moved onto another.
    ///
    /// # Errors
    ///
    /// [`SynthError::Instrument`] for a number past [`MAX_INSTRUMENTS`].
    pub fn add_zone(&mut self, id: u32, position: u32, zone: Zone) -> Result<(), SynthError> {
        let slot = self.slot(id)?;
        match slot {
            Some(Instrument::Font(zones)) => {
                match zones.iter_mut().find(|(at, _)| *at == position) {
                    Some(existing) => existing.1 = zone,
                    None => zones.push((position, zone)),
                }
            }
            _ => *slot = Some(Instrument::Font(vec![(position, zone)])),
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
        let font = Instrument::Font(vec![
            (0, zone(30, 40)),
            (1, zone(41, 60)),
            (2, zone(70, 80)),
        ]);
        // Each key looks up the key below it.
        assert_eq!(font.zone(35.0).map(|(i, _)| i), Some(0));
        assert_eq!(font.zone(41.6).map(|(i, _)| i), Some(1)); // rounds to 42, looks up 41
        assert_eq!(font.zone(41.0).map(|(i, _)| i), Some(0)); // a range's lowest key: the zone below
        assert_eq!(font.zone(61.0).map(|(i, _)| i), Some(1)); // looks up 60, inside 41–60
        assert_eq!(font.zone(64.0).map(|(i, _)| i), Some(1)); // 63: 3 above 60, 7 below 70
        assert_eq!(font.zone(67.0).map(|(i, _)| i), Some(2)); // 66: 6 above 60, 4 below 70
        assert_eq!(font.zone(0.0).map(|(i, _)| i), Some(0)); // no key below 0: key 0, nearest 30
        assert_eq!(font.zone(10.0).map(|(i, _)| i), Some(0));
        assert!(Instrument::Drum(ramp(4)).zone(60.0).is_none());
    }

    #[test]
    fn the_zone_chosen_does_not_depend_on_the_order_zones_were_loaded_in() {
        // A file's zones: 0 is keys 40..=50, 1 is 56..=70, 2 is 45..=60 (overlapping both). Key 48
        // looks up 47, in 0 and 2, so 0, first in the file; key 53 looks up 52, in 2 only; key 75
        // looks up 74, nearest 1 (4 above 70). With only 0 and 1 loaded, 52 is 2 above 0 and 4 below
        // 1, so 0.
        let zone = |low: u8, high: u8| Zone::new(ramp(4), f64::from(low), low, high, None).unwrap();
        let file = [(0, zone(40, 50)), (1, zone(56, 70)), (2, zone(45, 60))];
        let chosen = |order: &[usize], key: f64| {
            let mut bank = Bank::new();
            for &i in order {
                let (position, zone) = file[i].clone();
                bank.add_zone(0, position, zone).unwrap();
            }
            let Some(Instrument::Font(zones)) = bank.get(0) else {
                unreachable!("a font was loaded")
            };
            let (index, _) = bank.get(0).unwrap().zone(key).unwrap();
            zones[index].0
        };
        for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0], [2, 0, 1]] {
            assert_eq!(chosen(&order, 48.0), 0, "{order:?}");
            assert_eq!(chosen(&order, 53.0), 2, "{order:?}");
            assert_eq!(chosen(&order, 75.0), 1, "{order:?}");
        }
        // The gap case the page reaches by loading zones as pieces need them: [1] then [0] (an edit
        // added key 53) chooses as a fresh load of [0, 1] does.
        assert_eq!(chosen(&[1, 0], 53.0), 0);
        assert_eq!(chosen(&[0, 1], 53.0), 0);
    }

    #[test]
    fn a_loop_is_read_as_it_plays_not_as_the_buffer_lies() {
        // A constant 0.5 for 100 frames, looping 50..100. Played at a fractional position near the
        // loop's end, the frames after it are the loop's start (0.5), not silence. At 99.5 read
        // from the buffer, the four frames are 0.5, 0.5, 0, 0 (frames 100 and 101 lie past it):
        // c1 = -0.25, c2 = -0.75, c3 = 0.5, so ((0.5/2 - 0.75)/2 - 0.25)/2 + 0.5 = 0.25, a dip of
        // half the level. Read as the note plays it, all four are 0.5, every coefficient but y0 is
        // zero, and it is 0.5 exactly.
        let constant = Sample::new(vec![0.5; 100], None, 48_000.0).unwrap();
        let looped = Loop::new(50.0, 100.0, 100).unwrap();
        assert!((constant.at(99.5).0 - 0.25).abs() < 1e-12);
        for at in [98.5, 99.25, 99.5, 99.9] {
            assert!(
                (constant.at_looped(at, looped, false).0 - 0.5).abs() < 1e-12,
                "{at}"
            );
        }

        // One period of a sine over the loop 50..100, and silence before it. Once wrapped, the
        // frame before the loop's start is its end (the sine's last frame), not the silence: at
        // 50.5 the value is the sine's, sin(2 pi 0.5/50), to Catmull-Rom's accuracy at 50 frames a
        // period (its error is far below 10^-3 there).
        let period = 50.0;
        let wave: Vec<f32> = (0..100)
            .map(|n| {
                if n < 50 {
                    0.0
                } else {
                    (std::f64::consts::TAU * f64::from(n - 50) / period).sin() as f32
                }
            })
            .collect();
        let sine = Sample::new(wave, None, 48_000.0).unwrap();
        let expected = |phase: f64| (std::f64::consts::TAU * phase / period).sin();
        assert!((sine.at_looped(50.5, looped, true).0 - expected(0.5)).abs() < 1e-3);
        // Before it has wrapped, the frames before the start are what played: the silence.
        assert!((sine.at_looped(50.5, looped, false).0 - expected(0.5)).abs() > 1e-3);
        // Near the end, the start follows.
        assert!((sine.at_looped(99.5, looped, true).0 - expected(49.5)).abs() < 1e-3);
        assert!((sine.at(99.5).0 - expected(49.5)).abs() > 1e-3);
    }

    #[test]
    fn a_bank_numbers_its_sounds() {
        let mut bank = Bank::new();
        bank.set_drum(3, ramp(4)).unwrap();
        assert!(matches!(bank.get(3), Some(Instrument::Drum(_))));
        assert!(bank.get(0).is_none());
        assert!(bank.get(99).is_none());
        bank.add_zone(5, 0, Zone::new(ramp(4), 60.0, 0, 127, None).unwrap())
            .unwrap();
        bank.add_zone(5, 1, Zone::new(ramp(4), 72.0, 0, 127, None).unwrap())
            .unwrap();
        assert!(matches!(bank.get(5), Some(Instrument::Font(zones)) if zones.len() == 2));
        // The same position again replaces it, where it was.
        bank.add_zone(5, 0, Zone::new(ramp(4), 48.0, 0, 127, None).unwrap())
            .unwrap();
        assert!(matches!(
            bank.get(5),
            Some(Instrument::Font(zones))
                if zones.len() == 2 && zones[0].0 == 0 && (zones[0].1.root() - 48.0).abs() < 1e-12
        ));
        assert!(bank.set_drum(super::MAX_INSTRUMENTS, ramp(4)).is_err());
        assert!(Zone::new(ramp(4), 60.0, 70, 60, None).is_err());
    }
}
