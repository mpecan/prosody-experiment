//! Rendering a piece, block by block, and taking over from one piece to the next at a bar
//! (`docs/player.md` §5, "Live edits").
//!
//! Everything here counts in output frames — whole numbers — so where a note starts, where the
//! piece loops round and where an edit takes over are decided without a float (`CLAUDE.md` §4).
//! A note's time in seconds becomes a frame once, when the piece is made.

use crate::SynthError;
use crate::bus::Bus;
use crate::note::{Channel, Note};
use crate::sample::{Bank, Sample};
use crate::speech::{Speech, Spoken, Utterance};
use crate::stems::{STEMS, Stems};
use crate::units::{Gain, SampleRate, Seconds};
use crate::voice::{Pool, Unplayable, Voice};

/// Whether a piece loops round at its end, as Strudel's pattern does, or plays once (the CLI's
/// render).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    /// Round and round, as on the page.
    Loop,
    /// Once, as a file.
    Once,
}

/// A piece ready to render: its notes placed on frames, its bars, its length.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Sorted by frame, then by tag (the Performance's own order), so the order notes start in is
    /// total and the same every time.
    notes: Vec<(u64, Note)>,
    /// Where each bar starts, ascending, from 0.
    bars: Vec<u64>,
    length: u64,
    repeat: Repeat,
    /// The latest onset: how far back a loop's notes can reach into the next.
    latest: u64,
}

impl Piece {
    /// `notes` with `bars` starting where they say, `length` long, at `rate`. A looping piece of no
    /// length plays once: there is nothing to loop.
    #[must_use]
    pub fn new(
        notes: Vec<Note>,
        bars: &[Seconds],
        length: Seconds,
        repeat: Repeat,
        rate: SampleRate,
    ) -> Self {
        let mut placed: Vec<(u64, Note)> = notes
            .into_iter()
            .map(|note| (note.spec().onset.frames(rate), note))
            .collect();
        placed.sort_by_key(|(at, note)| (*at, note.spec().tag));
        let mut bars: Vec<u64> = bars.iter().map(|bar| bar.frames(rate)).collect();
        bars.push(0);
        bars.sort_unstable();
        bars.dedup();
        let length = length.frames(rate);
        let repeat = if length == 0 { Repeat::Once } else { repeat };
        let latest = placed.last().map_or(0, |(at, _)| *at);
        Self {
            notes: placed,
            bars,
            length,
            repeat,
            latest,
        }
    }

    /// How many notes it has.
    #[must_use]
    pub fn notes(&self) -> usize {
        self.notes.len()
    }

    /// Its length in frames: where it loops round.
    #[must_use]
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// The notes starting on piece frames `from..to`, as indices.
    fn starting(&self, from: u64, to: u64) -> std::ops::Range<usize> {
        let first = self.notes.partition_point(|(at, _)| *at < from);
        let end = self.notes.partition_point(|(at, _)| *at < to);
        first..end.max(first)
    }
}

/// A piece on the output's clock: from output frame `out_start` it plays from its own frame
/// `piece_start`.
#[derive(Debug, Clone, PartialEq)]
struct Segment {
    piece: Piece,
    out_start: u64,
    piece_start: u64,
}

impl Segment {
    /// Every note of this segment starting on output frames `from..to` (both at or after
    /// `out_start`), in order, appended to `starts` with `which`.
    fn collect(&self, from: u64, to: u64, which: Which, starts: &mut Vec<Start>) {
        if to <= from {
            return;
        }
        let p0 = self.piece_start + (from - self.out_start);
        let p1 = p0 + (to - from);
        let length = self.piece.length;
        let rounds = match self.piece.repeat {
            Repeat::Once => 0..=0,
            Repeat::Loop => p0.saturating_sub(self.piece.latest) / length..=(p1 - 1) / length,
        };
        for round in rounds {
            // A piece played once has one round, round 0.
            let base = round * length;
            let range = self
                .piece
                .starting(p0.saturating_sub(base), p1.saturating_sub(base));
            for index in range {
                if let Some((at, _)) = self.piece.notes.get(index) {
                    let out = self.out_start + (base + at - self.piece_start);
                    starts.push(Start {
                        at: out,
                        which,
                        index,
                    });
                }
            }
        }
    }
}

/// Which segment a note to start belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Which {
    Now,
    Next,
}

/// A note to start within the block being rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Start {
    at: u64,
    which: Which,
    index: usize,
}

/// Where a piece plays from on the output's clock: from output frame `out_start`, its own frame
/// `piece_start`. What the page maps its playhead through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// The output frame it takes over on.
    pub out_start: u64,
    /// Its own frame it starts from there.
    pub piece_start: u64,
}

/// What has gone missing from the sound, so it can be said (`CLAUDE.md` §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Losses {
    /// Voices cut off early because the pool was full.
    pub stolen: u64,
    /// Notes not played because their instrument was not loaded.
    pub missing: u64,
}

