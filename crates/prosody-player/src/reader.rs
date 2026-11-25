//! The reading, timed as the player renders ahead (`docs/player.md` §5, "The reading", and §12):
//! where each line of the poem is read on the music's clock.
//!
//! The cues come from the Performance: for each line, the bar it is read in at the fitted pace, and
//! the bar it may begin in at the natural pace, both in seconds from the piece's start. Before a
//! block is rendered the [`crate::Engine`] asks the reader which cues fall in it, and the reader
//! places each line on the synth's speech stem ([`Voice::speak`]) — a recorded line at its frame,
//! exactly — or, for a line the browser's own voice reads, tells the page when it is due, since
//! that voice cannot be placed in a stream; the page reads it off the playhead, unsynchronised.
//! Who reads each line, and with which recording, the page decides ([`Cast`]): it knows which
//! engines have loaded and which lines they have rendered.
//!
//! The natural pace's chaining is the page's from before the player (`web/src/reading`,
//! `naturalCue` and friends, `SPEC.md` §4.7), moved unchanged in what it decides: a line follows the
//! last by its pause; a stanza begins where its cue says the music has reached it, and under
//! `breath` not before a whole silent bar has passed since the last stanza's last line ended. It
//! needs each line's spoken length, which only the recorded line has — which is why it runs where
//! the line is placed. "Now" is the render frontier, the first frame not yet rendered, so a late
//! line starts there, never in what is already queued.
//!
//! A recorded line is placed when the block its recording begins in is about to be rendered, not as
//! soon as the chain knows when it falls: a stanza's held lines would otherwise all be placed in one
//! go, and a long stanza would overflow the synth's placements ([`prosody_synth::MAX_PLACED`]).
//! Placed that late it starts on the same frame, since nothing before that block is rendered
//! meanwhile.
//!
//! **Times are whole output frames.** Until step 5 this ran in the render worker as TypeScript, on
//! seconds compared with a tolerance; here every time the chain decides by is a frame, so whether a
//! line waits for the next bar is integer arithmetic (`CLAUDE.md` §4). A cue's time and a line's
//! pause become frames once, the nearest frame, as a note's onset does
//! ([`prosody_synth::Seconds::frames`]). The reading is the same in the browser and in the app
//! because it is this one module in both (`docs/player.md` §13).

use std::collections::{BTreeMap, BTreeSet};

use prosody_synth::Seconds;

/// Which pace the reading is read at (`SPEC.md` §4.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pace {
    /// A line a bar, each fitted to its bar.
    Fitted,
    /// A line after the last, at the voice's own speed.
    Natural,
}

impl Pace {
    /// Its name, as the page writes it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fitted => "fitted",
            Self::Natural => "natural",
        }
    }

    /// The pace named `name`, as the page writes it.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "fitted" => Some(Self::Fitted),
            "natural" => Some(Self::Natural),
            _ => None,
        }
    }
}

/// What a line waits for at the natural pace (`prosody_emit::reading::Onset`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Onset {
    /// Its bar, or the line before and its pause if they end later (`at`).
    At,
    /// Its bar, and a whole silent bar after the last stanza's last line (`breath`).
    Breath,
    /// The line before and its pause.
    Follows,
}

/// Where a line is read at the fitted pace: the bar (slot) and when it begins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    /// The slot of the arrangement it is read in.
    pub slot: u32,
    /// When that bar begins, from the piece's start.
    pub at: Seconds,
}

/// What a line waits for at the natural pace, and when its bar begins (none for a line that only
/// follows the one before).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Natural {
    /// What it waits for.
    pub onset: Onset,
    /// When the bar it may begin in begins, from the piece's start.
    pub at: Option<Seconds>,
}

/// A line of the reading on the piece's clock (`prosody_emit::perform::Cue`).
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    /// Which line of the poem, in reading order from 0.
    pub line: u32,
    /// Which stanza it is in.
    pub stanza: u32,
    /// The pause after it at the natural pace.
    pub pause: Seconds,
    /// Where the fitted pace reads it, if it does.
    pub fitted: Option<Fitted>,
    /// Where the natural pace may begin it, if it reads it.
    pub natural: Option<Natural>,
}

