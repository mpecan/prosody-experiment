//! What the output does with the blocks the engine renders (`docs/player.md` §5): queue them, mix
//! each stem at its gain — mute and solo are a gain of 0, glided to over a few ms — through the
//! master stage, and count the times the queue ran dry.
//!
//! In the browser this is the audio worklet's job, in TypeScript (`web/src/player/mixer.ts`), since
//! a worklet cannot wait for a WebAssembly module to load; in the app it is this, called from the
//! audio callback. The two are the same mix, and their tests pin the same hand-worked numbers. The
//! master is `prosody_synth::master`'s chain: the stems at their gains, the music (every stem but
//! the reading's) at the block's duck and at the browser voice's, the glue compressor, the soft clip.
//!
//! Nothing here allocates once the queue has its room: it runs on the audio thread. A block the
//! output has finished is kept for the host to take back ([`Mixer::take_spent`]) and reuse, so it is
//! not freed there either.

use std::collections::VecDeque;

use prosody_synth::dynamics::{Compressor, Setting};
use prosody_synth::master::soft_clip;
use prosody_synth::{DUCK_FALL, DUCK_RISE, STEMS, Stems, math};

/// The reading's stem: the duck lowers the music under it, never it.
const SPEECH: usize = STEMS - 1;

/// Frames a stem's gain takes to go all the way from 1 to 0 or back: about 5 ms at 48 kHz. A gain
/// switched from one sample to the next is a step in a sounding wave — a click on every mute and
/// solo (`docs/player.md` §10).
pub const GLIDE: u32 = 256;

/// Frames the music still queued fades out over when it is dropped, at a stop or a restart from the
/// top: about 5 ms, for the same reason.
pub const FADE: usize = 256;

/// The most blocks the queue holds without growing: half a second at 2 048 frames is 12.
const ROOM: usize = 64;

/// A rendered block: from output frame `frame`, in start of play `generation`, the stems planar
/// (each stem's left side, then its right), then the duck ([`Stems`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The output frame it starts on.
    pub frame: u64,
    /// The start of play it belongs to.
    pub generation: u32,
    /// Its samples, [`Stems::PER_FRAME`] a frame.
    pub data: Vec<f32>,
}

impl Block {
    /// Its frames.
    #[must_use]
    pub const fn frames(&self) -> usize {
        self.data.len() / Stems::PER_FRAME
    }

    /// Sample `n` of side `side` (0 left, 1 right) of stem `stem`, or the duck at `n` for
    /// `stem == STEMS`.
    fn at(&self, stem: usize, side: usize, n: usize) -> f64 {
        let frames = self.frames();
        let index = (stem * 2 + side) * frames + n;
        self.data.get(index).map_or(0.0, |x| f64::from(*x))
    }

    /// The duck at frame `n`.
    fn duck(&self, n: usize) -> f64 {
        let index = 2 * STEMS * self.frames() + n;
        self.data.get(index).map_or(1.0, |x| f64::from(*x))
    }
}

/// How the output is doing, for the page and the engine.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Report {
    /// The output frame the next frame out plays: the playhead, in the engine's count.
    pub played: u64,
    /// Output buffers that wanted music and found the queue empty.
    pub underruns: u64,
    /// Frames queued ahead of the playhead.
    pub queued: u64,
    /// Whether anything has arrived since the last flush.
    pub started: bool,
    /// The start of play it is in.
    pub generation: u32,
    /// The loudest sample sent out since last asked: proof of sound.
    pub peak: f32,
    /// The loudest the reading's stem has been since last asked.
    pub speech_peak: f32,
}

/// The queue of blocks and the master stage's state.
#[derive(Debug, Clone)]
pub struct Mixer {
    blocks: VecDeque<Block>,
    spent: VecDeque<Block>,
    /// How far into the first block.
    offset: usize,
    played: u64,
    started: bool,
    underruns: u64,
    generation: u32,
    /// Each stem's gain now, gliding towards `targets`.
    gains: [f64; STEMS],
    targets: [f64; STEMS],
    /// Set when a new start of play arrives: its first frame takes the gains asked for as they are,
    /// since there is nothing sounding to glide from.
    fresh: bool,
    /// The fade-out of what was queued at the last flush, mixed (before the clip), and how far
    /// through it the output is.
    tail: Vec<(f64, f64)>,
    tail_frames: usize,
    tail_at: usize,
    glue: Compressor,
    /// The duck the page asks for while the browser's own voice reads (it cannot be in the block's
    /// lane), and the duck now, approached as the renderer approaches its own.
    outer_target: f64,
    outer: f64,
    fall: f64,
    rise: f64,
    peak: f32,
    speech_peak: f32,
}

