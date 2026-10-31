//! A channel's bus (`docs/player.md` §4): where its voices add up, through its compressor, with its
//! echo and its reverb, into its stem.
//!
//! Strudel compresses each note's own chain and sends that to a reverb and an echo shared by every
//! layer. Here the compressor is the channel's — one detector on the sum — but it is driven by the
//! voices' sound *before* their `postgain`, and its gain is applied to their sound *after* it and to
//! what they send, which is where Strudel's per-note compressor sits (measured: a postgain of ½ on
//! a compressed note halves its compressed output, `docs/player.md` §11). With one note sounding the
//! two are the same; with several, the loudest sets the reduction for all. The echo and the reverb
//! are linear, so one per channel sounds as one shared, and it lets mute and solo silence a
//! channel's tail too.

use crate::delay::EchoLine;
use crate::dynamics::{Compressor, Setting};
use crate::effects::Effects;
use crate::reverb::Reverb;

/// The sound the voices of one channel leave for the bus, over one block: each a left and a right
/// side.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Lanes {
    /// Before `postgain`: what the compressor listens to.
    pub(crate) pre: [Vec<f32>; 2],
    /// After `postgain`: what is heard.
    pub(crate) post: [Vec<f32>; 2],
    /// Sent to the reverb.
    pub(crate) room: [Vec<f32>; 2],
    /// Sent to the echo.
    pub(crate) echo: [Vec<f32>; 2],
}

impl Lanes {
    fn resize(&mut self, frames: usize) {
        for side in self
            .pre
            .iter_mut()
            .chain(&mut self.post)
            .chain(&mut self.room)
            .chain(&mut self.echo)
        {
            side.clear();
            side.resize(frames, 0.0);
        }
    }
}