/// Who reads each line of a piece, by line number: the render key of the recording to place, or
/// `None` for the browser's own voice. A line it leaves out is not read.
pub type Cast = BTreeMap<u32, Option<String>>;

/// A recorded line the synth holds: its number there, the quiet margin before its speech, and its
/// length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
    /// Its number in the synth.
    pub id: u32,
    /// The quiet margin before its speech begins.
    pub lead: Seconds,
    /// Its length, seconds.
    pub seconds: f64,
}

/// What the reader tells the page. Frames are output frames; `serial` names the piece.
#[derive(Debug, Clone, PartialEq)]
pub enum Told {
    /// A recorded line placed: from `start` (where its speech begins) to `end`; `over` is how far
    /// its speech runs past its bar (fitted), `late` how far after it was due it starts (natural).
    Spoken {
        /// The piece.
        serial: u32,
        /// The line.
        line: u32,
        /// The pace read.
        pace: Pace,
        /// Where its speech begins.
        start: u64,
        /// The frame after its last.
        end: u64,
        /// Seconds its speech runs past its bar; `None` where it has no bar after it.
        over: Option<f64>,
        /// Seconds after it was due.
        late: f64,
    },
    /// A line for the browser's voice, due at frame `at` — or one whose recording is not there
    /// (`late`). `chain` names the natural pace's chain the page answers ([`Reader::said`]); 0 for
    /// the fitted pace, which waits for no answer.
    Say {
        /// The piece.
        serial: u32,
        /// The line.
        line: u32,
        /// The pace read.
        pace: Pace,
        /// When it is due.
        at: u64,
        /// Whether a recording was cast and is not there.
        late: bool,
        /// The chain waiting for it to end.
        chain: u32,
    },
    /// Every line cut off at frame `at`.
    Cut {
        /// Where.
        at: u64,
    },
    /// The chain waits for a recording the page has not sent.
    Wait {
        /// The piece.
        serial: u32,
        /// The line.
        line: u32,
        /// Its render key.
        key: String,
    },
    /// Something the reading lost or could not do, to be said (`CLAUDE.md` §6); `loss` keeps it in
    /// view beside the next note.
    Note {
        /// What.
        text: String,
        /// Whether a line went unread.
        loss: bool,
    },
    /// Where the reading is: the line it will want next, or `None` once the round's are read.
    At {
        /// The piece.
        serial: u32,
        /// The pace read.
        pace: Pace,
        /// The next line.
        line: Option<u32>,
    },
}

/// What the reader drives: the synth's speech stem, and the page.
pub trait Voice {
    /// Places held line `id`, its recording from frame `start`, its speech from `speech`, the music
    /// kept down `hold` frames after it; gives the frame after its last, or `None` if the synth
    /// refused it (and said why) — a line that is not heard is not told as spoken.
    fn speak(&mut self, id: u32, start: u64, speech: u64, hold: u64) -> Option<u64>;
    /// Cuts every line off at frame `at`.
    fn cut(&mut self, at: u64);
    /// Tells the page.
    fn tell(&mut self, told: Told);
}

/// A piece on the output's clock: from output frame `out_start` it plays from its own
/// `piece_start`.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    /// Which piece.
    pub serial: u32,
    /// The output frame it takes over on.
    pub out_start: u64,
    /// Its own frame it plays from there.
    pub piece_start: u64,
    /// Where each of its bars begins, in its own frames, ascending, and its length last: where it
    /// loops round.
    pub bars: Vec<u64>,
    /// Its reading's cues.
    pub cues: Vec<Cue>,
    /// Who reads them.
    pub cast: Cast,
}

impl Segment {
    /// Its length in frames.
    fn length(&self) -> u64 {
        self.bars.last().copied().unwrap_or(0)
    }
}

/// What the natural pace's chain is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Busy {
    /// Nothing: it reads on.
    No,
    /// The browser's voice to finish a line.
    Said,
    /// A recording to arrive, by render key.
    Key(String),
}

