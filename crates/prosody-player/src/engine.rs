//! The engine: the synth kept rendering ahead of what the output has played, with the reading
//! placed on it as it goes (`docs/player.md` §5, and §13 for why it is here).
//!
//! It is what the browser's render worker did until step 5, as Rust, so that the app's native
//! player and the browser run the same thing: a host — the worker (`prosody-synth-wasm`), or the
//! app's render thread (`prosody-app`) — hands it the page's messages as calls, asks it for blocks
//! while it is less than its lead ahead of the output ([`Engine::due`], [`Engine::render`]), sends
//! each block on to whatever mixes them (the worklet, or the app's output), and passes what it says
//! ([`Engine::drain`]) back to the page.
//!
//! A restart from the top (a Play, a Stop) drops everything queued downstream; the engine says so
//! with [`Engine::take_flush`], which the host sends down the same path as the blocks, ahead of the
//! new ones, so nothing rendered before a stop can be heard after it.

use std::collections::BTreeMap;

use prosody_synth::{
    Bank, Gain, Losses, Note, Piece, Placement, Renderer, Repeat, Sample, SampleRate, Seconds,
    Stems, SynthError, Utterance, Zone,
};

use crate::event::Event;
use crate::reader::{Cast, Cue, Held, Pace, Reader, Segment, Told, Voice};

/// A piece the page hands over: its notes, bars and length, its reading's cues, who reads them, and
/// the serial the page knows it by.
#[derive(Debug, Clone, PartialEq)]
pub struct Handed {
    /// The serial the page will match its placement by.
    pub serial: u32,
    /// Its notes.
    pub notes: Vec<Note>,
    /// Where its bars begin.
    pub bars: Vec<Seconds>,
    /// Its length: where it loops round.
    pub seconds: Seconds,
    /// Its reading's cues.
    pub cues: Vec<Cue>,
    /// Who reads them.
    pub cast: Cast,
}

/// The synth rendering ahead, and the reading on it.
#[derive(Debug, Clone)]
pub struct Engine {
    renderer: Renderer,
    stems: Stems,
    reader: Reader,
    /// Frames the output may be behind the frontier before rendering stops.
    lead: u64,
    playing: bool,
    /// The start of play everything belongs to: the page's count, so what it is told of an older
    /// one is recognised as stale.
    generation: u32,
    /// The output frame the mixer has played up to, as it last said.
    played: u64,
    /// The losses last told.
    told: Losses,
    /// Numbers the synth's lines are free to take: a line let go gives its number back.
    free_lines: Vec<u32>,
    next_line: u32,
    events: Vec<Event>,
    flush: Option<u32>,
}

impl Engine {
    /// An engine at `rate`, rendering `block` frames at a time (128..=16 384) and keeping `lead`
    /// frames ahead of the output.
    #[must_use]
    pub fn new(rate: SampleRate, block: usize, lead: u64) -> Self {
        Self {
            renderer: Renderer::new(rate, Bank::new()),
            stems: Stems::new(block.clamp(128, 16_384)),
            reader: Reader::new(rate.get()),
            lead,
            playing: false,
            generation: 0,
            played: 0,
            told: Losses::default(),
            free_lines: Vec::new(),
            next_line: 0,
            events: Vec::new(),
            flush: None,
        }
    }

    /// Its output rate.
    #[must_use]
    pub const fn rate(&self) -> SampleRate {
        self.renderer.rate()
    }

    /// Frames a block.
    #[must_use]
    pub const fn block(&self) -> usize {
        self.stems.frames()
    }

    /// Whether a piece is playing.
    #[must_use]
    pub const fn playing(&self) -> bool {
        self.playing
    }

    /// The start of play it is in.
    #[must_use]
    pub const fn generation(&self) -> u32 {
        self.generation
    }

    /// The next output frame it will render.
    #[must_use]
    pub const fn frontier(&self) -> u64 {
        self.renderer.frontier()
    }

    /// What the synth has lost so far.
    #[must_use]
    pub const fn losses(&self) -> Losses {
        self.renderer.losses()
    }

    /// The sounds, to load a drum or a zone into.
    pub fn bank_mut(&mut self) -> &mut Bank {
        self.renderer.bank_mut()
    }

    /// Loads a drum as instrument `id`.
    ///
    /// # Errors
    ///
    /// A number past the bank's end.
    pub fn add_drum(&mut self, id: u32, sample: Sample) -> Result<(), SynthError> {
        self.renderer.bank_mut().set_drum(id, sample)
    }

    /// Adds zone `position` of its file to soundfont instrument `id`.
    ///
    /// # Errors
    ///
    /// A number past the bank's end.
    pub fn add_zone(&mut self, id: u32, position: u32, zone: Zone) -> Result<(), SynthError> {
        self.renderer.bank_mut().add_zone(id, position, zone)
    }

