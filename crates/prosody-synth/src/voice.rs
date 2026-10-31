//! A sounding note: its source, envelope, filter and level (`docs/player.md` §4's per-note chain),
//! and the fixed pool voices are drawn from.

use crate::env::Envelope;
use crate::filter::{BUTTERWORTH_Q, Biquad, Coefficients};
use crate::note::{Channel, Note, Source};
use crate::osc::Oscillator;
use crate::sample::{Bank, Instrument, Loop, Sample};
use crate::units::SampleRate;

/// How many notes can sound at once. The densest corpus piece stacks four lines of drums, bass,
/// melody and four-note chords with releases of up to 0.6 s; 64 leaves room, and a fixed number
/// means the render never allocates.
pub const VOICES: usize = 64;

/// Where a voice's sound comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Generator {
    Oscillator(Oscillator),
    Sampler {
        instrument: u32,
        /// Which zone of a soundfont; `None` for a drum.
        zone: Option<usize>,
        /// Where in the sample, in its frames.
        at: f64,
        /// How far a frame moves through the sample: pitch ratio × the sample's rate ÷ the output's.
        step: f64,
        looped: Option<Loop>,
        /// Whether it has gone round its loop yet: from then on, what comes before the loop's
        /// start is its end, for the interpolation (`Sample::at_looped`).
        wrapped: bool,
    },
}

/// A sounding note.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Voice {
    tag: u32,
    channel: Channel,
    generator: Generator,
    envelope: Envelope,
    /// Frames until the note is let go; `None` once it has been, or for a drum, which plays its
    /// sample through.
    gate: Option<u64>,
    coefficients: Coefficients,
    filters: [Biquad; 2],
    level: f64,
    /// The output frame it began on: the oldest is stolen when nothing is releasing.
    began: u64,
}

/// Why a note could not become a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unplayable {
    /// Its instrument is not in the bank (not loaded, or a drum where a soundfont was asked for).
    Missing,
}

/// Frames for `seconds` at `rate`, as a count an envelope can hold.
fn frames(seconds: crate::units::Seconds, rate: SampleRate) -> u32 {
    u32::try_from(seconds.frames(rate)).unwrap_or(u32::MAX)
}

impl Voice {
    /// The voice for `note`, beginning on output frame `began`.
    pub(crate) fn new(
        note: &Note,
        bank: &Bank,
        rate: SampleRate,
        began: u64,
    ) -> Result<Self, Unplayable> {
        let spec = note.spec();
        let hz = f64::from(rate.get());
        let pitch = spec.pitch.map_or(60.0, crate::units::Midi::get);
        let held = Some(spec.length.frames(rate));
        let (generator, gate) = match spec.source {
            Source::Wave(wave) => {
                let hertz = spec.pitch.map_or(261.63, crate::units::Midi::hertz);
                (
                    Generator::Oscillator(Oscillator::new(wave, hertz, hz)),
                    held,
                )
            }
            Source::Drum(instrument) => match bank.get(instrument) {
                // A drum plays its sample through, at its own pitch (`docs/player.md` §4).
                Some(Instrument::Drum(sample)) => (
                    Generator::Sampler {
                        instrument,
                        zone: None,
                        at: 0.0,
                        step: sample.rate() / hz,
                        looped: None,
                        wrapped: false,
                    },
                    None,
                ),
                _ => return Err(Unplayable::Missing),
            },
            Source::Font(instrument) => {
                let Some((index, zone)) = bank.get(instrument).and_then(|font| font.zone(pitch))
                else {
                    return Err(Unplayable::Missing);
                };
                // SoundFont 2 §8.1.2: a zone's sample sounds its root key at its own rate, so a key
                // n semitones up plays it 2^(n/12) times as fast.
                let ratio = crate::math::exp2((pitch - zone.root()) / 12.0);
                (
                    Generator::Sampler {
                        instrument,
                        zone: Some(index),
                        at: 0.0,
                        step: ratio * zone.sample().rate() / hz,
                        looped: zone.looped(),
                        wrapped: false,
                    },
                    held,
                )
            }
        };
        Ok(Self {
            tag: spec.tag,
            channel: spec.channel,
            generator,
            envelope: Envelope::new(frames(note.attack(), rate), frames(note.release(), rate)),
            gate,
            coefficients: Coefficients::low_pass(spec.lpf.get(), BUTTERWORTH_Q, hz),
            filters: [Biquad::default(); 2],
            level: spec.gain.get() * spec.postgain.get(),
            began,
        })
    }

