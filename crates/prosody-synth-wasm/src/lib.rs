//! `prosody-synth-wasm` — the player inside the page's render worker (`docs/player.md` §5, §13).
//!
//! The worker (`web/src/player/render-worker.ts`) loads the samples it is sent, is handed each
//! Performance as flat arrays of numbers — one record a note and one a cue, laid out in
//! [`prosody_player::records`] — and renders blocks of stems ahead of the playhead, which it passes
//! straight on to the audio worklet. The page's own thread is not on that path, so a page busy
//! drawing cannot make a note late.
//!
//! Everything that decides — rendering ahead, the reading's timing, which recordings to ask for —
//! is [`prosody_player::Engine`], which the app's native player runs too; this crate only moves the
//! worker's numbers and strings into it and its events back out, as JSON.

use std::collections::BTreeMap;

use prosody_player::records::{decode_cues, decode_notes};
use prosody_player::{Cast, Engine, Handed, Pace};
use prosody_synth::{Gain, Loop, Sample, SampleRate, Seconds, Zone};
use wasm_bindgen::prelude::wasm_bindgen;

pub use prosody_player::records::{CUE_STRIDE, NOTE_STRIDE as STRIDE};

/// A count of frames from JavaScript: whole and not negative.
fn frame(value: f64, what: &str) -> Result<u64, String> {
    (value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value < 9e15)
        .then(|| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked above
            let frame = value as u64;
            frame
        })
        .ok_or_else(|| format!("{what} is not a frame"))
}

/// A piece from its records, its bars' starts and its length, all in seconds.
fn handed(
    serial: u32,
    notes: &[f64],
    bars: &[f64],
    seconds: f64,
    cues: &[f64],
    cast: Cast,
) -> Result<Handed, String> {
    Ok(Handed {
        serial,
        notes: decode_notes(notes).map_err(|error| error.to_string())?,
        bars: bars.iter().filter_map(|at| Seconds::new(*at)).collect(),
        seconds: Seconds::new(seconds).ok_or("the piece's length is not a time")?,
        cues: decode_cues(cues).map_err(|error| error.to_string())?,
        cast,
    })
}

/// A pace as the page names it.
fn pace(name: &str) -> Result<Pace, String> {
    Pace::named(name).ok_or_else(|| format!("{name} is not a pace"))
}

/// The player, as the render worker drives it.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Player {
    engine: Engine,
    /// Casts given ([`Player::cast_of`]) and not yet taken by a play, a hand-over or a recast.
    casts: BTreeMap<u32, Cast>,
}

