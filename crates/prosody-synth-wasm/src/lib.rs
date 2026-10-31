//! `prosody-synth-wasm` — the synth inside the page's render worker (`docs/player.md` §5).
//!
//! The worker (`web/src/player/render-worker.ts`) loads the samples it is sent, is handed each
//! Performance as a flat array of numbers — one [`STRIDE`]-long record a note, laid out below —
//! and renders blocks of stems ahead of the playhead, which it passes straight on to the
//! audio worklet. The page's own thread is not on that path, so a page busy drawing cannot make a
//! note late.
//!
//! # A note's record
//!
//! | index | field | |
//! |---|---|---|
//! | 0 | tag | the note's index in the Performance's `notes` (what it is attributed to, I4) |
//! | 1 | channel | 0 drums, 1 bass, 2 melody, 3 chords |
//! | 2 | kind | 0 drum, 1 soundfont, 2 oscillator |
//! | 3 | id | the instrument number the page loaded it as; for an oscillator 0 sine, 1 sawtooth, 2 square, 3 triangle |
//! | 4 | onset | seconds from the start of the piece |
//! | 5 | length | seconds |
//! | 6 | pitch | MIDI, or NaN for none |
//! | 7 | gain | `gain` × `velocity` |
//! | 8 | postgain | |
//! | 9 | attack | seconds, or NaN where the text sets none |
//! | 10 | release | seconds, or NaN where the text sets none |
//! | 11 | lpf | Hz |
//! | 12 | crush | bits, or NaN for none |
//! | 13 | vowel | 0 a, 1 e, 2 i, 3 o, 4 u, or NaN for none |
//! | 14 | shape | 0..1, or NaN for none |
//! | 15 | room | the reverb send; 0 for none |
//! | 16 | roomsize | the reverb's RT60, seconds (read only with a send) |
//! | 17 | delay | the echo send, or NaN for none |
//! | 18 | delaytime | seconds |
//! | 19 | delayfeedback | 0..1 |
//! | 20 | compressor | threshold, dB, or NaN for none |
//! | 21 | compressorRatio | `n` for `n`:1 |
//!
//! The page writes it (`web/src/player/notes.ts`), and a test on each side pins the layout.
//!
//! # The reading
//!
//! The worker also places the reading's lines (`docs/player.md` §5, "The reading"): each recorded
//! line is loaded once ([`Player::add_line`]) and placed at an output frame ([`Player::speak`]),
//! which gives back where it ends, so the worker can time the next line from it. The synth writes
//! the duck from the same placements.

use prosody_synth::{
    Bank, Channel, Compression, Crush, Echo, Effects, Gain, Hertz, Loop, Midi, Note, NoteSpec,
    Piece, Placement, Renderer, Repeat, Room, Sample, SampleRate, Seconds, Shape, Source, Stems,
    SynthError, Utterance, Vowel, Wave, Zone,
};
use wasm_bindgen::prelude::wasm_bindgen;

/// Numbers a note's record holds.
pub const STRIDE: usize = 22;

/// The notes of a Performance from their records, in order.
///
/// # Errors
///
/// The first record that does not describe a note, by its tag.
pub fn decode_notes(records: &[f64]) -> Result<Vec<Note>, SynthError> {
    if !records.len().is_multiple_of(STRIDE) {
        return Err(SynthError::Malformed {
            tag: 0,
            why: "the records are not a whole number of notes long",
        });
    }
    records.chunks_exact(STRIDE).map(decode_note).collect()
}

