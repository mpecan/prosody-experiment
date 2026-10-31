//! A sounding note: its source, envelope, filter, colour and level (`docs/player.md` §4's per-note
//! chain), and the fixed pool voices are drawn from.
//!
//! The chain, in Strudel's order as far as its output shows it (`docs/player.md` §11): the source at
//! its level (gain, and [`SOURCE_LEVEL`]), the envelope, the low-pass, the vowel, the crush (it
//! quantises the note at that level: Strudel's `.crush(4)` on a 0.144 sine gave exactly −0.125, 0
//! and 0.125), the shape (it saw the note after the envelope: its third harmonic grows with the
//! sustained level), then `postgain` (it halves a crushed, a shaped and a
//! compressed note alike, so it comes after all three). The compressor is the channel's (`bus`).

use crate::bus::Slice;
use crate::effects::Effects;
use crate::env::Envelope;
use crate::filter::{Biquad, Coefficients, LOW_PASS_Q};
use crate::formant::Formants;
use crate::math;
use crate::note::{Channel, Note, Source};
use crate::osc::Oscillator;
use crate::sample::{Bank, Instrument, Loop, Sample};
use crate::units::SampleRate;

/// What an oscillator or a soundfont note plays at, before its gain: 0.3. Measured from Strudel's
/// output (`docs/player.md` §11): a sine at gain 1 peaks at 0.300, at gain 2 at 0.597 (linear, no
/// limit); a piano zone whose recording peaks at 0.933 plays at 0.282. A drum plays its file at
/// its own level (`uzu/bd.flac`, peaking at 0.891, came out at 0.891), so drums are 1.
pub const SOURCE_LEVEL: f64 = 0.3;

/// A note's colour: its vowel, crush and shape, each `None` when the text sets none.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Colour {
    formants: Option<Formants>,
    /// The crush's step: `2^(1 − bits)`, the quantiser's spacing on a full-scale signal.
    step: Option<f64>,
    /// The shape's `k`.
    drive: Option<f64>,
}

impl Colour {
    fn new(effects: &Effects, rate: f64) -> Self {
        Self {
            formants: effects.vowel.map(|vowel| Formants::new(vowel, rate)),
            step: effects.crush.map(|crush| math::exp2(1.0 - crush.bits())),
            drive: effects.shape.map(|shape| {
                let s = shape.amount();
                2.0 * s / (1.0 - s)
            }),
        }
    }