    /// Plays `handed` from its top, as start of play `generation`: everything sounding stops, and
    /// what is queued downstream is dropped ([`Engine::take_flush`]).
    pub fn play(&mut self, generation: u32, handed: Handed, pace: Pace, duck: Gain) {
        self.renderer.stop();
        self.restart(generation);
        self.renderer.set_duck(duck);
        let (piece, rest) = split(handed, self.rate());
        let placement = self.renderer.play(piece);
        self.reader.pace = pace;
        let serial = rest.serial;
        self.reader.play(rest.segment(placement));
        self.playing = true;
        self.placed(serial, placement);
        self.want_now();
    }

    /// Hands over to `handed` at the next bar boundary at or after the frontier. A hand-over with
    /// nothing playing is stale (sent after a Stop the page had not yet noticed) and is ignored: a
    /// start of play comes as [`Engine::play`].
    pub fn swap(&mut self, handed: Handed) {
        if !self.playing {
            return;
        }
        let (piece, rest) = split(handed, self.rate());
        let placement = self.renderer.swap(piece);
        let serial = rest.serial;
        self.reader.swap(rest.segment(placement));
        self.placed(serial, placement);
        self.want_now();
    }

    /// Stops, as start of play `generation`: every voice silenced, the reading let go, what is
    /// queued downstream dropped. The recorded lines are kept: a Play straight after is most likely
    /// the same piece, and the next piece handed over lets go of any it does not cast.
    pub fn stop(&mut self, generation: u32) {
        self.playing = false;
        self.renderer.stop();
        self.reader.stop();
        self.restart(generation);
    }

    /// A new start of play: the lead is counted from the frontier, and what is queued downstream
    /// is to be dropped.
    fn restart(&mut self, generation: u32) {
        self.generation = generation;
        self.flush = Some(generation);
        self.played = self.renderer.frontier();
        self.reader.at(self.played);
    }

    /// The flush to send downstream before the next block, if a restart is waiting to be said.
    pub const fn take_flush(&mut self) -> Option<u32> {
        self.flush.take()
    }

    /// New casts from the page, by serial; a change of `pace` or `quiet` lets go of what is placed
    /// from the frontier on.
    pub fn cast(&mut self, casts: &BTreeMap<u32, Cast>, pace: Pace, quiet: bool) {
        let mut hands = Hands::new(&mut self.renderer, &mut self.events, self.generation);
        self.reader.recast(casts, pace, quiet, &mut hands);
        self.want_now();
    }

    /// A recorded line under render key `key`, its speech beginning `lead` into it. Taken whether
    /// or not a piece casts it yet: the page sends a piece's lines just before the piece, and the
    /// next piece handed over lets go of any no piece casts.
    ///
    /// # Errors
    ///
    /// More lines held than the synth holds.
    pub fn line(&mut self, key: &str, sample: Sample, lead: Seconds) -> Result<(), SynthError> {
        if self.reader.held.contains_key(key) {
            return Ok(());
        }
        #[allow(clippy::cast_precision_loss)] // a recording far shorter than 2^52 frames
        let seconds = sample.frames() as f64 / sample.rate();
        let id = if let Some(id) = self.free_lines.pop() {
            id
        } else {
            self.next_line += 1;
            self.next_line - 1
        };
        if let Err(error) = self.renderer.add_line(id, sample) {
            self.free_lines.push(id);
            return Err(error);
        }
        self.reader
            .held
            .insert(key.to_owned(), Held { id, lead, seconds });
        let mut hands = Hands::new(&mut self.renderer, &mut self.events, self.generation);
        self.reader.arrived(key, &mut hands);
        Ok(())
    }

    /// The browser's voice finished chain `chain`'s line, the playhead at output frame `end`.
    pub fn said(&mut self, chain: u32, end: u64) {
        let mut hands = Hands::new(&mut self.renderer, &mut self.events, self.generation);
        self.reader.said(chain, end, &mut hands);
    }

    /// The page could not render the line under `key`.
    pub fn failed(&mut self, key: &str) {
        let mut hands = Hands::new(&mut self.renderer, &mut self.events, self.generation);
        self.reader.failed(key, &mut hands);
    }

    /// The music's level under a line of the reading, from the frontier on.
    pub fn duck(&mut self, level: Gain) {
        self.renderer.set_duck(level);
    }

    /// The mixer has played up to output frame `frame` in start of play `generation`; what it says
    /// of an older one is stale.
    pub const fn played(&mut self, generation: u32, frame: u64) {
        if generation == self.generation {
            self.played = frame;
        }
    }

    /// Frames it keeps rendered ahead of the output.
    #[must_use]
    pub const fn lead(&self) -> u64 {
        self.lead
    }