/// One note from its record.
fn decode_note(record: &[f64]) -> Result<Note, SynthError> {
    let field = |index: usize| record.get(index).copied().unwrap_or(f64::NAN);
    let tag = whole(field(0)).and_then(|tag| u32::try_from(tag).ok());
    let Some(tag) = tag else {
        return Err(SynthError::Malformed {
            tag: 0,
            why: "its tag is not a note's index",
        });
    };
    let malformed = |why| SynthError::Malformed { tag, why };
    // The reading's stem (4) takes no Performance note: it comes with the reading, in step 4.
    let channel = whole(field(1))
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < 4)
        .and_then(Channel::from_index)
        .ok_or_else(|| malformed("its channel is not 0–3"))?;
    let id = whole(field(3))
        .and_then(|id| u32::try_from(id).ok())
        .ok_or_else(|| malformed("its sound's number is not a number"))?;
    let source = match whole(field(2)) {
        Some(0) => Source::Drum(id),
        Some(1) => Source::Font(id),
        Some(2) => Source::Wave(match id {
            0 => Wave::Sine,
            1 => Wave::Sawtooth,
            2 => Wave::Square,
            3 => Wave::Triangle,
            _ => return Err(malformed("its wave is not 0–3")),
        }),
        _ => return Err(malformed("its kind is not 0–2")),
    };
    let seconds = |index, why| Seconds::new(field(index)).ok_or_else(|| malformed(why));
    let optional = |index, why| {
        let value = field(index);
        if value.is_nan() {
            Ok(None)
        } else {
            Seconds::new(value).map(Some).ok_or_else(|| malformed(why))
        }
    };
    let pitch = if field(6).is_nan() {
        None
    } else {
        Some(Midi::new(field(6)).ok_or_else(|| malformed("its pitch is not a MIDI note"))?)
    };
    Note::new(NoteSpec {
        tag,
        channel,
        source,
        onset: seconds(4, "its onset is not a time")?,
        length: seconds(5, "its length is not a time")?,
        pitch,
        gain: Gain::new(field(7)).ok_or_else(|| malformed("its gain is not a level"))?,
        postgain: Gain::new(field(8)).ok_or_else(|| malformed("its postgain is not a level"))?,
        attack: optional(9, "its attack is not a time")?,
        release: optional(10, "its release is not a time")?,
        lpf: Hertz::new(field(11)).ok_or_else(|| malformed("its cutoff is not a frequency"))?,
        effects: decode_effects(&field, &malformed)?,
    })
}

/// A note's effects from fields 12–21 of its record.
fn decode_effects(
    field: &dyn Fn(usize) -> f64,
    malformed: &dyn Fn(&'static str) -> SynthError,
) -> Result<Effects, SynthError> {
    // A field that is NaN is an effect the text does not set; any other value must be one the
    // synth can mean.
    let optional = |index: usize| Some(field(index)).filter(|value| !value.is_nan());
    let crush = optional(12)
        .map(|bits| Crush::new(bits).ok_or_else(|| malformed("its crush is not 1–16 bits")))
        .transpose()?;
    let vowel = optional(13)
        .map(|index| {
            whole(index)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(Vowel::from_index)
                .ok_or_else(|| malformed("its vowel is not 0–4"))
        })
        .transpose()?;
    let shape = optional(14)
        .map(|amount| Shape::new(amount).ok_or_else(|| malformed("its shape is not 0–0.99")))
        .transpose()?;
    let send = Gain::new(field(15)).ok_or_else(|| malformed("its room is not a level"))?;
    let room = if send.get() > 0.0 {
        let size =
            Seconds::new(field(16)).ok_or_else(|| malformed("its roomsize is not a time"))?;
        Some(Room::new(send, size).ok_or_else(|| malformed("its roomsize is not 0–10 s"))?)
    } else {
        None
    };
    let echo = optional(17)
        .map(|level| {
            let send = Gain::new(level).ok_or_else(|| malformed("its delay is not a level"))?;
            let time =
                Seconds::new(field(18)).ok_or_else(|| malformed("its delaytime is not a time"))?;
            Echo::new(send, time, field(19)).ok_or_else(|| malformed("its delay is not an echo"))
        })
        .transpose()?;
    let compression = optional(20)
        .map(|threshold| {
            Compression::new(threshold, field(21))
                .ok_or_else(|| malformed("its compressor is not a compressor"))
        })
        .transpose()?;
    Ok(Effects {
        crush,
        vowel,
        shape,
        room,
        echo,
        compression,
    })
}

/// `value` as a whole number, if it is one.
fn whole(value: f64) -> Option<i64> {
    #[allow(clippy::cast_possible_truncation)] // checked to be whole and in range first
    let whole =
        (value.is_finite() && value.fract() == 0.0 && value.abs() < 9e15).then_some(value as i64);
    whole
}

/// A count of frames from JavaScript: whole and not negative.
fn frame(value: f64, what: &str) -> Result<u64, String> {
    whole(value)
        .and_then(|frame| u64::try_from(frame).ok())
        .ok_or_else(|| format!("{what} is not a frame"))
}

/// A piece from a Performance's records, its bars' starts and its length, all in seconds.
fn piece(records: &[f64], bars: &[f64], seconds: f64, rate: SampleRate) -> Result<Piece, String> {
    let notes = decode_notes(records).map_err(|error| error.to_string())?;
    let bars: Vec<Seconds> = bars.iter().filter_map(|at| Seconds::new(*at)).collect();
    let length = Seconds::new(seconds).ok_or("the piece's length is not a time")?;
    Ok(Piece::new(notes, &bars, length, Repeat::Loop, rate))
}

/// The synth, as the render worker drives it.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Player {
    renderer: Renderer,
    stems: Stems,
    placement: Placement,
}