/// The natural pace's chain: which line is next, and what it waits for. Times are output frames,
/// signed, since a round's origin can fall before frame 0 when a piece is joined part-way.
#[derive(Debug, Clone, PartialEq)]
struct Chain {
    /// Its number, which the page answers [`Told::Say`] with.
    serial: u32,
    /// The piece it reads.
    segment: u32,
    /// Its piece's bars (and length last), in frames.
    bars: Vec<i64>,
    /// The cues the natural pace reads, by index into the piece's, in reading order.
    lines: Vec<usize>,
    next: usize,
    round: Option<i64>,
    wrap_to: Option<usize>,
    /// The output frame this round's bar 1 began.
    origin: i64,
    /// Index into `lines` → the output frame its cue fired, this round.
    anchors: BTreeMap<usize, i64>,
    prev_end: Option<i64>,
    prev_pause: i64,
    busy: Busy,
}

impl Chain {
    /// Bar `k` of the looping piece (negative or past the end: the rounds before and after) on the
    /// output's clock.
    fn bar_time(&self, k: i64) -> i64 {
        let (slots, length) = self.shape();
        let round = k.div_euclid(slots);
        let within = usize::try_from(k - round * slots).unwrap_or(0);
        self.origin + round * length + self.bars.get(within).copied().unwrap_or(0)
    }

    /// The bar playing at output frame `t`.
    fn bar_at(&self, t: i64) -> i64 {
        let (slots, length) = self.shape();
        let round = (t - self.origin).div_euclid(length);
        let within = t - self.origin - round * length;
        let mut k = 0;
        while k + 1 < slots
            && usize::try_from(k + 1)
                .ok()
                .and_then(|next| self.bars.get(next))
                .is_some_and(|start| *start <= within)
        {
            k += 1;
        }
        k + round * slots
    }

    /// Its bars and its length, each at least 1 so neither divides by zero: a chain is made only for
    /// a piece with a length (`Reader::reach`), and a piece with no bars is one bar long.
    fn shape(&self) -> (i64, i64) {
        let slots = i64::try_from(self.bars.len().saturating_sub(1)).unwrap_or(1);
        let length = self.bars.last().copied().unwrap_or(1);
        (slots.max(1), length.max(1))
    }

    /// The output frame this round's piece ends on.
    fn piece_end(&self) -> i64 {
        self.origin + self.bars.last().copied().unwrap_or(0)
    }
}

/// A cue reached in a block: on output frame `out`, in round `round` of its piece.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Reached {
    out: i64,
    line: u32,
    round: i64,
    cue: usize,
}

/// The reading's timing: the pieces playing and to play, and the natural pace's chain.
#[derive(Debug, Clone, PartialEq)]
pub struct Reader {
    /// Frames a second.
    rate: u32,
    /// The pieces playing and to play: the first is heard, a second takes over at its `out_start`.
    segments: Vec<Segment>,
    chain: Option<Chain>,
    chains: u32,
    /// Where the old piece's last line ended, and its pause, carried into the new piece's chain at
    /// a hand-over: a stanza of the new piece still waits its silent bar after the old one's last
    /// line.
    carry: Option<(i64, i64)>,
    /// The pace read.
    pub pace: Pace,
    /// The recorded lines the synth holds, by render key.
    pub held: BTreeMap<String, Held>,
    /// The first frame not yet rendered: what "now" is.
    frontier: u64,
    /// While a block is being prepared ([`Reader::before`]), the frame after it: a line whose
    /// recording begins before this is placed now, a later one waits for its own block.
    until: Option<u64>,
    /// The chain stopped at a held line whose block has not come yet: `before` resumes it.
    deferred: bool,
}

impl Reader {
    /// A reader on an output of `rate` frames a second, nothing playing.
    #[must_use]
    pub const fn new(rate: u32) -> Self {
        Self {
            rate,
            segments: Vec::new(),
            chain: None,
            chains: 0,
            carry: None,
            pace: Pace::Natural,
            held: BTreeMap::new(),
            frontier: 0,
            until: None,
            deferred: false,
        }
    }

    /// `seconds` as the nearest frame, as the synth turns a note's time into one.
    fn frames(&self, seconds: Seconds) -> i64 {
        let frames = (seconds.get() * f64::from(self.rate)).round();
        #[allow(clippy::cast_possible_truncation)] // a time within a piece, far below 2^63 frames
        let whole = frames as i64;
        whole
    }