/// The part of the lanes a voice writes into: frames `from..from + length` of each.
pub(crate) struct Slice<'a> {
    pub(crate) pre: [&'a mut [f32]; 2],
    pub(crate) post: [&'a mut [f32]; 2],
    pub(crate) room: [&'a mut [f32]; 2],
    pub(crate) echo: [&'a mut [f32]; 2],
}

/// A channel's bus.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Bus {
    rate: f64,
    lanes: Lanes,
    compressor: Option<Compressor>,
    echo: Option<EchoLine>,
    /// Echoes a new echo time or feedback took over from, ringing out what they hold with nothing
    /// more sent, dropped once quiet.
    fading: Vec<EchoLine>,
    reverb: Option<Reverb>,
}

impl Bus {
    /// An empty bus at `rate` frames a second.
    pub(crate) fn new(rate: f64) -> Self {
        Self {
            rate,
            lanes: Lanes::default(),
            compressor: None,
            echo: None,
            fading: Vec::new(),
            reverb: None,
        }
    }

    /// Ready for a block of `frames`: every lane silent.
    pub(crate) fn begin(&mut self, frames: usize) {
        self.lanes.resize(frames);
    }

    /// Takes on what a starting note asks of its channel: its compressor, its reverb's size, its
    /// echo's time and feedback. Strudel's reverb and echo belong to the orbit, not the note, and
    /// the text sets the same on every note of a layer; the last note to start sets them.
    ///
    /// The compressor follows the note both ways: a note that asks for none takes it away, since
    /// it comes from the style (`docs/player.md` §4, "when the style asks for one"), which every
    /// layer shares — kept, it would go on compressing the next piece, and what a piece sounds like
    /// would depend on what played before it (I1). A reverb or an echo a note does not ask for is
    /// left to ring out: with nothing sent it falls silent of itself.
    ///
    /// A changed setting keeps what is sounding (`docs/player.md` §5, "Live edits": tails are the
    /// synth's state): the reverb takes its new size with its tail in it, the compressor its new
    /// curve from the reduction it had reached, and an echo with a new time (any change of tempo)
    /// or feedback lets the old line ring out its repeats beside the new one rather than cutting
    /// them on one frame.
    pub(crate) fn configure(&mut self, effects: &Effects) {
        match effects.compression {
            Some(compression) => {
                let setting = Setting::layer(compression.threshold(), compression.ratio());
                match &mut self.compressor {
                    Some(compressor) => compressor.set(setting, self.rate),
                    None => self.compressor = Some(Compressor::new(setting, self.rate)),
                }
            }
            None => self.compressor = None,
        }
        if let Some(room) = effects.room {
            let size = room.size().get();
            match &mut self.reverb {
                Some(reverb) if reverb.is(size) => {}
                Some(reverb) => reverb.resize(size),
                None => self.reverb = Some(Reverb::new(size, self.rate)),
            }
        }
        if let Some(echo) = effects.echo {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // positive and at most Echo::MAX_TIME seconds of frames
            let frames = (echo.time().get() * self.rate).round() as usize;
            if self
                .echo
                .as_ref()
                .is_none_or(|line| !line.is(frames, echo.feedback()))
                && let Some(old) = self.echo.replace(EchoLine::new(frames, echo.feedback()))
                && !old.is_quiet()
            {
                self.fading.push(old);
            }
        }
    }

    /// Frames `from..from + length` of the lanes, for a voice to add itself into.
    pub(crate) fn slice(&mut self, from: usize, length: usize) -> Option<Slice<'_>> {
        let to = from + length;
        let [pre_l, pre_r] = &mut self.lanes.pre;
        let [post_l, post_r] = &mut self.lanes.post;
        let [room_l, room_r] = &mut self.lanes.room;
        let [echo_l, echo_r] = &mut self.lanes.echo;
        Some(Slice {
            pre: [pre_l.get_mut(from..to)?, pre_r.get_mut(from..to)?],
            post: [post_l.get_mut(from..to)?, post_r.get_mut(from..to)?],
            room: [room_l.get_mut(from..to)?, room_r.get_mut(from..to)?],
            echo: [echo_l.get_mut(from..to)?, echo_r.get_mut(from..to)?],
        })
    }

    /// Frames `from..from + left.len()` of the block, finished: through the compressor, with the
    /// echo and the reverb, added into `left` and `right` — that stretch of the channel's stem.
    ///
    /// The renderer finishes a block a stretch at a time, up to each note that starts, so a note
    /// that changes the bus changes it from its own frame on: where it takes effect does not
    /// depend on where a block happens to begin (I1 across block sizes).
    pub(crate) fn finish(&mut self, from: usize, left: &mut [f32], right: &mut [f32]) {
        let lanes = &self.lanes;
        let frames = left.len().min(right.len());
        for n in 0..frames {
            let at = |side: &[f32]| side.get(from + n).map_or(0.0, |x| f64::from(*x));
            let gain = match &mut self.compressor {
                Some(compressor) => {
                    let level = at(&lanes.pre[0]).abs().max(at(&lanes.pre[1]).abs());
                    compressor.gain(level)
                }
                None => 1.0,
            };
            let mut l = gain * at(&lanes.post[0]);
            let mut r = gain * at(&lanes.post[1]);
            if let Some(echo) = &mut self.echo {
                let (x, y) = echo.process(gain * at(&lanes.echo[0]), gain * at(&lanes.echo[1]));
                l += x;
                r += y;
            }
            for echo in &mut self.fading {
                let (x, y) = echo.process(0.0, 0.0);
                l += x;
                r += y;
            }
            if let Some(reverb) = &mut self.reverb {
                let (x, y) = reverb.process(gain * at(&lanes.room[0]), gain * at(&lanes.room[1]));
                l += x;
                r += y;
            }
            #[allow(clippy::cast_possible_truncation)] // a sample to f32 PCM
            if let (Some(out_l), Some(out_r)) = (left.get_mut(n), right.get_mut(n)) {
                *out_l += l as f32;
                *out_r += r as f32;
            }
        }
        self.fading.retain(|echo| !echo.is_quiet());
    }

    /// Silenced: what its echo and reverb were ringing is dropped, as a stop drops the voices, and
    /// its compressor with them — the next piece's first note sets the bus up afresh, so it sounds
    /// as it would on a renderer that had played nothing before it.
    pub(crate) fn clear(&mut self) {
        self.compressor = None;
        self.echo = None;
        self.fading.clear();
        self.reverb = None;
    }
}

#[cfg(test)]
mod tests {
    use super::Bus;
    use crate::effects::{Compression, Echo, Effects, Room};
    use crate::units::{Gain, Seconds};

    #[test]
    fn a_dry_bus_passes_what_is_heard_and_nothing_else() {
        let mut bus = Bus::new(48_000.0);
        bus.begin(4);
        let slice = bus.slice(1, 2).unwrap();
        slice.post[0].fill(0.5);
        slice.post[1].fill(0.25);
        slice.pre[0].fill(9.0);
        let (mut l, mut r) = (vec![0.0; 4], vec![0.0; 4]);
        bus.finish(0, &mut l, &mut r);
        assert_eq!(l, vec![0.0, 0.5, 0.5, 0.0]);
        assert_eq!(r, vec![0.0, 0.25, 0.25, 0.0]);
    }

    #[test]
    fn its_echo_returns_what_was_sent_a_delay_later() {
        // An echo of 2 frames (at 8 kHz, 0.25 ms): sent 1.0 at frame 0, heard at frame 2.
        let mut bus = Bus::new(8_000.0);
        let mut effects = Effects::DRY;
        effects.echo = Echo::new(
            Gain::new(1.0).unwrap(),
            Seconds::new(2.0 / 8_000.0).unwrap(),
            0.5,
        );
        bus.configure(&effects);
        bus.begin(5);
        bus.slice(0, 1).unwrap().echo[0][0] = 1.0;
        let (mut l, mut r) = (vec![0.0; 5], vec![0.0; 5]);
        bus.finish(0, &mut l, &mut r);
        assert_eq!(l, vec![0.0, 0.0, 1.0, 0.0, 0.5]);
    }

    #[test]
    fn its_compressor_listens_before_postgain_and_acts_after() {
        // A 0 dBFS level before postgain is reduced by the curve (16.29 dB at −24 dB, 7:1) and
        // lifted by the makeup (9.77 dB): −6.51 dB, ×0.4722, once the 0.6 ms attack has settled. The
        // heard lane, at a quarter, comes out at 0.25 × 0.4722 = 0.1181.
        let mut bus = Bus::new(48_000.0);
        let mut effects = Effects::DRY;
        effects.compression = Compression::new(-24.0, 7.0);
        bus.configure(&effects);
        bus.begin(4_800);
        let slice = bus.slice(0, 4_800).unwrap();
        slice.pre[0].fill(1.0);
        slice.post[0].fill(0.25);
        let (mut l, mut r) = (vec![0.0; 4_800], vec![0.0; 4_800]);
        bus.finish(0, &mut l, &mut r);
        assert!((l[4_799] - 0.1181).abs() < 2e-4, "{}", l[4_799]);
    }

    #[test]
    fn a_new_reverb_size_keeps_what_the_reverb_is_ringing() {
        // An impulse sent into a 2 s room at frame 0 first comes out of the shortest line, 1 399
        // frames later. 1 000 frames in, the size changes to 3 s: the next 2 000 frames hold that
        // first return. A reverb built afresh for the new size would hold nothing, sent nothing.
        let mut bus = Bus::new(48_000.0);
        let mut effects = Effects::DRY;
        effects.room = Room::new(Gain::new(0.5).unwrap(), Seconds::new(2.0).unwrap());
        bus.configure(&effects);
        bus.begin(1_000);
        bus.slice(0, 1).unwrap().room[0][0] = 1.0;
        let (mut l, mut r) = (vec![0.0; 1_000], vec![0.0; 1_000]);
        bus.finish(0, &mut l, &mut r);
        assert!(l.iter().chain(&r).all(|x| *x == 0.0));
        effects.room = Room::new(Gain::new(0.5).unwrap(), Seconds::new(3.0).unwrap());
        bus.configure(&effects);
        assert!(bus.reverb.as_ref().is_some_and(|reverb| reverb.is(3.0)));
        bus.begin(2_000);
        let (mut l, mut r) = (vec![0.0; 2_000], vec![0.0; 2_000]);
        bus.finish(0, &mut l, &mut r);
        assert!(l[..399].iter().all(|x| *x == 0.0));
        assert!(l[399] != 0.0 && r[399] != 0.0, "{} {}", l[399], r[399]);
    }

    #[test]
    fn a_new_echo_time_lets_the_old_repeats_ring_out() {
        // An echo of 2 frames at 0.5 (8 kHz): 1.0 sent at frame 0 comes back 1.0 at frame 2 and
        // 0.5 at frame 4. After frame 2 the tempo changes and the echo becomes 3 frames: the old
        // line's 0.5 is still heard at frame 4 (then 0.25 at 6), while a send now (0.5 at frame 3)
        // returns on the new line 3 frames later, at frame 6, adding 0.5.
        let mut bus = Bus::new(8_000.0);
        let mut effects = Effects::DRY;
        let echo = |frames: f64| {
            Echo::new(
                Gain::new(1.0).unwrap(),
                Seconds::new(frames / 8_000.0).unwrap(),
                0.5,
            )
        };
        effects.echo = echo(2.0);
        bus.configure(&effects);
        bus.begin(3);
        bus.slice(0, 1).unwrap().echo[0][0] = 1.0;
        let (mut l, mut r) = (vec![0.0; 3], vec![0.0; 3]);
        bus.finish(0, &mut l, &mut r);
        assert_eq!(l, vec![0.0, 0.0, 1.0]);
        effects.echo = echo(3.0);
        bus.configure(&effects);
        bus.begin(5);
        bus.slice(0, 1).unwrap().echo[0][0] = 0.5;
        let (mut l, mut r) = (vec![0.0; 5], vec![0.0; 5]);
        bus.finish(0, &mut l, &mut r);
        // Frames 3 to 7.
        assert_eq!(l, vec![0.0, 0.5, 0.0, 0.75, 0.0]);
    }

    #[test]
    fn a_note_without_compression_takes_the_compressor_away() {
        // The compressor comes from the style: a piece without one must not keep the last one's.
        let mut bus = Bus::new(48_000.0);
        let mut effects = Effects::DRY;
        effects.compression = Compression::new(-24.0, 7.0);
        bus.configure(&effects);
        assert!(bus.compressor.is_some());
        bus.configure(&Effects::DRY);
        assert!(bus.compressor.is_none());
        bus.configure(&effects);
        bus.clear();
        assert!(bus.compressor.is_none());
    }

    #[test]
    fn a_stretch_is_finished_from_where_it_starts() {
        // Finishing frames 2..4 of a block reads the lanes from frame 2.
        let mut bus = Bus::new(48_000.0);
        bus.begin(4);
        bus.slice(0, 4).unwrap().post[0].copy_from_slice(&[0.1, 0.2, 0.3, 0.4]);
        let (mut l, mut r) = (vec![0.0; 2], vec![0.0; 2]);
        bus.finish(2, &mut l, &mut r);
        assert_eq!(l, vec![0.3, 0.4]);
    }
}