#[wasm_bindgen]
impl Player {
    /// A player at `rate` frames a second, rendering `block` frames at a time.
    ///
    /// # Errors
    ///
    /// A rate the synth does not run at.
    #[wasm_bindgen(constructor)]
    pub fn new(rate: u32, block: u32) -> Result<Self, String> {
        let rate = SampleRate::new(rate).map_err(|error| error.to_string())?;
        let frames = usize::try_from(block.clamp(128, 16_384)).unwrap_or(2_048);
        Ok(Self {
            renderer: Renderer::new(rate, Bank::new()),
            stems: Stems::new(frames),
            placement: Placement {
                out_start: 0,
                piece_start: 0,
            },
        })
    }

    /// Loads a drum as instrument `id`: its left channel, its right (empty for mono), its rate.
    ///
    /// # Errors
    ///
    /// A sample the synth cannot play, or a number past the bank's end.
    pub fn add_drum(
        &mut self,
        id: u32,
        left: Vec<f32>,
        right: Vec<f32>,
        rate: f64,
    ) -> Result<(), String> {
        let sample = Sample::new(left, (!right.is_empty()).then_some(right), rate)
            .map_err(|error| error.to_string())?;
        self.renderer
            .bank_mut()
            .set_drum(id, sample)
            .map_err(|error| error.to_string())
    }

    /// Adds zone `position` of its file (its index among the file's zones, which decides between
    /// zones that could both play a key: `Instrument::zone`) to soundfont instrument `id`: its
    /// sample's channels and rate, the pitch it sounds at its own rate, its keys, and its loop in
    /// the sample's frames (NaN for none).
    ///
    /// # Errors
    ///
    /// A sample or zone the synth cannot play, or a number past the bank's end.
    #[allow(clippy::too_many_arguments)] // one zone's fields, as the page parsed them
    pub fn add_zone(
        &mut self,
        id: u32,
        position: u32,
        left: Vec<f32>,
        right: Vec<f32>,
        rate: f64,
        root: f64,
        low: u8,
        high: u8,
        loop_start: f64,
        loop_end: f64,
    ) -> Result<(), String> {
        let sample = Sample::new(left, (!right.is_empty()).then_some(right), rate)
            .map_err(|error| error.to_string())?;
        let looped = Loop::new(loop_start, loop_end, sample.frames());
        let zone = Zone::new(sample, root, low, high, looped).map_err(|error| error.to_string())?;
        self.renderer
            .bank_mut()
            .add_zone(id, position, zone)
            .map_err(|error| error.to_string())
    }