    /// Frames as seconds, for what the page is told.
    fn seconds(&self, frames: i64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // far below 2^52 frames
        let seconds = frames as f64 / f64::from(self.rate);
        seconds
    }

    /// A piece from its top, from the frontier: nothing before it is read on.
    pub fn play(&mut self, segment: Segment) {
        self.segments = vec![segment];
        self.drop_chain();
        self.carry = None;
    }

    /// A piece taking over at its `out_start`: a second hand-over before the first has happened
    /// replaces it, as the synth's does.
    pub fn swap(&mut self, segment: Segment) {
        self.segments.truncate(1);
        self.segments.push(segment);
    }

    /// Nothing playing.
    pub fn stop(&mut self) {
        self.segments.clear();
        self.drop_chain();
        self.carry = None;
    }

    fn drop_chain(&mut self) {
        self.chain = None;
        self.deferred = false;
    }

    /// The pieces playing and to play, by serial: whose casts the page keeps up to date.
    #[must_use]
    pub fn serials(&self) -> Vec<u32> {
        self.segments.iter().map(|segment| segment.serial).collect()
    }

    /// Every render key some piece playing or to play may place.
    #[must_use]
    pub fn wanted(&self) -> BTreeSet<String> {
        self.segments
            .iter()
            .flat_map(|segment| segment.cast.values().flatten().cloned())
            .collect()
    }

    /// New casts from the page, by serial. A change of pace, or `quiet` (a change of voice, engine
    /// or language), lets go of everything placed from the frontier on, and the reading picks up at
    /// the next cue.
    pub fn recast(
        &mut self,
        casts: &BTreeMap<u32, Cast>,
        pace: Pace,
        quiet: bool,
        voice: &mut dyn Voice,
    ) {
        for segment in &mut self.segments {
            if let Some(cast) = casts.get(&segment.serial) {
                segment.cast.clone_from(cast);
            }
        }
        if quiet || pace != self.pace {
            self.pace = pace;
            cut_at(voice, self.frontier);
            self.drop_chain();
            self.carry = None;
            return;
        }
        // A line waited for that its piece no longer casts with that key (dropped: the engine or
        // voice changed) is asked for again from whoever reads it now.
        let wanted = self.wanted();
        let reask = self
            .chain
            .as_ref()
            .is_some_and(|chain| matches!(&chain.busy, Busy::Key(key) if !wanted.contains(key)));
        if reask {
            self.step(|chain| chain.busy = Busy::No);
            self.advance(voice);
        }
    }

    /// A recorded line the synth now holds: a line waiting for it is read.
    pub fn arrived(&mut self, key: &str, voice: &mut dyn Voice) {
        let waiting = self
            .chain
            .as_ref()
            .is_some_and(|chain| matches!(&chain.busy, Busy::Key(waited) if waited == key));
        if waiting {
            self.step(|chain| chain.busy = Busy::No);
            self.advance(voice);
        }
    }

    /// A line the page could not render: the chain goes on without it.
    pub fn failed(&mut self, key: &str, voice: &mut dyn Voice) {
        let Some(pause) =
            self.waiting_pause(|busy| matches!(busy, Busy::Key(waited) if waited == key))
        else {
            return;
        };
        self.step(|chain| {
            chain.prev_pause = pause;
            chain.next += 1;
            chain.busy = Busy::No;
        });
        self.advance(voice);
    }

    /// The browser's voice finished chain `chain`'s current line, as the playhead stood at `end`.
    pub fn said(&mut self, chain: u32, end: u64, voice: &mut dyn Voice) {
        let ours = self.chain.as_ref().is_some_and(|c| c.serial == chain);
        let Some(pause) = self
            .waiting_pause(|busy| *busy == Busy::Said)
            .filter(|_| ours)
        else {
            return;
        };
        self.step(|c| {
            c.prev_end = Some(i64::try_from(end).unwrap_or(i64::MAX));
            c.prev_pause = pause;
            c.next += 1;
            c.busy = Busy::No;
        });
        self.advance(voice);
    }

    /// The pause after the chain's current line, in frames, if the chain is waiting as `waiting`
    /// says.
    fn waiting_pause(&self, waiting: impl Fn(&Busy) -> bool) -> Option<i64> {
        let chain = self.chain.as_ref().filter(|chain| waiting(&chain.busy))?;
        let segment = self.segment_of(chain)?;
        let pause = chain
            .lines
            .get(chain.next)
            .and_then(|index| segment.cues.get(*index))
            .map_or(0, |cue| self.frames(cue.pause));
        Some(pause)
    }