    /// Keeps at least `frames` ahead from now on; a smaller number changes nothing. The app's output
    /// asks for buffers of its own size, which a phone may enlarge — to more than the lead — when
    /// its screen turns off (`docs/player.md` §13); a lead shorter than a buffer runs dry inside it.
    pub const fn widen(&mut self, frames: u64) {
        if frames > self.lead {
            self.lead = frames;
        }
    }

    /// Whether a block is due: playing, and less than the lead ahead of the output.
    #[must_use]
    pub const fn due(&self) -> bool {
        self.playing && self.renderer.frontier().saturating_sub(self.played) < self.lead
    }

    /// Renders the next block, the reading placed on it first, and gives the output frame it starts
    /// on and its stems.
    pub fn render(&mut self) -> (u64, &Stems) {
        let frame = self.renderer.frontier();
        let to = frame + u64::try_from(self.stems.frames()).unwrap_or(0);
        let mut hands = Hands::new(&mut self.renderer, &mut self.events, self.generation);
        self.reader.before(frame, to, &mut hands);
        self.renderer.render(&mut self.stems);
        self.reader.at(self.renderer.frontier());
        let losses = self.renderer.losses();
        if losses != self.told {
            self.told = losses;
            self.events.push(Event::Losses(losses));
        }
        (frame, &self.stems)
    }

    /// What it has to tell the page, oldest first.
    pub fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The page is told where a piece plays from.
    fn placed(&mut self, serial: u32, placement: Placement) {
        self.events.push(Event::Placement {
            serial,
            out_start: placement.out_start,
            piece_start: placement.piece_start,
        });
    }

    /// Lets go of the recorded lines no piece playing or to play casts, and asks the page for those
    /// it casts and the synth does not hold.
    fn want_now(&mut self) {
        let wanted = self.reader.wanted();
        let gone: Vec<String> = self
            .reader
            .held
            .keys()
            .filter(|key| !wanted.contains(*key))
            .cloned()
            .collect();
        for key in &gone {
            if let Some(held) = self.reader.held.remove(key) {
                self.renderer.drop_line(held.id);
                self.free_lines.push(held.id);
            }
        }
        if !gone.is_empty() {
            self.events.push(Event::Dropped(gone));
        }
        let missing = wanted
            .into_iter()
            .filter(|key| !self.reader.held.contains_key(key))
            .collect();
        self.events.push(Event::Want(missing));
    }
}

/// What of a handed piece the reader needs, once the synth has its notes.
struct Rest {
    serial: u32,
    /// Its bars in frames, from 0, and its length last.
    bars: Vec<u64>,
    cues: Vec<Cue>,
    cast: Cast,
}

impl Rest {
    fn segment(self, placement: Placement) -> Segment {
        Segment {
            serial: self.serial,
            out_start: placement.out_start,
            piece_start: placement.piece_start,
            bars: self.bars,
            cues: self.cues,
            cast: self.cast,
        }
    }
}

/// A handed piece as the synth's [`Piece`] and the rest, its bars on frames the same way the
/// synth's are: from 0, ascending, each once.
fn split(handed: Handed, rate: SampleRate) -> (Piece, Rest) {
    let length = handed.seconds.frames(rate);
    let mut bars: Vec<u64> = handed.bars.iter().map(|bar| bar.frames(rate)).collect();
    bars.push(0);
    bars.sort_unstable();
    bars.dedup();
    bars.retain(|bar| *bar < length.max(1));
    bars.push(length);
    let piece = Piece::new(
        handed.notes,
        &handed.bars,
        handed.seconds,
        Repeat::Loop,
        rate,
    );
    let rest = Rest {
        serial: handed.serial,
        bars,
        cues: handed.cues,
        cast: handed.cast,
    };
    (piece, rest)
}

/// The reader's hands: the synth's speech stem, and the page's ear.
struct Hands<'a> {
    renderer: &'a mut Renderer,
    events: &'a mut Vec<Event>,
    generation: u32,
}

impl<'a> Hands<'a> {
    const fn new(renderer: &'a mut Renderer, events: &'a mut Vec<Event>, generation: u32) -> Self {
        Self {
            renderer,
            events,
            generation,
        }
    }
}

impl Voice for Hands<'_> {
    fn speak(&mut self, id: u32, start: u64, speech: u64, hold: u64) -> Option<u64> {
        let utterance = Utterance {
            line: id,
            start,
            speech,
            hold,
        };
        match self.renderer.speak(utterance) {
            Ok(spoken) => Some(spoken.end),
            Err(error) => {
                self.events.push(Event::Error(format!(
                    "a line of the reading could not be placed: {error}"
                )));
                None
            }
        }
    }

    fn cut(&mut self, at: u64) {
        self.renderer.cut_speech(at);
    }

    fn tell(&mut self, told: Told) {
        self.events.push(Event::Reading {
            generation: self.generation,
            told,
        });
    }
}

#[cfg(test)]
mod tests;