    /// The note it plays: its index in the Performance.
    pub(crate) const fn tag(&self) -> u32 {
        self.tag
    }

    /// Its stem.
    pub(crate) const fn channel(&self) -> Channel {
        self.channel
    }

    /// Frames until it ends of its own accord, if that is known: a released note's release, or the
    /// rest of a drum's sample. `None` while a note is held. The pool steals the voice nearest its
    /// end — a count of frames, worked out from the same numbers on every platform.
    fn frames_left(&self, bank: &Bank) -> Option<u64> {
        if let Some(left) = self.envelope.frames_left() {
            return Some(u64::from(left));
        }
        match (self.gate, self.generator) {
            (
                None,
                Generator::Sampler {
                    instrument,
                    zone: None,
                    at,
                    step,
                    looped: None,
                    ..
                },
            ) => {
                let frames = match bank.get(instrument) {
                    Some(Instrument::Drum(sample)) => sample.frames(),
                    _ => 0,
                };
                #[allow(clippy::cast_precision_loss)] // a sample's length, far below 2^52
                let rest = ((frames as f64 - at) / step).ceil().max(0.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // not negative, and a sample's frames at most
                let rest = rest as u64;
                Some(rest)
            }
            _ => None,
        }
    }

    /// Adds the voice's next `left.len()` frames into its stem's two sides. Returns whether it is
    /// still sounding.
    pub(crate) fn render(&mut self, bank: &Bank, left: &mut [f32], right: &mut [f32]) -> bool {
        let sample: Option<&Sample> = match self.generator {
            Generator::Sampler {
                instrument, zone, ..
            } => match (bank.get(instrument), zone) {
                (Some(Instrument::Drum(sample)), None) => Some(sample),
                (Some(Instrument::Font(zones)), Some(index)) => {
                    zones.get(index).map(|(_, zone)| zone.sample())
                }
                // Its instrument was replaced while it sounded: it stops rather than plays another.
                _ => return false,
            },
            Generator::Oscillator(_) => None,
        };
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            match self.gate {
                Some(0) => {
                    self.envelope.release();
                    self.gate = None;
                }
                Some(frames) => self.gate = Some(frames - 1),
                None => {}
            }
            if self.envelope.done() {
                return false;
            }
            let level = self.envelope.next_level() * self.level;
            let (x, y) = match (&mut self.generator, sample) {
                (Generator::Oscillator(oscillator), _) => {
                    let x = oscillator.next_sample();
                    (x, x)
                }
                (
                    Generator::Sampler {
                        at,
                        step,
                        looped,
                        wrapped,
                        ..
                    },
                    Some(sample),
                ) => {
                    #[allow(clippy::cast_precision_loss)] // a sample's length, far below 2^52
                    let end = sample.frames() as f64;
                    let value = match looped {
                        Some(span) => sample.at_looped(*at, *span, *wrapped),
                        None if *at >= end => return false,
                        None => sample.at(*at),
                    };
                    *at += *step;
                    if let Some(span) = looped {
                        while *at >= span.end() {
                            *at -= span.length();
                            *wrapped = true;
                        }
                    }
                    value
                }
                (Generator::Sampler { .. }, None) => return false,
            };
            let [filter_left, filter_right] = &mut self.filters;
            let x = filter_left.process(&self.coefficients, x) * level;
            let y = filter_right.process(&self.coefficients, y) * level;
            #[allow(clippy::cast_possible_truncation)] // a sample to f32 PCM
            {
                *l += x as f32;
                *r += y as f32;
            }
        }
        !self.envelope.done()
    }
}

/// The fixed set of voices.
#[derive(Debug, Clone)]
pub(crate) struct Pool {
    voices: Vec<Option<Voice>>,
}

impl Pool {
    /// [`VOICES`] silent voices.
    pub(crate) fn new() -> Self {
        Self {
            voices: vec![None; VOICES],
        }
    }