    /// The piece a chain reads.
    fn segment_of(&self, chain: &Chain) -> Option<&Segment> {
        self.segments
            .first()
            .filter(|segment| segment.serial == chain.segment)
    }

    /// Moves "now" to `frontier`, as the engine renders.
    pub const fn at(&mut self, frontier: u64) {
        self.frontier = frontier;
    }

    /// Before output frames `from..to` are rendered: every cue in them, in order, on the piece that
    /// plays there — the old one up to a hand-over, and the new one from it, where the old reading
    /// is cut off (its chain was the old piece's).
    pub fn before(&mut self, from: u64, to: u64, voice: &mut dyn Voice) {
        self.frontier = from;
        let takeover = self
            .segments
            .get(1)
            .map(|next| next.out_start)
            .filter(|at| *at < to);
        if self.segments.is_empty() {
            return;
        }
        if let Some(out_start) = takeover {
            let at = from.max(out_start);
            self.block(from, at, voice);
            cut_at(voice, at);
            let at_frame = i64::try_from(at).unwrap_or(i64::MAX);
            self.carry = self
                .chain
                .as_ref()
                .and_then(|old| old.prev_end.map(|end| (end.min(at_frame), old.prev_pause)));
            self.drop_chain();
            self.segments.remove(0);
            self.block(at, to, voice);
        } else {
            self.block(from, to, voice);
        }
        self.until = None;
    }

    /// Output frames `from..to` of the piece heard: the lines the chain held back for them placed
    /// first (each falls before any cue reached here, so a round's lines are read before the seam),
    /// then the cues in them.
    fn block(&mut self, from: u64, to: u64, voice: &mut dyn Voice) {
        self.until = Some(to);
        if self.deferred {
            self.deferred = false;
            self.advance(voice);
        }
        self.reach(from, to, voice);
    }

    /// Every cue of the pace read, of the piece heard, that falls on output frames `from..to`.
    fn reach(&mut self, from: u64, to: u64, voice: &mut dyn Voice) {
        let Some(segment) = self.segments.first() else {
            return;
        };
        let length = i64::try_from(segment.length()).unwrap_or(0);
        if to <= from || length <= 0 {
            return;
        }
        let (Ok(out_start), Ok(piece_start), Ok(from), Ok(to)) = (
            i64::try_from(segment.out_start),
            i64::try_from(segment.piece_start),
            i64::try_from(from),
            i64::try_from(to),
        ) else {
            return;
        };
        let p0 = piece_start + (from - out_start);
        let p1 = p0 + (to - from);
        let mut reached = Vec::new();
        for (index, cue) in segment.cues.iter().enumerate() {
            let at = match self.pace {
                Pace::Fitted => cue.fitted.map(|fitted| fitted.at),
                Pace::Natural => cue.natural.and_then(|natural| natural.at),
            };
            let Some(at) = at else { continue };
            let frame = self.frames(at);
            let mut round = (p0 - frame).div_euclid(length);
            while round * length + frame < p1 {
                let position = round * length + frame;
                if position >= p0 {
                    reached.push(Reached {
                        out: out_start + position - piece_start,
                        line: cue.line,
                        round,
                        cue: index,
                    });
                }
                round += 1;
            }
        }
        reached.sort_unstable();
        for Reached {
            out, round, cue, ..
        } in reached
        {
            match self.pace {
                Pace::Fitted => self.fitted(cue, out, voice),
                Pace::Natural => self.natural(cue, out, round, voice),
            }
        }
    }

    // ---- The fitted pace: a line a bar ----