/// Renders pieces into stems, keeping the voices, the clock and what is lost.
#[derive(Debug, Clone)]
pub struct Renderer {
    rate: SampleRate,
    bank: Bank,
    pool: Pool,
    /// Each channel's bus, in stem order.
    buses: Vec<Bus>,
    /// The next output frame to render.
    frontier: u64,
    now: Option<Segment>,
    next: Option<Segment>,
    /// Reused each block, so rendering does not allocate once it has warmed up.
    starts: Vec<Start>,
    losses: Losses,
    /// The reading's lines and the duck (`docs/player.md` §5, "The reading").
    speech: Speech,
}

impl Renderer {
    /// A renderer at `rate` with the sounds in `bank`, playing nothing.
    #[must_use]
    pub fn new(rate: SampleRate, bank: Bank) -> Self {
        Self {
            rate,
            bank,
            pool: Pool::new(),
            buses: (0..STEMS)
                .map(|_| Bus::new(f64::from(rate.get())))
                .collect(),
            frontier: 0,
            now: None,
            next: None,
            starts: Vec::with_capacity(256),
            losses: Losses::default(),
            speech: Speech::new(rate),
        }
    }

    /// The sounds, to load more into.
    pub fn bank_mut(&mut self) -> &mut Bank {
        &mut self.bank
    }

    /// Its output rate.
    #[must_use]
    pub const fn rate(&self) -> SampleRate {
        self.rate
    }

    /// The next output frame it will render: how far it has got.
    #[must_use]
    pub const fn frontier(&self) -> u64 {
        self.frontier
    }

    /// What it has lost so far.
    #[must_use]
    pub const fn losses(&self) -> Losses {
        self.losses
    }

    /// How many voices are sounding.
    #[must_use]
    pub fn sounding(&self) -> usize {
        self.pool.sounding()
    }

    /// Plays `piece` from its top, from the frontier on: a new piece, or the other side of an A/B
    /// switch (`docs/player.md` §5: "presets and A/B switches still start from the top"). Notes
    /// already sounding ring on.
    pub fn play(&mut self, piece: Piece) -> Placement {
        let placement = Placement {
            out_start: self.frontier,
            piece_start: 0,
        };
        self.now = Some(Segment {
            piece,
            out_start: placement.out_start,
            piece_start: 0,
        });
        self.next = None;
        placement
    }

    /// Hands over to `piece` at the first bar boundary at or after the frontier — what is already
    /// rendered stays as it was — continuing from the same bar of the new piece (`docs/player.md`
    /// §5: position is kept by bar, not seconds, so an edit that changes the tempo carries on from
    /// the same place). A piece with fewer bars carries on from that bar counted round its own.
    /// With nothing playing, it plays from the top. A second edit before the first has taken over
    /// replaces it.
    pub fn swap(&mut self, piece: Piece) -> Placement {
        let Some(now) = &self.now else {
            return self.play(piece);
        };
        let at_piece = now.piece_start + (self.frontier - now.out_start);
        let looping = now.piece.repeat == Repeat::Loop;
        let (within, length) = if looping {
            (at_piece % now.piece.length, now.piece.length)
        } else {
            (at_piece, u64::MAX)
        };
        let bar = now
            .piece
            .bars
            .iter()
            .enumerate()
            .find(|(_, start)| **start >= within)
            .map(|(index, start)| (index, *start));
        let (index, boundary) = match bar {
            Some(found) => found,
            // Past the last bar's start: the next boundary is the loop's end, bar 0 again; a piece
            // played once has none, and hands over now.
            None if looping => (0, length),
            None => (0, within),
        };
        let out_start = self.frontier + (boundary - within);
        let piece_start = piece
            .bars
            .get(index % piece.bars.len().max(1))
            .copied()
            .unwrap_or(0);
        self.next = Some(Segment {
            piece,
            out_start,
            piece_start,
        });
        Placement {
            out_start,
            piece_start,
        }
    }

    /// Stops: every voice silenced, every line of the reading let go, nothing playing. The clock
    /// keeps counting.
    pub fn stop(&mut self) {
        self.now = None;
        self.next = None;
        self.pool.clear();
        for bus in &mut self.buses {
            bus.clear();
        }
        self.speech.clear();
    }

    /// Loads a recorded line of the reading as line `id`, in place of whatever had that number.
    /// Mono: a voice reads in the middle.
    ///
    /// # Errors
    ///
    /// [`SynthError::Line`] for a number past [`crate::MAX_LINES`].
    pub fn add_line(&mut self, id: u32, sample: Sample) -> Result<(), SynthError> {
        self.speech.add_line(id, sample)
    }

    /// Lets line `id` go. Where it is already placed, it still plays.
    pub fn drop_line(&mut self, id: u32) {
        self.speech.drop_line(id);
    }