    /// Starts `voice`, in a free slot or else in place of another. Returns the voice it took the
    /// place of, if it had to steal one: the one nearest its end among those releasing (or drums
    /// playing out), and if none is, the oldest. Ties go to the lower slot, so the choice is the
    /// same every time.
    pub(crate) fn start(&mut self, voice: Voice, bank: &Bank) -> Option<Voice> {
        if let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(voice);
            return None;
        }
        let ending = self
            .voices
            .iter()
            .enumerate()
            .filter_map(|(slot, v)| {
                v.as_ref()
                    .and_then(|v| v.frames_left(bank))
                    .map(|left| (left, slot))
            })
            .min();
        let slot = ending.map(|(_, slot)| slot).or_else(|| {
            self.voices
                .iter()
                .enumerate()
                .filter_map(|(slot, v)| v.as_ref().map(|v| (v.began, slot)))
                .min()
                .map(|(_, slot)| slot)
        })?;
        self.voices.get_mut(slot)?.replace(voice)
    }

    /// Every voice, to render.
    pub(crate) fn voices_mut(&mut self) -> impl Iterator<Item = &mut Option<Voice>> {
        self.voices.iter_mut()
    }

    /// The notes the sounding voices play, in slot order.
    pub(crate) fn tags(&self) -> impl Iterator<Item = u32> + '_ {
        self.voices
            .iter()
            .filter_map(|v| v.as_ref().map(Voice::tag))
    }

    /// How many are sounding.
    pub(crate) fn sounding(&self) -> usize {
        self.voices.iter().filter(|v| v.is_some()).count()
    }

    /// Silences them all.
    pub(crate) fn clear(&mut self) {
        for voice in &mut self.voices {
            *voice = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Pool, VOICES, Voice};
    use crate::note::{Channel, Note, NoteSpec, Source};
    use crate::osc::Wave;
    use crate::sample::{Bank, Loop, Sample, Zone};
    use crate::units::{Gain, Hertz, Midi, SampleRate, Seconds};

    fn rate() -> SampleRate {
        SampleRate::new(48_000).unwrap()
    }

    fn note(source: Source, pitch: Option<f64>, length: f64) -> Note {
        Note::new(NoteSpec {
            tag: 1,
            channel: Channel::Bass,
            source,
            onset: Seconds::new(0.0).unwrap(),
            length: Seconds::new(length).unwrap(),
            pitch: pitch.and_then(Midi::new),
            gain: Gain::new(0.5).unwrap(),
            postgain: Gain::new(1.0).unwrap(),
            attack: Seconds::new(0.0),
            release: Seconds::new(0.0),
            lpf: Hertz::new(20_000.0).unwrap(),
        })
        .unwrap()
    }

    fn run(voice: &mut Voice, bank: &Bank, frames: usize) -> (Vec<f32>, bool) {
        let mut left = vec![0.0; frames];
        let mut right = vec![0.0; frames];
        let alive = voice.render(bank, &mut left, &mut right);
        assert_eq!(left, right, "a mono source plays the same both sides");
        (left, alive)
    }

    #[test]
    fn a_soundfont_note_an_octave_above_its_root_reads_its_sample_twice_as_fast() {
        // A ramp sampled at 48 kHz, root C4: C5 plays it at twice the speed, so frame n of the
        // output is frame 2n of the ramp (scaled by the note's gain, 0.5, under an open filter).
        let ramp: Vec<f32> = (0..2_000).map(|n| n as f32 / 2_000.0).collect();
        let mut bank = Bank::new();
        let sample = Sample::new(ramp, None, 48_000.0).unwrap();
        bank.add_zone(0, 0, Zone::new(sample, 60.0, 0, 127, None).unwrap())
            .unwrap();
        let mut voice =
            Voice::new(&note(Source::Font(0), Some(72.0), 1.0), &bank, rate(), 0).unwrap();
        let (out, _) = run(&mut voice, &bank, 400);
        // The filter, open at 20 kHz (held at 0.45 × 48 kHz = 21.6 kHz), barely touches a ramp:
        // compare after it has settled.
        for n in [100_usize, 200, 300] {
            let expected = 0.5 * (2 * n) as f32 / 2_000.0;
            assert!(
                (out[n] - expected).abs() < 2e-3,
                "frame {n}: {} vs {expected}",
                out[n]
            );
        }
    }

    #[test]
    fn a_drum_plays_its_sample_through_whatever_the_notes_length() {
        // A 100-frame drum on a note one frame long still sounds all 100 frames, then stops.
        let mut bank = Bank::new();
        bank.set_drum(2, Sample::new(vec![1.0; 100], None, 48_000.0).unwrap())
            .unwrap();
        let mut voice = Voice::new(
            &note(Source::Drum(2), None, 1.0 / 48_000.0),
            &bank,
            rate(),
            0,
        )
        .unwrap();
        let (out, alive) = run(&mut voice, &bank, 200);
        assert!(!alive);
        assert!(out[90].abs() > 0.4);
        assert!(out[150].abs() < 1e-6);
    }

    #[test]
    fn a_looped_zone_keeps_sounding_while_held() {
        // A 100-frame sample looping frames 50..100, held for 1 000 frames: still sounding at the
        // end, which it could not be without the loop.
        let mut bank = Bank::new();
        let sample = Sample::new(vec![0.5; 100], None, 48_000.0).unwrap();
        let looped = Loop::new(50.0, 100.0, 100);
        bank.add_zone(0, 0, Zone::new(sample, 60.0, 0, 127, looped).unwrap())
            .unwrap();
        let mut voice =
            Voice::new(&note(Source::Font(0), Some(60.0), 1.0), &bank, rate(), 0).unwrap();
        let (out, alive) = run(&mut voice, &bank, 1_000);
        assert!(alive);
        assert!(out[999].abs() > 0.2);
    }

    #[test]
    fn a_looped_zone_played_off_its_root_does_not_dip_at_the_wrap() {
        // A constant 0.5 sample looping 50..100, played a fifth up (2^(7/12) = 1.498 frames a
        // frame), so the read position lands between frames all around the loop's end. The note
        // has gain 0.5 and the filter is open, so once the filter has settled every frame is
        // 0.25. Read past the loop's end from the buffer, each pass dipped towards half that.
        let mut bank = Bank::new();
        let sample = Sample::new(vec![0.5; 100], None, 48_000.0).unwrap();
        let looped = Loop::new(50.0, 100.0, 100);
        bank.add_zone(0, 0, Zone::new(sample, 60.0, 0, 127, looped).unwrap())
            .unwrap();
        let mut voice =
            Voice::new(&note(Source::Font(0), Some(67.0), 1.0), &bank, rate(), 0).unwrap();
        let (out, _) = run(&mut voice, &bank, 2_000);
        for (n, x) in out.iter().enumerate().skip(200) {
            assert!((x - 0.25).abs() < 1e-3, "frame {n}: {x}");
        }
    }

    #[test]
    fn a_held_oscillator_ends_its_release_after_its_length() {
        // Held 100 frames with no release: it stops on the 100th.
        let mut voice = Voice::new(
            &note(Source::Wave(Wave::Square), Some(60.0), 100.0 / 48_000.0),
            &Bank::new(),
            rate(),
            0,
        )
        .unwrap();
        let (out, alive) = run(&mut voice, &Bank::new(), 200);
        assert!(!alive);
        assert!(out[50].abs() > 0.1);
        assert!(out[150].abs() < 1e-9);
    }

    #[test]
    fn a_missing_instrument_is_said_not_guessed() {
        assert!(Voice::new(&note(Source::Drum(9), None, 0.1), &Bank::new(), rate(), 0).is_err());
        assert!(
            Voice::new(
                &note(Source::Font(9), Some(60.0), 0.1),
                &Bank::new(),
                rate(),
                0
            )
            .is_err()
        );
    }

    #[test]
    fn a_full_pool_steals_the_voice_nearest_its_end_else_the_oldest() {
        let bank = Bank::new();
        let held = |began| {
            Voice::new(
                &note(Source::Wave(Wave::Sine), Some(60.0), 10.0),
                &bank,
                rate(),
                began,
            )
            .unwrap()
        };
        let mut pool = Pool::new();
        for began in 0..VOICES as u64 {
            assert!(pool.start(held(began + 10), &bank).is_none());
        }
        // Nothing releasing: the oldest goes (began 10, slot 0).
        let stolen = pool.start(held(1_000), &bank).unwrap();
        assert_eq!(stolen.began, 10);
        // Release two: the one with fewer frames left goes, whatever its age.
        let mut voices: Vec<&mut Option<Voice>> = pool.voices_mut().collect();
        voices[5].as_mut().unwrap().envelope = crate::env::Envelope::new(0, 500);
        voices[5].as_mut().unwrap().envelope.release();
        voices[9].as_mut().unwrap().envelope = crate::env::Envelope::new(0, 200);
        voices[9].as_mut().unwrap().envelope.release();
        let stolen = pool.start(held(2_000), &bank).unwrap();
        assert_eq!(stolen.began, 19);
        assert_eq!(pool.sounding(), VOICES);
    }
}