    fn fitted(&self, index: usize, out: i64, voice: &mut dyn Voice) {
        let Some(segment) = self.segments.first() else {
            return;
        };
        let Some(cue) = segment.cues.get(index) else {
            return;
        };
        let out = u64::try_from(out).unwrap_or(0);
        // A line still sounding when the next is due is cut off at the bar.
        cut_at(voice, out);
        let after = segment
            .cues
            .iter()
            .skip(index + 1)
            .find(|cue| cue.fitted.is_some());
        voice.tell(Told::At {
            serial: segment.serial,
            pace: Pace::Fitted,
            line: after.map(|cue| cue.line),
        });
        // A line the cast leaves out is not read (a reading let go: a preset in another language).
        let Some(key) = segment.cast.get(&cue.line) else {
            return;
        };
        let Some(held) = key.as_ref().and_then(|key| self.held.get(key)) else {
            voice.tell(Told::Say {
                serial: segment.serial,
                line: cue.line,
                pace: Pace::Fitted,
                at: out,
                late: key.is_some(),
                chain: 0,
            });
            return;
        };
        let lead = u64::try_from(self.frames(held.lead)).unwrap_or(0);
        let start = out.saturating_sub(lead);
        let Some(end) = voice.speak(held.id, start, out, 0) else {
            return;
        };
        let slot = cue
            .fitted
            .and_then(|fitted| usize::try_from(fitted.slot).ok());
        let bar = slot.and_then(|slot| {
            let (begin, next) = (segment.bars.get(slot)?, segment.bars.get(slot + 1)?);
            // The piece's length is the last entry, not a bar: a line in the last bar has the loop's
            // end after it.
            Some(next.saturating_sub(*begin))
        });
        let over = bar.map(|bar| {
            held.seconds - 2.0 * held.lead.get() - self.seconds(i64::try_from(bar).unwrap_or(0))
        });
        voice.tell(Told::Spoken {
            serial: segment.serial,
            line: cue.line,
            pace: Pace::Fitted,
            start: out,
            end,
            over,
            late: 0.0,
        });
    }

    // ---- The natural pace: a line after the last ----

    fn natural(&mut self, index: usize, out: i64, round: i64, voice: &mut dyn Voice) {
        let Some(segment) = self.segments.first() else {
            return;
        };
        let Some(cue) = segment.cues.get(index) else {
            return;
        };
        let Some(natural) = cue.natural else { return };
        if self
            .chain
            .as_ref()
            .is_none_or(|chain| chain.segment != segment.serial)
        {
            let chain = self.new_chain();
            self.chain = chain;
        }
        let slot_at = natural.at.map_or(0, |at| self.frames(at));
        let notes = self.chain.as_mut().and_then(|c| {
            let position = c.lines.iter().position(|line| *line == index)?;
            c.origin = out - slot_at;
            let mut notes = None;
            if Some(round) != c.round {
                // The piece has come round: this round's cues begin afresh, from this one. While a
                // line is still being read (the browser's voice until it ends, a line until it is
                // rendered) the move waits for it to finish (`wrap_to`, taken in `advance`), or its
                // step on would pass the last line and leave this round unread. The last line's end
                // is kept, so a first stanza still waits for its breath after a last one that ran
                // over the loop's seam.
                if c.round.is_some() && c.busy == Busy::No {
                    notes = Some(position);
                } else if c.round.is_some() {
                    c.wrap_to = Some(position);
                } else {
                    c.next = position;
                }
                c.round = Some(round);
                c.anchors.clear();
            }
            c.anchors.insert(position, out);
            Some((position, notes))
        });
        let Some((position, wrap)) = notes else {
            return;
        };
        if let Some(position) = wrap {
            self.wrap_round(position, voice);
        }
        // Nothing being read, and the line waited for is not this one: either the clock was joined
        // part-way through (a hand-over) or the reading had finished this round. Start here.
        if let Some(c) = &mut self.chain
            && c.busy == Busy::No
            && c.next != position
            && !c.anchors.contains_key(&c.next)
        {
            c.next = position;
        }
        self.advance(voice);
    }

    fn new_chain(&mut self) -> Option<Chain> {
        let segment = self.segments.first()?;
        let carried = self.carry.take();
        self.chains += 1;
        Some(Chain {
            serial: self.chains,
            segment: segment.serial,
            bars: segment
                .bars
                .iter()
                .map(|bar| i64::try_from(*bar).unwrap_or(i64::MAX))
                .collect(),
            lines: segment
                .cues
                .iter()
                .enumerate()
                .filter(|(_, cue)| cue.natural.is_some())
                .map(|(index, _)| index)
                .collect(),
            next: 0,
            round: None,
            wrap_to: None,
            origin: 0,
            anchors: BTreeMap::new(),
            prev_end: carried.map(|(end, _)| end),
            prev_pause: carried.map_or(0, |(_, pause)| pause),
            busy: Busy::No,
        })
    }