    /// Places a line of the reading on the speech stem, and says where it sounds. A line placed to
    /// start before the frontier is heard from the frontier on, from as far into it as it has got.
    ///
    /// # Errors
    ///
    /// [`SynthError::Line`] for a line not loaded; [`SynthError::Speech`] for more than
    /// [`crate::MAX_PLACED`] lines placed and not finished, or speech that begins before its
    /// recording.
    pub fn speak(&mut self, utterance: Utterance) -> Result<Spoken, SynthError> {
        self.speech.speak(utterance)
    }

    /// Cuts the reading off at output frame `at`: a line sounding there fades out over
    /// [`crate::CUT_FADE`] frames, one placed to start at or after it is let go, and the music
    /// rises back from there (after [`crate::DUCK_RELEASE_DELAY`]), its hold not kept.
    pub fn cut_speech(&mut self, at: u64) {
        self.speech.cut(at);
    }

    /// The music's level under a line of the reading, from the frontier on: 1 for no duck.
    pub fn set_duck(&mut self, level: Gain) {
        self.speech.set_level(level);
    }

    /// Renders the next `stems.frames()` frames into `stems`, replacing what it held.
    pub fn render(&mut self, stems: &mut Stems) {
        stems.clear();
        for bus in &mut self.buses {
            bus.begin(stems.frames());
        }
        let from = self.frontier;
        let frames = u64::try_from(stems.frames()).unwrap_or(0);
        let to = from + frames;
        let mut starts = std::mem::take(&mut self.starts);
        starts.clear();
        let takeover = self.next.as_ref().map(|next| next.out_start);
        if let Some(now) = &self.now {
            let until = takeover.map_or(to, |at| at.clamp(from, to));
            now.collect(from.max(now.out_start), until, Which::Now, &mut starts);
        }
        if let Some(next) = &self.next {
            next.collect(next.out_start.max(from), to, Which::Next, &mut starts);
        }
        starts.sort_unstable();
        let mut cursor = from;
        for start in &starts {
            // The stretch up to the note is finished before the note can change its channel's bus,
            // so a change lands on the note's frame, not on the block's first.
            self.render_stretch(stems, from, cursor, start.at);
            cursor = start.at;
            self.begin(*start);
        }
        self.render_stretch(stems, from, cursor, to);
        self.speech.render(from, stems);
        self.starts = starts;
        if takeover.is_some_and(|at| at <= to) {
            self.now = self.next.take();
        }
        self.frontier = to;
    }

    /// Starts the note `start` names.
    fn begin(&mut self, start: Start) {
        let segment = match start.which {
            Which::Now => self.now.as_ref(),
            Which::Next => self.next.as_ref(),
        };
        let Some((_, note)) = segment.and_then(|segment| segment.piece.notes.get(start.index))
        else {
            return;
        };
        match Voice::new(note, &self.bank, self.rate, start.at) {
            Ok(voice) => {
                if let Some(bus) = self.buses.get_mut(note.spec().channel.index()) {
                    bus.configure(&note.spec().effects);
                }
                if self.pool.start(voice, &self.bank).is_some() {
                    self.losses.stolen += 1;
                }
            }
            Err(Unplayable::Missing) => self.losses.missing += 1,
        }
    }

    /// Renders output frames `from..to` of the block starting at `block` into `stems`: every
    /// voice into its bus, then every bus through its effects into its stem.
    fn render_stretch(&mut self, stems: &mut Stems, block: u64, from: u64, to: u64) {
        if to <= from {
            return;
        }
        let (Ok(offset), Ok(length)) = (usize::try_from(from - block), usize::try_from(to - from))
        else {
            return;
        };
        self.render_voices(offset, length);
        let end = offset + length;
        for (channel, bus) in Channel::ALL.iter().zip(&mut self.buses) {
            if let Some((left, right)) = stems.sides_mut(*channel)
                && let (Some(left), Some(right)) =
                    (left.get_mut(offset..end), right.get_mut(offset..end))
            {
                bus.finish(offset, left, right);
            }
        }
    }

    /// Renders every voice over `length` frames from frame `offset` of the block.
    fn render_voices(&mut self, offset: usize, length: usize) {
        for slot in self.pool.voices_mut() {
            let Some(voice) = slot else { continue };
            let Some(lanes) = self
                .buses
                .get_mut(voice.channel().index())
                .and_then(|bus| bus.slice(offset, length))
            else {
                continue;
            };
            if !voice.render(&self.bank, lanes) {
                *slot = None;
            }
        }
    }

    /// The notes the sounding voices play, by their tags (their indices in the Performance), in
    /// voice order: what every sound is attributed to (I4).
    #[must_use]
    pub fn sounding_tags(&self) -> Vec<u32> {
        self.pool.tags().collect()
    }
}