    /// Plays a Performance from its top, from the frontier on.
    ///
    /// # Errors
    ///
    /// A record that is not a note, or a length that is not a time.
    pub fn play(&mut self, notes: &[f64], bars: &[f64], seconds: f64) -> Result<(), String> {
        let piece = piece(notes, bars, seconds, self.renderer.rate())?;
        self.placement = self.renderer.play(piece);
        Ok(())
    }

    /// Hands over to a new Performance at the next bar boundary at or after the frontier.
    ///
    /// # Errors
    ///
    /// A record that is not a note, or a length that is not a time.
    pub fn swap(&mut self, notes: &[f64], bars: &[f64], seconds: f64) -> Result<(), String> {
        let piece = piece(notes, bars, seconds, self.renderer.rate())?;
        self.placement = self.renderer.swap(piece);
        Ok(())
    }

    /// Stops: every voice silenced, every line of the reading let go.
    pub fn stop(&mut self) {
        self.renderer.stop();
    }

    /// Loads a recorded line of the reading as line `id`: its mono PCM and the rate it is at.
    ///
    /// # Errors
    ///
    /// A recording the synth cannot play, or a number past the most it holds.
    pub fn add_line(&mut self, id: u32, pcm: Vec<f32>, rate: f64) -> Result<(), String> {
        let sample = Sample::new(pcm, None, rate).map_err(|error| error.to_string())?;
        self.renderer
            .add_line(id, sample)
            .map_err(|error| error.to_string())
    }

    /// Lets line `id` go; where it is already placed, it still plays.
    pub fn drop_line(&mut self, id: u32) {
        self.renderer.drop_line(id);
    }

    /// Places line `id` on the speech stem, its recording from output frame `start`, its speech
    /// from `speech`, the music kept down `hold` frames after it; and gives the frame after its
    /// last, which the reading's chaining times the next line from.
    ///
    /// # Errors
    ///
    /// A line not loaded, a frame that is not one, or more lines placed than the synth holds.
    pub fn speak(&mut self, id: u32, start: f64, speech: f64, hold: f64) -> Result<f64, String> {
        let spoken = self
            .renderer
            .speak(Utterance {
                line: id,
                start: frame(start, "the line's start")?,
                speech: frame(speech, "the line's speech")?,
                hold: frame(hold, "the line's hold")?,
            })
            .map_err(|error| error.to_string())?;
        Ok(frames(spoken.end))
    }

    /// Cuts the reading off at output frame `at`: a line sounding there fades out, one placed to
    /// start at or after it is let go.
    ///
    /// # Errors
    ///
    /// A frame that is not one.
    pub fn cut_speech(&mut self, at: f64) -> Result<(), String> {
        self.renderer.cut_speech(frame(at, "the cut")?);
        Ok(())
    }

    /// The music's level under a line of the reading: 1 for no duck.
    ///
    /// # Errors
    ///
    /// A level that is not one.
    pub fn set_duck(&mut self, level: f64) -> Result<(), String> {
        let level = Gain::new(level).ok_or("the duck's level is not a level")?;
        self.renderer.set_duck(level);
        Ok(())
    }

    /// Renders the next block into `out`: [`prosody_synth::STEMS`] stereo stems, planar (each
    /// stem's left, then its right), `block` frames each, then the duck, `block` frames more
    /// ([`Stems`]).
    pub fn render(&mut self, out: &mut [f32]) {
        self.renderer.render(&mut self.stems);
        for (to, from) in out.iter_mut().zip(self.stems.as_slice()) {
            *to = *from;
        }
    }