#[wasm_bindgen]
impl Player {
    /// A player at `rate` frames a second, rendering `block` frames at a time, keeping `lead` frames
    /// ahead of the worklet.
    ///
    /// # Errors
    ///
    /// A rate the synth does not run at.
    #[wasm_bindgen(constructor)]
    pub fn new(rate: u32, block: u32, lead: f64) -> Result<Self, String> {
        let rate = SampleRate::new(rate).map_err(|error| error.to_string())?;
        let block = usize::try_from(block).unwrap_or(2_048);
        Ok(Self {
            engine: Engine::new(rate, block, frame(lead, "the lead")?),
            casts: BTreeMap::new(),
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
        self.engine
            .add_drum(id, sample)
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
        self.engine
            .add_zone(id, position, zone)
            .map_err(|error| error.to_string())
    }

    /// Who reads piece `serial`'s lines: each of `lines` by the recording under the same place in
    /// `keys`, each of `browser` by the browser's own voice. Taken by the next [`Player::play`],
    /// [`Player::swap`] or [`Player::recast`].
    pub fn cast_of(&mut self, serial: u32, lines: Vec<u32>, keys: Vec<String>, browser: Vec<u32>) {
        let mut cast: Cast = lines
            .into_iter()
            .zip(keys)
            .map(|(line, key)| (line, Some(key)))
            .collect();
        cast.extend(browser.into_iter().map(|line| (line, None)));
        self.casts.insert(serial, cast);
    }

    /// Plays a Performance from its top, as start of play `generation`.
    ///
    /// # Errors
    ///
    /// A record that is not a note or a cue, a length that is not a time, a pace or duck that is not
    /// one.
    #[allow(clippy::too_many_arguments)] // one piece's parts, as the page sent them
    pub fn play(
        &mut self,
        generation: u32,
        serial: u32,
        notes: &[f64],
        bars: &[f64],
        seconds: f64,
        cues: &[f64],
        pace_name: &str,
        duck: f64,
    ) -> Result<(), String> {
        let cast = self.casts.remove(&serial).unwrap_or_default();
        let piece = handed(serial, notes, bars, seconds, cues, cast)?;
        let pace = pace(pace_name)?;
        let duck = Gain::new(duck).ok_or("the duck's level is not a level")?;
        self.engine.play(generation, piece, pace, duck);
        Ok(())
    }

    /// Hands over to a new Performance at the next bar boundary at or after the frontier.
    ///
    /// # Errors
    ///
    /// A record that is not a note or a cue, or a length that is not a time.
    pub fn swap(
        &mut self,
        serial: u32,
        notes: &[f64],
        bars: &[f64],
        seconds: f64,
        cues: &[f64],
    ) -> Result<(), String> {
        let cast = self.casts.remove(&serial).unwrap_or_default();
        let piece = handed(serial, notes, bars, seconds, cues, cast)?;
        self.engine.swap(piece);
        Ok(())
    }

    /// Stops, as start of play `generation`.
    pub fn stop(&mut self, generation: u32) {
        self.engine.stop(generation);
    }

    /// The casts given since, at `pace_name`; `quiet` lets go of what is placed from the frontier.
    ///
    /// # Errors
    ///
    /// A pace that is not one.
    pub fn recast(&mut self, pace_name: &str, quiet: bool) -> Result<(), String> {
        let casts = std::mem::take(&mut self.casts);
        self.engine.cast(&casts, pace(pace_name)?, quiet);
        Ok(())
    }

    /// A recorded line under render key `key`: its mono PCM at `rate`, its speech `lead` seconds in.
    ///
    /// # Errors
    ///
    /// A recording the synth cannot play, or more than it holds.
    pub fn line(&mut self, key: &str, pcm: Vec<f32>, rate: f64, lead: f64) -> Result<(), String> {
        let sample = Sample::new(pcm, None, rate).map_err(|error| error.to_string())?;
        let lead = Seconds::new(lead).ok_or("the line's lead is not a time")?;
        self.engine
            .line(key, sample, lead)
            .map_err(|error| error.to_string())
    }

    /// The browser's voice finished chain `chain`'s line, the playhead at frame `end`.
    ///
    /// # Errors
    ///
    /// A frame that is not one.
    pub fn said(&mut self, chain: u32, end: f64) -> Result<(), String> {
        self.engine.said(chain, frame(end, "the line's end")?);
        Ok(())
    }

    /// The page could not render the line under `key`.
    pub fn failed(&mut self, key: &str) {
        self.engine.failed(key);
    }

    /// The music's level under a line of the reading: 1 for no duck.
    ///
    /// # Errors
    ///
    /// A level that is not one.
    pub fn set_duck(&mut self, level: f64) -> Result<(), String> {
        let level = Gain::new(level).ok_or("the duck's level is not a level")?;
        self.engine.duck(level);
        Ok(())
    }

    /// The worklet has played up to frame `frame` of start of play `generation`.
    ///
    /// # Errors
    ///
    /// A frame that is not one.
    pub fn played(&mut self, generation: u32, frame_played: f64) -> Result<(), String> {
        self.engine
            .played(generation, frame(frame_played, "the frame played")?);
        Ok(())
    }

    /// The flush to post to the worklet before the next block, if a restart is waiting to be said.
    pub fn take_flush(&mut self) -> Option<u32> {
        self.engine.take_flush()
    }

    /// Whether a block is due.
    #[must_use]
    pub fn due(&self) -> bool {
        self.engine.due()
    }

    /// Renders the next block into `out` — [`prosody_synth::STEMS`] stereo stems, planar (each
    /// stem's left, then its right), `block` frames each, then the duck ([`prosody_synth::Stems`])
    /// — and gives the frame it starts on.
    pub fn render(&mut self, out: &mut [f32]) -> f64 {
        let (frame, stems) = self.engine.render();
        for (to, from) in out.iter_mut().zip(stems.as_slice()) {
            *to = *from;
        }
        frames(frame)
    }

    /// What the page is to be told, oldest first, as a JSON array of the worker's messages.
    pub fn drain(&mut self) -> String {
        let events: Vec<String> = self
            .engine
            .drain()
            .iter()
            .map(prosody_player::Event::json)
            .collect();
        format!("[{}]", events.join(","))
    }

    /// The start of play the blocks rendered now belong to: the last restart's.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn generation(&self) -> u32 {
        self.engine.generation()
    }

    /// Frames a block.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn block(&self) -> u32 {
        u32::try_from(self.engine.block()).unwrap_or(0)
    }