    /// One frame through the colour, both sides.
    fn process(&mut self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = match &mut self.formants {
            Some(formants) => formants.process(x, y),
            None => (x, y),
        };
        if let Some(step) = self.step {
            // Rounded to the nearest step, zero among them: Strudel's levels were symmetric about 0.
            x = (x / step).round() * step;
            y = (y / step).round() * step;
        }
        if let Some(k) = self.drive {
            // Bram de Jong's waveshaper (musicdsp.org, "Waveshaper", 2002), unscaled: Strudel's
            // `.shape`, measured at five drives and levels, matched it to 0.01 dB in its fundamental
            // and its third harmonic (`docs/player.md` §11).
            x = (1.0 + k) * x / (1.0 + k * x.abs());
            y = (1.0 + k) * y / (1.0 + k * y.abs());
        }
        (x, y)
    }
}

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
    /// Its level into the chain: gain, the source's level and a wave's.
    level: f64,
    colour: Colour,
    postgain: f64,
    /// What it sends, after its postgain.
    room: f64,
    echo: f64,
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
        let (generator, gate, source_level) = match spec.source {
            Source::Wave(wave) => {
                let hertz = spec.pitch.map_or(261.63, crate::units::Midi::hertz);
                (
                    Generator::Oscillator(Oscillator::new(wave, hertz, hz)),
                    held,
                    SOURCE_LEVEL * wave.level(),
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
                    1.0,
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
                    SOURCE_LEVEL,
                )
            }
        };
        let (attack, release) = (frames(note.attack(), rate), frames(note.release(), rate));
        let envelope = match note.decay() {
            Some((decay, sustain)) => {
                Envelope::with_decay(attack, frames(decay, rate), sustain, release)
            }
            None => Envelope::new(attack, release),
        };
        let effects = &spec.effects;
        Ok(Self {
            tag: spec.tag,
            channel: spec.channel,
            generator,
            envelope,
            gate,
            coefficients: Coefficients::low_pass(spec.lpf.get(), LOW_PASS_Q, hz),
            filters: [Biquad::default(); 2],
            level: spec.gain.get() * source_level,
            colour: Colour::new(effects, hz),
            postgain: spec.postgain.get(),
            room: effects.room.map_or(0.0, |room| room.send().get()),
            echo: effects.echo.map_or(0.0, |echo| echo.send().get()),
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

    /// Adds the voice's next frames into its channel's lanes, as many as `out` holds. Returns
    /// whether it is still sounding.
    pub(crate) fn render(&mut self, bank: &Bank, out: Slice<'_>) -> bool {
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
        let Slice {
            pre: [pre_l, pre_r],
            post: [post_l, post_r],
            room: [room_l, room_r],
            echo: [echo_l, echo_r],
        } = out;
        for n in 0..pre_l.len() {
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
            let x = filter_left.process(&self.coefficients, x * level);
            let y = filter_right.process(&self.coefficients, y * level);
            let (x, y) = self.colour.process(x, y);
            let (heard_x, heard_y) = (x * self.postgain, y * self.postgain);
            add(pre_l, n, x);
            add(pre_r, n, y);
            add(post_l, n, heard_x);
            add(post_r, n, heard_y);
            if self.room != 0.0 {
                add(room_l, n, heard_x * self.room);
                add(room_r, n, heard_y * self.room);
            }
            if self.echo != 0.0 {
                add(echo_l, n, heard_x * self.echo);
                add(echo_r, n, heard_y * self.echo);
            }
        }
        !self.envelope.done()
    }
}

/// Adds `value` to frame `n` of `lane`.
fn add(lane: &mut [f32], n: usize, value: f64) {
    if let Some(slot) = lane.get_mut(n) {
        #[allow(clippy::cast_possible_truncation)] // a sample to f32 PCM
        {
            *slot += value as f32;
        }
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
    #[allow(clippy::large_types_passed_by_value)] // moved into its slot once a note; a reference would be copied all the same
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
    use super::{Pool, SOURCE_LEVEL, VOICES, Voice};
    use crate::bus::Bus;
    use crate::effects::{Crush, Effects, Room, Shape};
    use crate::note::{Channel, Note, NoteSpec, Source};
    use crate::osc::Wave;
    use crate::sample::{Bank, Loop, Sample, Zone};
    use crate::units::{Gain, Hertz, Midi, SampleRate, Seconds};

    fn rate() -> SampleRate {
        SampleRate::new(48_000).unwrap()
    }

    fn spec(source: Source, pitch: Option<f64>, length: f64) -> NoteSpec {
        NoteSpec {
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
            effects: Effects::DRY,
        }
    }

    fn note(source: Source, pitch: Option<f64>, length: f64) -> Note {
        Note::new(spec(source, pitch, length)).unwrap()
    }

    /// The lanes a voice rendering `frames` frames leaves: what is heard (left), and what it sends
    /// to the reverb (left).
    fn lanes(voice: &mut Voice, bank: &Bank, frames: usize) -> (Vec<f32>, Vec<f32>, bool) {
        let mut bus = Bus::new(48_000.0);
        bus.begin(frames);
        let alive = voice.render(bank, bus.slice(0, frames).unwrap());
        let slice = bus.slice(0, frames).unwrap();
        assert_eq!(
            slice.post[0], slice.post[1],
            "a mono source plays the same both sides"
        );
        (slice.post[0].to_vec(), slice.room[0].to_vec(), alive)
    }

    fn run(voice: &mut Voice, bank: &Bank, frames: usize) -> (Vec<f32>, bool) {
        let (heard, _, alive) = lanes(voice, bank, frames);
        (heard, alive)
    }

    #[test]
    fn a_soundfont_note_an_octave_above_its_root_reads_its_sample_twice_as_fast() {
        // A ramp sampled at 48 kHz, root C4: C5 plays it at twice the speed, so frame n of the
        // output is frame 2n of the ramp (scaled by the note's gain, 0.5, and a soundfont's 0.3,
        // under an open filter).
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
            let expected = 0.15 * (2 * n) as f32 / 2_000.0;
            assert!(
                (out[n] - expected).abs() < 1e-3,
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
        assert!(out[999].abs() > 0.07);
    }

    #[test]
    fn a_looped_zone_played_off_its_root_does_not_dip_at_the_wrap() {
        // A constant 0.5 sample looping 50..100, played a fifth up (2^(7/12) = 1.498 frames a
        // frame), so the read position lands between frames all around the loop's end. The note
        // has gain 0.5, a soundfont plays at 0.3, and the filter is open, so once the filter has
        // settled every frame is 0.075. Read past the loop's end from the buffer, each pass
        // dipped towards half that.
        let mut bank = Bank::new();
        let sample = Sample::new(vec![0.5; 100], None, 48_000.0).unwrap();
        let looped = Loop::new(50.0, 100.0, 100);
        bank.add_zone(0, 0, Zone::new(sample, 60.0, 0, 127, looped).unwrap())
            .unwrap();
        let mut voice =
            Voice::new(&note(Source::Font(0), Some(67.0), 1.0), &bank, rate(), 0).unwrap();
        let (out, _) = run(&mut voice, &bank, 2_000);
        for (n, x) in out.iter().enumerate().skip(200) {
            assert!((x - 0.075).abs() < 3e-4, "frame {n}: {x}");
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
    fn a_crushed_note_takes_only_the_steps_of_its_depth() {
        // A constant 0.5 drum at gain 0.5 is 0.25 into the colour; four bits step by 2^-3 = 0.125,
        // so it stays 0.25 exactly. At gain 0.3 it is 0.15, and rounds to the nearer step, 0.125.
        let mut bank = Bank::new();
        bank.set_drum(0, Sample::new(vec![0.5; 400], None, 48_000.0).unwrap())
            .unwrap();
        for (gain, expected) in [(0.5, 0.25), (0.3, 0.125)] {
            let mut crushed = spec(Source::Drum(0), None, 0.001);
            crushed.gain = Gain::new(gain).unwrap();
            crushed.effects.crush = Crush::new(4.0);
            let mut voice = Voice::new(&Note::new(crushed).unwrap(), &bank, rate(), 0).unwrap();
            let (out, _) = run(&mut voice, &bank, 400);
            // After the open filter's few frames of settling.
            for x in &out[50..390] {
                assert!((x - expected).abs() < 1e-6, "{x} vs {expected}");
            }
        }
    }

    #[test]
    fn a_shaped_note_follows_the_curve_and_postgain_comes_after() {
        // Shape 0.5: k = 2·0.5/0.5 = 2. A constant 0.25 in becomes 3·0.25/(1 + 2·0.25) = 0.5, and a
        // postgain of ½ halves that to 0.25; it sends a quarter of what is heard to the reverb:
        // 0.0625.
        let mut bank = Bank::new();
        bank.set_drum(0, Sample::new(vec![0.5; 400], None, 48_000.0).unwrap())
            .unwrap();
        let mut shaped = spec(Source::Drum(0), None, 0.001);
        shaped.effects.shape = Shape::new(0.5);
        shaped.effects.room = Room::new(Gain::new(0.25).unwrap(), Seconds::new(1.0).unwrap());
        shaped.postgain = Gain::new(0.5).unwrap();
        let mut voice = Voice::new(&Note::new(shaped).unwrap(), &bank, rate(), 0).unwrap();
        let (heard, room, _) = lanes(&mut voice, &bank, 400);
        let expected = 0.25;
        assert!(
            (f64::from(heard[200]) - expected).abs() < 1e-5,
            "{}",
            heard[200]
        );
        assert!(
            (f64::from(room[200]) - expected * 0.25).abs() < 1e-5,
            "{}",
            room[200]
        );
    }

    #[test]
    fn a_bare_oscillator_decays_to_its_sustain_and_a_set_one_holds() {
        // A sine at gain 1, A4, nothing set: at full level (0.3) at once, 0.6 of it (0.18) once the
        // 50 ms decay is over — Strudel's measured envelope. With a release set it holds at 0.3.
        let peak = |out: &[f32], from: usize| {
            out[from..from + 4_800]
                .iter()
                .fold(0.0_f32, |p, x| p.max(x.abs()))
        };
        let mut bare = spec(Source::Wave(Wave::Sine), Some(69.0), 1.0);
        bare.gain = Gain::new(1.0).unwrap();
        bare.attack = None;
        bare.release = None;
        let mut voice = Voice::new(&Note::new(bare).unwrap(), &Bank::new(), rate(), 0).unwrap();
        let (out, _) = run(&mut voice, &Bank::new(), 24_000);
        assert!((f64::from(peak(&out, 0)) - SOURCE_LEVEL).abs() < 0.005);
        assert!((f64::from(peak(&out, 9_600)) - 0.18).abs() < 0.003);
        bare.release = Seconds::new(0.3);
        let mut voice = Voice::new(&Note::new(bare).unwrap(), &Bank::new(), rate(), 0).unwrap();
        let (out, _) = run(&mut voice, &Bank::new(), 24_000);
        assert!((f64::from(peak(&out, 9_600)) - SOURCE_LEVEL).abs() < 0.005);
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