    /// Frames a block.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn block(&self) -> u32 {
        u32::try_from(self.stems.frames()).unwrap_or(0)
    }

    /// The next output frame to render.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn frontier(&self) -> f64 {
        frames(self.renderer.frontier())
    }

    /// The output frame the last piece handed over (or will hand over) on.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn out_start(&self) -> f64 {
        frames(self.placement.out_start)
    }

    /// The frame of its own it plays from there.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn piece_start(&self) -> f64 {
        frames(self.placement.piece_start)
    }

    /// Voices cut off early because every voice was busy.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn stolen(&self) -> f64 {
        frames(self.renderer.losses().stolen)
    }

    /// Notes not played because their sound was not loaded.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn missing(&self) -> f64 {
        frames(self.renderer.losses().missing)
    }
}

/// A count of frames for JavaScript, whose numbers are doubles: exact below 2^53, which is three
/// thousand years of audio.
fn frames(count: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // see above
    let count = count as f64;
    count
}

// Tests may unwrap and index freely; a panic in a test is a test failure. See CLAUDE.md §3.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::{Player, STRIDE, Stems, decode_notes};
    use prosody_synth::{Channel, Source, Wave};

    /// A record as the page writes it (`web/test/player.test.ts` pins the same layout).
    fn record(tag: f64, kind: f64, id: f64, pitch: f64) -> [f64; STRIDE] {
        [
            tag,
            2.0,
            kind,
            id,
            0.25,
            0.5,
            pitch,
            0.8,
            1.0,
            f64::NAN,
            0.1,
            3_000.0,
            f64::NAN,
            f64::NAN,
            f64::NAN,
            0.0,
            2.5,
            f64::NAN,
            0.0,
            0.0,
            f64::NAN,
            1.0,
        ]
    }

    #[test]
    fn a_record_is_read_field_by_field() {
        let notes = decode_notes(&record(3.0, 2.0, 1.0, 64.0)).unwrap();
        let spec = notes[0].spec();
        assert_eq!(spec.tag, 3);
        assert_eq!(spec.channel, Channel::Melody);
        assert_eq!(spec.source, Source::Wave(Wave::Sawtooth));
        assert!((spec.onset.get() - 0.25).abs() < 1e-12);
        assert!((spec.length.get() - 0.5).abs() < 1e-12);
        assert!((spec.pitch.unwrap().get() - 64.0).abs() < 1e-12);
        assert!((spec.gain.get() - 0.8).abs() < 1e-12);
        assert!(spec.attack.is_none());
        assert!((spec.release.unwrap().get() - 0.1).abs() < 1e-12);
        assert!((spec.lpf.get() - 3_000.0).abs() < 1e-9);
        assert_eq!(spec.effects, prosody_synth::Effects::DRY);
    }

    #[test]
    fn a_records_effects_are_read_field_by_field() {
        let mut r = record(4.0, 2.0, 0.0, 60.0);
        r[12..22].copy_from_slice(&[8.0, 3.0, 0.1, 0.35, 2.5, 0.2, 0.3, 0.35, -24.0, 7.0]);
        let notes = decode_notes(&r).unwrap();
        let effects = notes[0].spec().effects;
        assert!((effects.crush.unwrap().bits() - 8.0).abs() < 1e-12);
        assert_eq!(effects.vowel, Some(prosody_synth::Vowel::O));
        assert!((effects.shape.unwrap().amount() - 0.1).abs() < 1e-12);
        let room = effects.room.unwrap();
        assert!(
            (room.send().get() - 0.35).abs() < 1e-12 && (room.size().get() - 2.5).abs() < 1e-12
        );
        let echo = effects.echo.unwrap();
        assert!((echo.send().get() - 0.2).abs() < 1e-12);
        assert!((echo.time().get() - 0.3).abs() < 1e-12 && (echo.feedback() - 0.35).abs() < 1e-12);
        let compression = effects.compression.unwrap();
        assert!((compression.threshold() + 24.0).abs() < 1e-12);
        assert!((compression.ratio() - 7.0).abs() < 1e-12);
        // Out of range is refused by the note's tag, not dropped.
        r[12] = 20.0;
        assert!(decode_notes(&r).unwrap_err().to_string().contains("note 4"));
    }

    #[test]
    fn a_record_that_is_not_a_note_is_refused_by_its_tag() {
        let error = decode_notes(&record(5.0, 7.0, 0.0, 60.0)).unwrap_err();
        assert!(error.to_string().contains("note 5"), "{error}");
        assert!(decode_notes(&record(1.5, 0.0, 0.0, f64::NAN)).is_err());
        assert!(decode_notes(&[0.0; STRIDE - 1]).is_err());
        // A soundfont note with no pitch.
        assert!(decode_notes(&record(2.0, 1.0, 0.0, f64::NAN)).is_err());
    }

    #[test]
    fn it_plays_and_renders_a_block_of_stems() {
        let mut player = Player::new(48_000, 1_024).unwrap();
        // A sine on the melody stem (stem 2, whose left side starts 4 × 1 024 samples in) at a
        // quarter of a second: frame 12 000, in block 11 (frames 11 264..12 288).
        player
            .play(&record(0.0, 2.0, 0.0, 69.0), &[0.0], 2.0)
            .unwrap();
        let mut out = vec![0.0_f32; Stems::PER_FRAME * 1_024];
        let mut heard = Vec::new();
        for _ in 0..13 {
            player.render(&mut out);
            heard.push(out[4 * 1_024..5 * 1_024].iter().any(|x| x.abs() > 1e-3));
        }
        // Silent until block 11 (frames 11 264..12 288), sounding in it and after.
        assert_eq!(heard.iter().position(|h| *h), Some(11));
        assert!((player.frontier() - 13.0 * 1_024.0).abs() < 0.5);
        assert!(player.missing() < 0.5);
    }

    #[test]
    fn a_line_of_the_reading_is_placed_on_the_speech_stem_and_ducks_the_music() {
        let mut player = Player::new(48_000, 1_024).unwrap();
        player.play(&[], &[0.0], 2.0).unwrap();
        // 480 frames at 24 kHz: 960 at 48 kHz, from frame 100, its speech from 120.
        player.add_line(2, vec![0.25; 480], 24_000.0).unwrap();
        player.set_duck(0.6).unwrap();
        let end = player.speak(2, 100.0, 120.0, 0.0).unwrap();
        assert!((end - 1_060.0).abs() < 0.5);
        let mut out = vec![0.0_f32; Stems::PER_FRAME * 1_024];
        player.render(&mut out);
        // The speech stem is stem 4: its left side from 8 × 1 024, its right from 9 × 1 024; the
        // duck from 10 × 1 024.
        let left = &out[8 * 1_024..9 * 1_024];
        let right = &out[9 * 1_024..10 * 1_024];
        let duck = &out[10 * 1_024..11 * 1_024];
        assert!(left[..100].iter().all(|x| *x == 0.0));
        assert!((left[500] - 0.25).abs() < 1e-6 && (right[500] - 0.25).abs() < 1e-6);
        assert!(duck[..120].iter().all(|x| (*x - 1.0).abs() < 1e-9) && duck[500] < 1.0);
        // Not a frame, not loaded, not a level: refused, not guessed.
        assert!(player.speak(2, 1.5, 2.0, 0.0).is_err());
        assert!(player.speak(9, 0.0, 0.0, 0.0).is_err());
        assert!(player.set_duck(-1.0).is_err());
        assert!(player.cut_speech(f64::NAN).is_err());
    }

    #[test]
    fn a_note_whose_sound_was_never_loaded_is_counted() {
        let mut player = Player::new(48_000, 1_024).unwrap();
        player
            .play(&record(0.0, 0.0, 4.0, f64::NAN), &[0.0], 2.0)
            .unwrap();
        let mut out = vec![0.0_f32; Stems::PER_FRAME * 1_024];
        for _ in 0..12 {
            player.render(&mut out);
        }
        assert!((player.missing() - 1.0).abs() < 0.5);
    }
}