    /// The next output frame to render.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn frontier(&self) -> f64 {
        frames(self.engine.frontier())
    }

    /// Voices cut off early because every voice was busy.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn stolen(&self) -> f64 {
        frames(self.engine.losses().stolen)
    }

    /// Notes not played because their sound was not loaded.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn missing(&self) -> f64 {
        frames(self.engine.losses().missing)
    }
}

/// A count for JavaScript, whose numbers are doubles: exact below 2^53, which is three thousand
/// years of audio.
fn frames(count: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)] // see above
    let count = count as f64;
    count
}

// Tests may unwrap and index freely; a panic in a test is a test failure. See CLAUDE.md §3.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::{CUE_STRIDE, Player, STRIDE};
    use prosody_synth::Stems;

    /// A sine's record on the melody (`web/test/player.test.ts` pins the same layout).
    fn sine(onset: f64) -> [f64; STRIDE] {
        let mut r = [f64::NAN; STRIDE];
        r[..12].copy_from_slice(&[
            0.0,
            2.0,
            2.0,
            0.0,
            onset,
            0.5,
            69.0,
            0.8,
            1.0,
            f64::NAN,
            0.1,
            3_000.0,
        ]);
        r[15] = 0.0;
        r[16] = 1.0;
        r[18] = 0.0;
        r[19] = 0.0;
        r[21] = 1.0;
        r
    }

    #[test]
    fn it_plays_and_renders_a_block_of_stems_ahead_of_the_worklet() {
        let mut player = Player::new(48_000, 1_024, 4_096.0).unwrap();
        // A sine on the melody stem (stem 2, whose left side starts 4 × 1 024 samples in) at
        // 2 048 frames: in block 2.
        player
            .play(
                1,
                1,
                &sine(2_048.0 / 48_000.0),
                &[0.0],
                2.0,
                &[],
                "natural",
                1.0,
            )
            .unwrap();
        assert_eq!(player.take_flush(), Some(1));
        let mut out = vec![0.0_f32; Stems::PER_FRAME * 1_024];
        let mut heard = Vec::new();
        while player.due() {
            let frame = player.render(&mut out);
            heard.push((
                frame,
                out[4 * 1_024..5 * 1_024].iter().any(|x| x.abs() > 1e-3),
            ));
        }
        // Four blocks fill the 4 096-frame lead; the sine sounds from the third.
        assert_eq!(
            heard,
            vec![
                (0.0, false),
                (1_024.0, false),
                (2_048.0, true),
                (3_072.0, true)
            ]
        );
        player.played(1, 1_024.0).unwrap();
        assert!(player.due());
        assert!(player.played(1, -1.0).is_err());
    }

    #[test]
    fn its_events_are_a_json_array_of_the_workers_messages() {
        let mut player = Player::new(48_000, 1_024, 4_096.0).unwrap();
        player.cast_of(7, vec![0], vec!["k|v".to_owned()], vec![1]);
        // Line 0, fitted on bar 0; line 1 the browser's.
        let cue = [0.0, 0.0, 0.25, 0.0, 0.0, f64::NAN, f64::NAN];
        let mut cues = cue.to_vec();
        cues.extend([1.0, 0.0, 0.25, 0.0, 0.5, f64::NAN, f64::NAN]);
        assert_eq!(cues.len(), 2 * CUE_STRIDE);
        player
            .play(2, 7, &[], &[0.0, 0.5], 1.0, &cues, "fitted", 0.6)
            .unwrap();
        assert_eq!(
            player.drain(),
            r#"[{"op":"placement","outStart":0,"pieceStart":0,"serial":7},{"op":"want","keys":["k|v"]}]"#
        );
        assert_eq!(player.drain(), "[]");
    }

    #[test]
    fn what_is_not_a_piece_is_refused_not_guessed() {
        let mut player = Player::new(48_000, 1_024, 4_096.0).unwrap();
        assert!(
            player
                .play(1, 1, &[0.0; 3], &[0.0], 2.0, &[], "natural", 1.0)
                .is_err()
        );
        assert!(
            player
                .play(1, 1, &[], &[0.0], 2.0, &[1.0], "natural", 1.0)
                .is_err()
        );
        assert!(
            player
                .play(1, 1, &[], &[0.0], 2.0, &[], "brisk", 1.0)
                .is_err()
        );
        assert!(
            player
                .play(1, 1, &[], &[0.0], f64::NAN, &[], "natural", 1.0)
                .is_err()
        );
        assert!(player.set_duck(-1.0).is_err());
        assert!(player.line("k", vec![0.1; 4], 0.0, 0.0).is_err());
        assert!(Player::new(1_000, 1_024, 0.0).is_err());
    }
}
