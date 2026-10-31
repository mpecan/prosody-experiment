//! The native player's two threads (`docs/player.md` §13): a render thread that keeps the engine
//! about half a second ahead of the output, as the browser's render worker does, and the audio
//! callback, which only mixes, as the worklet does. Neither waits for the web view, so the music goes
//! on when the screen is off and the page is paused.
//!
//! ```text
//!   page ──invoke──► engine (Mutex) ◄── render thread ──blocks (channel)──► audio callback ──► speaker
//!     ▲                    │                                                    │
//!     └──poll── events ◄───┘                       status (atomics) ◄───────────┘
//! ```
//!
//! The audio callback takes no lock and allocates nothing: blocks arrive over a bounded channel,
//! finished ones go back over another for the render thread to reuse, the stems' gains are read from
//! atomics, and what the page and the engine need to know of it is written to atomics. Nothing
//! waits on it either: what the channel has no room for waits in the render thread's outbox.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Mutex, PoisonError};

use prosody_player::{Block, Engine, Mixer, Report};
use prosody_synth::STEMS;

/// The most events kept for a page that does not ask (a paused web view): beyond this the oldest
/// go, and the page is told how many.
const KEPT: usize = 4_096;

/// What the render thread sends the audio callback, in order.
#[derive(Debug)]
pub(crate) enum ToAudio {
    /// Drop what is queued, for this start of play.
    Flush(u32),
    /// A rendered block.
    Block(Block),
}

/// The mix the page asks for, read by the audio callback each buffer: the stems' gains (mute,
/// solo, the reading's level) and the music's level under a line of the browser's voice, as `f64`
/// bits.
#[derive(Debug)]
pub(crate) struct Mix {
    gains: [AtomicU64; STEMS],
    duck: AtomicU64,
}

impl Default for Mix {
    fn default() -> Self {
        Self {
            gains: std::array::from_fn(|_| AtomicU64::new(1.0_f64.to_bits())),
            duck: AtomicU64::new(1.0_f64.to_bits()),
        }
    }
}

impl Mix {
    /// The stems' gains, from the page.
    pub(crate) fn set_gains(&self, gains: &[f64]) {
        for (slot, gain) in self.gains.iter().zip(gains) {
            slot.store(gain.to_bits(), Ordering::Relaxed);
        }
    }

    /// The browser voice's duck, from the page.
    pub(crate) fn set_duck(&self, level: f64) {
        self.duck.store(level.to_bits(), Ordering::Relaxed);
    }

    fn gains(&self) -> [f64; STEMS] {
        std::array::from_fn(|n| {
            self.gains
                .get(n)
                .map_or(1.0, |gain| f64::from_bits(gain.load(Ordering::Relaxed)))
        })
    }

    fn duck(&self) -> f64 {
        f64::from_bits(self.duck.load(Ordering::Relaxed))
    }
}

/// How the output is doing, written by the audio callback and read by the render thread and the
/// page.
#[derive(Debug, Default)]
pub(crate) struct Status {
    played: AtomicU64,
    underruns: AtomicU64,
    queued: AtomicU64,
    started: AtomicBool,
    generation: AtomicU32,
    /// `f32` bits: for a level, never negative, the bits order as the numbers do, so the largest
    /// is a `fetch_max`.
    peak: AtomicU32,
    speech_peak: AtomicU32,
    latency_ns: AtomicU64,
    /// Times the output itself ran short or over (an `AAudio` xrun): a glitch below the mix, which
    /// the queue's underruns do not count.
    glitches: AtomicU64,
    /// The most frames the output has asked for in one buffer.
    buffer: AtomicU64,
}

impl Status {
    /// Records the mixer's report and the output's latency.
    pub(crate) fn store(&self, report: Report, latency_ns: u64) {
        self.played.store(report.played, Ordering::Relaxed);
        self.underruns.store(report.underruns, Ordering::Relaxed);
        self.queued.store(report.queued, Ordering::Relaxed);
        self.started.store(report.started, Ordering::Relaxed);
        self.generation.store(report.generation, Ordering::Relaxed);
        self.peak
            .fetch_max(report.peak.abs().to_bits(), Ordering::Relaxed);
        self.speech_peak
            .fetch_max(report.speech_peak.abs().to_bits(), Ordering::Relaxed);
        self.latency_ns.store(latency_ns, Ordering::Relaxed);
    }