    /// The chain moves to the new round's line `position`. A line of the old round not yet read
    /// will not be — its round's cues are gone — and is said, not dropped in silence (`CLAUDE.md`
    /// §6).
    fn wrap_round(&mut self, position: usize, voice: &mut dyn Voice) {
        let lost = self.lines_from(|_| true);
        if let Some(c) = &mut self.chain {
            if !lost.is_empty() {
                voice.tell(Told::Note {
                    text: format!(
                        "not read last time round: line {}, the piece came round before its turn",
                        numbers(&lost)
                    ),
                    loss: true,
                });
            }
            c.next = position;
            c.wrap_to = None;
        }
    }

    /// The lines of the chain from its next on that `keep` keeps, as the page numbers them (from
    /// 1).
    fn lines_from(&self, keep: impl Fn(&Cue) -> bool) -> Vec<u32> {
        let Some(chain) = &self.chain else {
            return Vec::new();
        };
        let Some(segment) = self.segment_of(chain) else {
            return Vec::new();
        };
        chain
            .lines
            .iter()
            .skip(chain.next)
            .filter_map(|index| segment.cues.get(*index))
            .filter(|cue| keep(cue))
            .map(|cue| cue.line + 1)
            .collect()
    }

    /// Reads on as far as it can, then says where the reading is.
    fn advance(&mut self, voice: &mut dyn Voice) {
        if self.chain.is_none() {
            return;
        }
        self.advance_chain(voice);
        let Some(chain) = &self.chain else { return };
        let Some(segment) = self.segment_of(chain) else {
            return;
        };
        let line = chain
            .lines
            .get(chain.next)
            .and_then(|index| segment.cues.get(*index))
            .map(|cue| cue.line);
        voice.tell(Told::At {
            serial: segment.serial,
            pace: Pace::Natural,
            line,
        });
    }

    fn advance_chain(&mut self, voice: &mut dyn Voice) {
        match &self.chain {
            Some(chain) if chain.busy == Busy::No => {}
            _ => return,
        }
        if let Some(position) = self.chain.as_ref().and_then(|chain| chain.wrap_to) {
            self.wrap_round(position, voice);
        }
        loop {
            let Some((c, segment)) = self
                .chain
                .as_ref()
                .and_then(|c| Some((c, self.segment_of(c)?)))
            else {
                return;
            };
            let Some(cue) = c
                .lines
                .get(c.next)
                .and_then(|index| segment.cues.get(*index))
            else {
                return;
            };
            let onset = cue.natural.map_or(Onset::Follows, |natural| natural.onset);
            let mut earliest = None;
            if onset == Onset::Follows {
                if c.prev_end.is_none() {
                    // Joined in the middle of a stanza: there is no line before to follow, so this
                    // one waits for the piece to come round, as a line before the clock's bar does
                    // at the fitted pace.
                    self.step(|c| c.next += 1);
                    continue;
                }
            } else if let Some(anchor) = c.anchors.get(&c.next) {
                earliest = Some(*anchor);
            } else {
                // Waiting for its bar. If a later line's cue has already fired this round, this
                // one's will not (the clock was joined after it): move on to that one.
                let next = c.next;
                let Some(after) = c.anchors.keys().find(|k| **k > next).copied() else {
                    return;
                };
                self.step(|c| c.next = after);
                continue;
            }
            if let Some(prev_end) = c.prev_end {
                let mut e = earliest.map_or(prev_end + c.prev_pause, |anchor: i64| {
                    anchor.max(prev_end + c.prev_pause)
                });
                if onset == Onset::Breath {
                    // On a bar, and at least two bars after the one the last line ended in, so one
                    // whole bar between is silent: the first bar from there that begins no earlier
                    // than `earliest`.
                    let mut bar = (c.bar_at(prev_end) + 2).max(c.bar_at(e));
                    if c.bar_time(bar) < e {
                        bar += 1;
                    }
                    e = c.bar_time(bar);
                }
                earliest = Some(e);
            }
            let Some(earliest) = earliest else { return };
            if earliest >= c.piece_end() {
                let stanza = cue.stanza;
                let lost = self.lines_from(|cue| cue.stanza >= stanza);
                voice.tell(Told::Note {
                    text: format!(
                        "not read this time round: line {}, the piece ends before its turn",
                        numbers(&lost)
                    ),
                    loss: false,
                });
                self.step(|c| c.next = c.lines.len());
                return;
            }
            if !self.speak_natural(earliest, voice) {
                return;
            }
            if self
                .chain
                .as_ref()
                .is_none_or(|chain| chain.busy != Busy::No)
            {
                return;
            }
        }
    }