impl Mixer {
    /// A mixer playing at `rate` frames a second.
    #[must_use]
    pub fn new(rate: f64) -> Self {
        Self {
            blocks: VecDeque::with_capacity(ROOM),
            spent: VecDeque::with_capacity(ROOM),
            offset: 0,
            played: 0,
            started: false,
            underruns: 0,
            generation: 0,
            gains: [1.0; STEMS],
            targets: [1.0; STEMS],
            fresh: true,
            tail: vec![(0.0, 0.0); FADE],
            tail_frames: 0,
            tail_at: 0,
            glue: Compressor::new(Setting::MASTER, rate),
            outer_target: 1.0,
            outer: 1.0,
            fall: math::exp(-1.0 / (DUCK_FALL * rate)),
            rise: math::exp(-1.0 / (DUCK_RISE * rate)),
            peak: 0.0,
            speech_peak: 0.0,
        }
    }

    /// The stems' gains to glide to.
    pub const fn set_gains(&mut self, gains: [f64; STEMS]) {
        self.targets = gains;
    }

    /// The music's level under a line of the browser's voice: 1 for none.
    pub const fn set_duck(&mut self, level: f64) {
        self.outer_target = level;
    }

    /// A block, in the order rendered; one of another start of play than the last flush's is
    /// stale, and goes back to the host unheard. The first after a flush says where the count
    /// starts.
    pub fn push(&mut self, block: Block) {
        if block.generation != self.generation {
            self.spent.push_back(block);
            return;
        }
        if !self.started && self.blocks.is_empty() {
            self.played = block.frame;
            self.offset = 0;
            self.fresh = true;
        }
        self.started = true;
        self.blocks.push_back(block);
    }

    /// Drops everything queued, for start of play `generation` — but not abruptly: the next
    /// [`FADE`] frames of it, at the gains they would have had, are kept as a tail that fades to
    /// nothing under whatever comes next. A tail still fading from a flush before goes on fading.
    pub fn flush(&mut self, generation: u32) {
        self.generation = generation;
        let mut frames = 0;
        for k in 0..FADE {
            // The tail before, still to be heard: read at or ahead of where this one is written,
            // so the two can share the buffer.
            let before = self.tail_at + k < self.tail_frames;
            let (mut l, mut r) = if before {
                self.tail
                    .get(self.tail_at + k)
                    .copied()
                    .unwrap_or((0.0, 0.0))
            } else {
                (0.0, 0.0)
            };
            let at = self.frame_at(k);
            if let Some((block, offset)) = at {
                #[allow(clippy::cast_precision_loss)] // at most FADE
                let fade = (FADE - k) as f64 / FADE as f64;
                let duck = block.duck(offset);
                for stem in 0..STEMS {
                    let ducked = if stem == SPEECH { 1.0 } else { duck };
                    let gain = self.gains.get(stem).copied().unwrap_or(1.0) * fade * ducked;
                    l += gain * block.at(stem, 0, offset);
                    r += gain * block.at(stem, 1, offset);
                }
            }
            if at.is_some() || before {
                frames = k + 1;
            }
            if let Some(slot) = self.tail.get_mut(k) {
                *slot = (l, r);
            }
        }
        self.tail_frames = frames;
        self.tail_at = 0;
        while let Some(block) = self.blocks.pop_front() {
            self.spent.push_back(block);
        }
        self.offset = 0;
        self.started = false;
    }

    /// The block and offset holding the `k`th frame from the playhead, if one is queued.
    fn frame_at(&self, k: usize) -> Option<(&Block, usize)> {
        let mut offset = self.offset + k;
        for block in &self.blocks {
            if offset < block.frames() {
                return Some((block, offset));
            }
            offset -= block.frames();
        }
        None
    }

    /// Frames queued ahead of the playhead.
    #[must_use]
    pub fn queued(&self) -> u64 {
        let frames: usize = self.blocks.iter().map(Block::frames).sum();
        u64::try_from(frames.saturating_sub(self.offset)).unwrap_or(0)
    }