    /// Times the queue ran dry, and times the output glitched.
    pub(crate) fn losses(&self) -> (u64, u64) {
        (
            self.underruns.load(Ordering::Relaxed),
            self.glitches.load(Ordering::Relaxed),
        )
    }

    /// Frames queued ahead of the playhead.
    pub(crate) fn queued(&self) -> u64 {
        self.queued.load(Ordering::Relaxed)
    }

    /// The output glitched once more.
    pub(crate) fn glitched(&self) {
        self.glitches.fetch_add(1, Ordering::Relaxed);
    }

    /// The report as the page's worklet report was, as JSON; the peaks start again from here.
    pub(crate) fn json(&self) -> String {
        let peak = f32::from_bits(self.peak.swap(0, Ordering::Relaxed));
        let speech = f32::from_bits(self.speech_peak.swap(0, Ordering::Relaxed));
        #[allow(clippy::cast_precision_loss)] // a latency in nanoseconds, far below 2^52
        let latency = self.latency_ns.load(Ordering::Relaxed) as f64 / 1e9;
        format!(
            r#"{{"played":{},"underruns":{},"queued":{},"started":{},"generation":{},"peak":{peak},"speechPeak":{speech},"latency":{latency},"glitches":{}}}"#,
            self.played.load(Ordering::Relaxed),
            self.underruns.load(Ordering::Relaxed),
            self.queued.load(Ordering::Relaxed),
            self.started.load(Ordering::Relaxed),
            self.generation.load(Ordering::Relaxed),
            self.glitches.load(Ordering::Relaxed),
        )
    }
}

/// What the render thread, the commands and the audio callback share.
#[derive(Debug)]
pub(crate) struct Shared {
    /// The engine: the commands drive it, the render thread renders it.
    pub(crate) engine: Mutex<Engine>,
    /// Its events as JSON, oldest first, until the page asks.
    events: Mutex<(VecDeque<String>, u64)>,
    /// The output's status.
    pub(crate) status: Status,
    /// The mix the page asks for.
    pub(crate) mix: Mix,
    /// To the audio callback.
    to_audio: SyncSender<ToAudio>,
    /// Set to end the render thread.
    pub(crate) quit: AtomicBool,
}

impl Shared {
    /// Shared state around `engine`, sending to the audio callback down `to_audio`.
    pub(crate) fn new(engine: Engine, to_audio: SyncSender<ToAudio>) -> Self {
        Self {
            engine: Mutex::new(engine),
            events: Mutex::new((VecDeque::new(), 0)),
            status: Status::default(),
            mix: Mix::default(),
            to_audio,
            quit: AtomicBool::new(false),
        }
    }

    /// Keeps an event for the page.
    pub(crate) fn tell(&self, json: String) {
        let mut kept = self.events.lock().unwrap_or_else(PoisonError::into_inner);
        if kept.0.len() >= KEPT {
            kept.0.pop_front();
            kept.1 += 1;
        }
        kept.0.push_back(json);
    }

    /// What the page asked for: the output's report, the events since it last asked, and how many
    /// were let go meanwhile, as JSON.
    pub(crate) fn poll(&self) -> String {
        let (events, lost) = {
            let mut kept = self.events.lock().unwrap_or_else(PoisonError::into_inner);
            let events: Vec<String> = kept.0.drain(..).collect();
            let lost = std::mem::take(&mut kept.1);
            (events, lost)
        };
        format!(
            r#"{{"report":{},"events":[{}],"lost":{lost}}}"#,
            self.status.json(),
            events.join(",")
        )
    }