    /// Changes the chain.
    fn step(&mut self, change: impl FnOnce(&mut Chain)) {
        if let Some(chain) = &mut self.chain {
            change(chain);
        }
    }

    /// Reads the chain's next line from `earliest`, or waits for it; false when its block has not
    /// come yet and it is left for `before` to place.
    fn speak_natural(&mut self, earliest: i64, voice: &mut dyn Voice) -> bool {
        let Some((c, segment)) = self
            .chain
            .as_ref()
            .and_then(|c| Some((c, self.segment_of(c)?)))
        else {
            return false;
        };
        let Some(line) = c
            .lines
            .get(c.next)
            .and_then(|index| segment.cues.get(*index))
        else {
            return false;
        };
        let now = i64::try_from(self.frontier).unwrap_or(i64::MAX);
        let last = c
            .lines
            .get(c.next + 1)
            .and_then(|index| segment.cues.get(*index))
            .is_none_or(|next| next.stanza != line.stanza);
        let serial = segment.serial;
        let chain_serial = c.serial;
        let (number, pause) = (line.line, self.frames(line.pause));
        // A line the cast leaves out is not read, and the next does not wait for it.
        let Some(key) = segment.cast.get(&number).cloned() else {
            self.step(|c| c.next += 1);
            return true;
        };
        let Some(key) = key else {
            // Not placed in the stream, so not held to the frontier: the page speaks it when its
            // playhead reaches the time it is due, or at once if that has passed.
            self.step(|c| c.busy = Busy::Said);
            voice.tell(Told::Say {
                serial,
                line: number,
                pace: Pace::Natural,
                at: u64::try_from(earliest.max(0)).unwrap_or(0),
                late: false,
                chain: chain_serial,
            });
            return true;
        };
        let Some(held) = self.held.get(&key).copied() else {
            voice.tell(Told::Wait {
                serial,
                line: number,
                key: key.clone(),
            });
            self.step(|c| c.busy = Busy::Key(key));
            return true;
        };
        let start = earliest.max(now);
        let begins = (start - self.frames(held.lead)).max(0);
        let until = i64::try_from(self.until.unwrap_or(self.frontier)).unwrap_or(i64::MAX);
        if begins >= until {
            self.deferred = true;
            return false;
        }
        let kept = if last { 0 } else { pause.max(0) };
        let spoken = voice.speak(
            held.id,
            u64::try_from(begins).unwrap_or(0),
            u64::try_from(start).unwrap_or(0),
            u64::try_from(kept).unwrap_or(0),
        );
        self.step(|c| {
            c.prev_pause = pause;
            c.next += 1;
        });
        // Refused (the engine has said why): not heard, so not told as spoken, and the next line
        // follows the last one heard by this one's pause, as after a line that could not be
        // rendered.
        let Some(end) = spoken else { return true };
        voice.tell(Told::Spoken {
            serial,
            line: number,
            pace: Pace::Natural,
            start: u64::try_from(start).unwrap_or(0),
            end,
            over: Some(0.0),
            late: self.seconds(start - earliest),
        });
        self.step(|c| c.prev_end = Some(i64::try_from(end).unwrap_or(i64::MAX)));
        true
    }
}

/// Cuts the reading at `at`, and says so.
fn cut_at(voice: &mut dyn Voice, at: u64) {
    voice.cut(at);
    voice.tell(Told::Cut { at });
}

/// Line numbers as the page lists them: "2, 3".
fn numbers(lines: &[u32]) -> String {
    lines
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