    /// A block the output has finished with, for the host to reuse.
    pub fn take_spent(&mut self) -> Option<Block> {
        self.spent.pop_front()
    }

    /// How it is doing; the peaks start again from here.
    pub fn report(&mut self) -> Report {
        let report = Report {
            played: self.played,
            underruns: self.underruns,
            queued: self.queued(),
            started: self.started,
            generation: self.generation,
            peak: self.peak,
            speech_peak: self.speech_peak,
        };
        self.peak = 0.0;
        self.speech_peak = 0.0;
        report
    }

    /// Fills `out`, `channels` samples a frame, with the next frames: each stem at its gain
    /// (gliding to the gains asked for, a 1/[`GLIDE`] step a frame), the music at the block's duck
    /// and at the browser voice's, plus any tail fading from a flush, through the glue and the soft
    /// clip. Left and right go to the first two channels (one channel: their mean), silence to the
    /// rest. Frames the queue cannot fill are silence, and a buffer short of music counts one
    /// underrun.
    pub fn mix(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let mut short = false;
        for frame in out.chunks_mut(channels) {
            let (l, r) = self.next_frame(&mut short);
            #[allow(clippy::cast_possible_truncation)] // within ±0.95 after the clip
            let (l, r) = (l as f32, r as f32);
            self.peak = self.peak.max(l.abs()).max(r.abs());
            for (n, sample) in frame.iter_mut().enumerate() {
                *sample = match (n, channels) {
                    (0, 1) => f32::midpoint(l, r),
                    (0, _) => l,
                    (1, _) => r,
                    _ => 0.0,
                };
            }
        }
        if short && self.started {
            self.underruns += 1;
        }
    }

    /// The next frame out, left and right, after the master; `short` is set if the queue had none.
    fn next_frame(&mut self, short: &mut bool) -> (f64, f64) {
        let (mut l, mut r) = (0.0, 0.0);
        let pole = if self.outer_target < self.outer {
            self.fall
        } else {
            self.rise
        };
        self.outer = self.outer_target + (self.outer - self.outer_target) * pole;
        if let Some(block) = self.blocks.front() {
            let offset = self.offset;
            let (mut sl, mut sr) = (0.0, 0.0);
            for stem in 0..STEMS {
                let gain = glide(
                    &mut self.gains,
                    stem,
                    self.targets.get(stem).copied().unwrap_or(1.0),
                    self.fresh,
                );
                let (left, right) = (block.at(stem, 0, offset), block.at(stem, 1, offset));
                if stem == SPEECH {
                    sl += gain * left;
                    sr += gain * right;
                } else {
                    l += gain * left;
                    r += gain * right;
                }
            }
            let lane = block.duck(offset) * self.outer;
            l = l * lane + sl;
            r = r * lane + sr;
            #[allow(clippy::cast_possible_truncation)] // a level for the page
            let speech = sl.abs().max(sr.abs()) as f32;
            self.speech_peak = self.speech_peak.max(speech);
            self.fresh = false;
            self.offset += 1;
            self.played += 1;
            if self.offset >= block.frames()
                && let Some(done) = self.blocks.pop_front()
            {
                self.spent.push_back(done);
                self.offset = 0;
            }
        } else {
            *short = true;
        }
        if self.tail_at < self.tail_frames {
            let (tl, tr) = self.tail.get(self.tail_at).copied().unwrap_or((0.0, 0.0));
            l += tl;
            r += tr;
            self.tail_at += 1;
        }
        let glue = self.glue.gain(l.abs().max(r.abs()));
        (soft_clip(glue * l), soft_clip(glue * r))
    }
}

/// Stem `stem`'s gain for the next frame: a step of at most 1/[`GLIDE`] towards `target`, or the
/// target itself on the first frame of a start of play.
fn glide(gains: &mut [f64; STEMS], stem: usize, target: f64, fresh: bool) -> f64 {
    let Some(now) = gains.get_mut(stem) else {
        return target;
    };
    let most = 1.0 / f64::from(GLIDE);
    *now = if fresh {
        target
    } else if target > *now {
        target.min(*now + most)
    } else {
        target.max(*now - most)
    };
    *now
}

#[cfg(test)]
mod tests;