    /// A button of the notification, the lock screen or a headset: `pause` and `stop` stop the music
    /// here and now — the page may be asleep — and tell the page, which stops too when it wakes;
    /// `play` asks the page to play, since only it can make the piece. Says whether it stopped.
    pub(crate) fn button(&self, action: &str) -> bool {
        let stops = matches!(action, "pause" | "stop");
        if stops {
            let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
            // In the start of play it was in: the page's next Stop or Play begins the next.
            let generation = engine.generation();
            engine.stop(generation);
        }
        let told = if stops { "stop" } else { "play" };
        self.tell(format!(r#"{{"op":"media","action":"{told}"}}"#));
        stops
    }

    /// One round of the render thread: the engine told how far the output has got, a restart said
    /// downstream, blocks rendered and sent until the lead is full, its events kept for the page.
    /// `spare` holds finished blocks' buffers to render into; `outbox` what the channel had no room
    /// for, sent first next time, in order — a dropped flush would let the old piece play on, and
    /// a dropped block would be a gap. (It fills only while the output has stopped asking, and then
    /// the engine stops rendering too: the output's count no longer moves.)
    pub(crate) fn pump(&self, spare: &mut Vec<Vec<f32>>, outbox: &mut VecDeque<ToAudio>) {
        let mut engine = self.engine.lock().unwrap_or_else(PoisonError::into_inner);
        // Two of the output's largest buffers ahead at least: one being played, one waiting. Measured
        // on an Android emulator, the buffer grew past the half-second lead as the screen went off,
        // and the queue ran dry inside it (`docs/player.md` §13).
        let wanted = self.status.buffer.load(Ordering::Relaxed).saturating_mul(2);
        if wanted > engine.lead() {
            log::info!(
                "the native player keeps {wanted} frames ahead, twice the output's largest buffer"
            );
            engine.widen(wanted);
        }
        engine.played(
            self.status.generation.load(Ordering::Relaxed),
            self.status.played.load(Ordering::Relaxed),
        );
        if let Some(generation) = engine.take_flush() {
            outbox.push_back(ToAudio::Flush(generation));
        }
        while engine.due() {
            let generation = engine.generation();
            let (frame, stems) = engine.render();
            let mut data = spare.pop().unwrap_or_default();
            data.clear();
            data.extend_from_slice(stems.as_slice());
            outbox.push_back(ToAudio::Block(Block {
                frame,
                generation,
                data,
            }));
        }
        let events = engine.drain();
        drop(engine);
        while let Some(message) = outbox.pop_front() {
            match self.to_audio.try_send(message) {
                Ok(()) => {}
                Err(TrySendError::Full(message)) => {
                    outbox.push_front(message);
                    break;
                }
                Err(TrySendError::Disconnected(_)) => {
                    outbox.clear();
                    break;
                }
            }
        }
        for event in events {
            self.tell(event.json());
        }
    }
}

/// The audio callback's own state: the mixer, what feeds it, and where finished blocks go.
#[derive(Debug)]
pub(crate) struct AudioSide {
    /// The queue and master stage.
    pub(crate) mixer: Mixer,
    /// From the render thread and the page.
    pub(crate) from: Receiver<ToAudio>,
    /// Finished blocks, back to the render thread.
    pub(crate) back: SyncSender<Vec<f32>>,
    /// Samples a frame the output takes.
    pub(crate) channels: usize,
}

impl AudioSide {
    /// Fills `out` with the next frames, after taking what has arrived and the mix asked for;
    /// reports to `status`.
    pub(crate) fn fill(&mut self, out: &mut [f32], shared: &Shared, latency_ns: u64) {
        loop {
            match self.from.try_recv() {
                Ok(ToAudio::Flush(generation)) => self.mixer.flush(generation),
                Ok(ToAudio::Block(block)) => self.mixer.push(block),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        self.mixer.set_gains(shared.mix.gains());
        self.mixer.set_duck(shared.mix.duck());
        let frames = out.len() / self.channels.max(1);
        shared
            .status
            .buffer
            .fetch_max(u64::try_from(frames).unwrap_or(u64::MAX), Ordering::Relaxed);
        self.mixer.mix(out, self.channels);
        while let Some(block) = self.mixer.take_spent() {
            // A full return channel frees the buffer here; it has room for every block in flight.
            let _ = self.back.try_send(block.data);
        }
        shared.status.store(self.mixer.report(), latency_ns);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::sync_channel;

    use prosody_player::records::NOTE_STRIDE;
    use prosody_player::{Engine, Handed, Mixer, Pace};
    use prosody_synth::{Gain, SampleRate, Seconds, Stems};

    use std::collections::VecDeque;

    use super::{AudioSide, Shared};

    const RATE: u32 = 48_000;

    /// A sine's record on the melody at `onset` seconds, half a second long.
    fn sine(onset: f64) -> [f64; NOTE_STRIDE] {
        let mut r = [f64::NAN; NOTE_STRIDE];
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

    fn rig() -> (Shared, AudioSide, std::sync::mpsc::Receiver<Vec<f32>>) {
        let (to_audio, from) = sync_channel(64);
        let (back, spent) = sync_channel(64);
        let engine = Engine::new(SampleRate::new(RATE).unwrap(), 1_024, 4_096);
        let audio = AudioSide {
            mixer: Mixer::new(f64::from(RATE)),
            from,
            back,
            channels: 2,
        };
        (Shared::new(engine, to_audio), audio, spent)
    }

    fn play(shared: &Shared, generation: u32, onset: f64) {
        let piece = Handed {
            serial: 1,
            notes: prosody_player::records::decode_notes(&sine(onset)).unwrap(),
            bars: vec![Seconds::new(0.0).unwrap()],
            seconds: Seconds::new(1.0).unwrap(),
            cues: Vec::new(),
            cast: prosody_player::Cast::new(),
        };
        shared.engine.lock().unwrap().play(
            generation,
            piece,
            Pace::Natural,
            Gain::new(1.0).unwrap(),
        );
    }

    #[test]
    fn the_render_thread_keeps_the_lead_and_the_callback_plays_it() {
        let (shared, mut audio, spent) = rig();
        play(&shared, 1, 0.0);
        let mut spare = Vec::new();
        shared.pump(&mut spare, &mut VecDeque::new());
        // Through the callback: the flush, then the sine, from the first frame.
        let mut out = vec![0.0_f32; 2 * 2_048];
        audio.fill(&mut out, &shared, 5_000_000);
        assert!(out.iter().any(|x| x.abs() > 0.01));
        // Four blocks were rendered (the 4 096-frame lead) and two played.
        let report = shared.poll();
        assert!(
            report.starts_with(
                r#"{"report":{"played":2048,"underruns":0,"queued":2048,"started":true,"generation":1,"#
            ),
            "{report}"
        );
        assert!(
            report.contains(r#""latency":0.005,"glitches":0}"#),
            "{report}"
        );
        // The two played come back for reuse; told the output is 2 048 frames on, the engine renders
        // two more into them to keep its lead.
        let mut spare: Vec<Vec<f32>> = spent.try_iter().collect();
        assert_eq!(spare.len(), 2);
        shared.pump(&mut spare, &mut VecDeque::new());
        assert!(spare.is_empty());
        let mut out = vec![0.0_f32; 2 * 4_096];
        audio.fill(&mut out, &shared, 0);
        let report = shared.poll();
        assert!(
            report.starts_with(r#"{"report":{"played":6144,"underruns":0,"queued":0,"#),
            "{report}"
        );
    }

    #[test]
    fn a_page_that_does_not_ask_loses_its_oldest_events_and_is_told_how_many() {
        let (shared, _audio, _spent) = rig();
        for n in 0..super::KEPT + 3 {
            shared.tell(format!("{n}"));
        }
        let answer = shared.poll();
        assert!(
            answer.ends_with(r#"4098],"lost":3}"#),
            "{}",
            &answer[answer.len() - 40..]
        );
        assert!(answer.contains(r#""events":[3,4,"#));
        assert!(shared.poll().contains(r#""events":[],"lost":0"#));
    }

    #[test]
    fn the_peaks_start_again_once_the_page_has_them() {
        let (shared, mut audio, _spent) = rig();
        // The drums' left side at 0.25 for four frames.
        let mut data = vec![0.0; Stems::PER_FRAME * 4];
        data[..4].fill(0.25);
        // The duck lane, after the five stems: the music at full level.
        data[40..].fill(1.0);
        audio.mixer.push(prosody_player::Block {
            frame: 0,
            generation: 0,
            data,
        });
        let mut out = vec![0.0_f32; 8];
        audio.fill(&mut out, &shared, 0);
        let first = shared.status.json();
        assert!(!first.contains(r#""peak":0,"#), "{first} {out:?}");
        assert!(shared.status.json().contains(r#""peak":0,"#));
    }

    #[test]
    fn a_stem_the_page_mutes_is_silent_from_the_next_buffer() {
        let (shared, mut audio, _spent) = rig();
        play(&shared, 1, 0.0);
        // Before the first block arrives: a stem muted before play is silent from its first frame.
        shared.mix.set_gains(&[1.0, 1.0, 0.0, 1.0, 1.0]);
        shared.pump(&mut Vec::new(), &mut VecDeque::new());
        let mut out = vec![0.0_f32; 2 * 1_024];
        audio.fill(&mut out, &shared, 0);
        assert!(out.iter().all(|x| *x == 0.0));
        shared.mix.set_gains(&[1.0; 5]);
        audio.fill(&mut out, &shared, 0);
        assert!(out.iter().any(|x| x.abs() > 0.01));
    }

    #[test]
    fn what_the_channel_has_no_room_for_waits_in_order() {
        // Room for two: a flush and four blocks go two at a time, in order.
        let (to_audio, from) = sync_channel(2);
        let engine = Engine::new(SampleRate::new(RATE).unwrap(), 1_024, 4_096);
        let shared = Shared::new(engine, to_audio);
        play(&shared, 1, 0.0);
        let mut outbox = VecDeque::new();
        let mut spare = Vec::new();
        let mut seen = Vec::new();
        for _ in 0..3 {
            shared.pump(&mut spare, &mut outbox);
            while let Ok(message) = from.try_recv() {
                seen.push(match message {
                    super::ToAudio::Flush(generation) => format!("flush {generation}"),
                    super::ToAudio::Block(block) => format!("block {}", block.frame),
                });
            }
        }
        assert_eq!(
            seen,
            vec![
                "flush 1",
                "block 0",
                "block 1024",
                "block 2048",
                "block 3072"
            ]
        );
        assert!(outbox.is_empty());
    }

    #[test]
    fn a_pause_button_stops_the_music_here_and_tells_the_page_and_play_only_asks() {
        let (shared, _audio, _spent) = rig();
        play(&shared, 3, 0.0);
        shared.poll();
        assert!(!shared.button("play"));
        assert!(shared.engine.lock().unwrap().playing());
        assert!(shared.button("pause"));
        assert!(!shared.engine.lock().unwrap().playing());
        // Stopped in the start of play it was in, so the page's own Stop, when it wakes, is the next.
        assert_eq!(shared.engine.lock().unwrap().generation(), 3);
        let answer = shared.poll();
        assert!(
            answer.contains(
                r#""events":[{"op":"media","action":"play"},{"op":"media","action":"stop"}]"#
            ),
            "{answer}"
        );
    }
}

#[cfg(test)]
mod lead {
    use std::collections::VecDeque;
    use std::sync::mpsc::sync_channel;

    use prosody_player::{Engine, Mixer};
    use prosody_synth::SampleRate;

    use super::{AudioSide, Shared};

    #[test]
    fn the_lead_grows_to_twice_the_largest_buffer_the_output_asks_for() {
        let (to_audio, from) = sync_channel(64);
        let (back, _spent) = sync_channel(64);
        let shared = Shared::new(
            Engine::new(SampleRate::new(48_000).unwrap(), 1_024, 4_096),
            to_audio,
        );
        let mut audio = AudioSide {
            mixer: Mixer::new(48_000.0),
            from,
            back,
            channels: 2,
        };
        // A buffer of 3 000 frames, stereo: the lead must be 6 000 frames at least.
        let mut out = vec![0.0_f32; 6_000];
        audio.fill(&mut out, &shared, 0);
        shared.pump(&mut Vec::new(), &mut VecDeque::new());
        assert_eq!(shared.engine.lock().unwrap().lead(), 6_000);
        // A smaller buffer after it does not shrink it.
        let mut out = vec![0.0_f32; 200];
        audio.fill(&mut out, &shared, 0);
        shared.pump(&mut Vec::new(), &mut VecDeque::new());
        assert_eq!(shared.engine.lock().unwrap().lead(), 6_000);
    }
}
