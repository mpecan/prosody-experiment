//! The render thread's rounds against a pretend output: a failed one reopened while something plays
//! and the music going on from the frontier, a reopen that fails told once and tried again, and the
//! output paused while nothing plays and resumed by a Play. Worked by hand at 48 kHz, 1 024-frame
//! blocks, a 4 096-frame lead.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::time::Instant;

use prosody_player::Mixer;

use super::{IDLE, RETRY, Rounds, Sink, Wait};
use crate::player::host::tests::{play, rig};
use crate::player::host::{AudioSide, Shared, ToAudio};

/// An output that records what was asked of it; a reopen gives a fresh mixer on the new channel.
#[derive(Default)]
struct Pretend {
    calls: Vec<&'static str>,
    /// Reopens still to fail.
    failing: u32,
    /// Whether a resume fails.
    stuck: bool,
    /// The newest output's callback side.
    audio: Option<AudioSide>,
}

impl Sink for Pretend {
    fn reopen(
        &mut self,
        shared: &Arc<Shared>,
        outbox: &mut VecDeque<ToAudio>,
    ) -> Result<(), String> {
        self.calls.push("reopen");
        if self.failing > 0 {
            self.failing -= 1;
            return Err("no output".to_owned());
        }
        let from = shared.reconnect(outbox);
        let (back, _spent) = sync_channel(64);
        self.audio = Some(AudioSide {
            mixer: Mixer::new(48_000.0),
            from,
            back,
            channels: 2,
        });
        Ok(())
    }

    fn pause(&mut self) -> Result<(), String> {
        self.calls.push("pause");
        Ok(())
    }

    fn resume(&mut self) -> Result<(), String> {
        self.calls.push("resume");
        if self.stuck {
            Err("would not start".to_owned())
        } else {
            Ok(())
        }
    }
}

#[test]
fn a_failed_output_is_reopened_while_playing_and_the_music_goes_on_from_the_frontier() {
    let (shared, mut audio, spent) = rig();
    let shared = Arc::new(shared);
    let mut rounds = Rounds::default();
    let mut output = Pretend::default();
    play(&shared, 1, 0.0);
    // The flush and four blocks, frames 0 to 4 096; the output plays 2 048 of them.
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    let mut out = vec![0.0_f32; 2 * 2_048];
    audio.fill(&mut out, &shared, 0);
    // It fails. The next round renders two blocks more for it (to 6 144), then reopens it.
    shared.lose();
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    assert_eq!(output.calls, vec!["reopen"]);
    // The new output hears a flush and four blocks from 6 144; a buffer of 1 024 takes it to 7 168,
    // with the sine (half a second, to 24 000) still sounding.
    rounds.round(&shared, &spent, &mut output);
    let mut fresh = output.audio.take().unwrap();
    let mut out = vec![0.0_f32; 2 * 1_024];
    fresh.fill(&mut out, &shared, 0);
    assert!(out.iter().any(|x| x.abs() > 0.01));
    let answer = shared.poll(Instant::now());
    assert!(
        answer.starts_with(
            r#"{"report":{"played":7168,"underruns":0,"queued":3072,"started":true,"generation":1,"#
        ),
        "{answer}"
    );
    // One glitch for what was in flight, and no failure told: it went on.
    assert!(answer.contains(r#""glitches":1}"#), "{answer}");
    assert!(!answer.contains(r#""op":"error""#), "{answer}");
}

#[test]
fn a_reopen_that_fails_is_told_once_and_tried_again_a_second_later() {
    let (shared, _audio, spent) = rig();
    let shared = Arc::new(shared);
    let mut rounds = Rounds::default();
    let mut output = Pretend {
        failing: 2,
        ..Pretend::default()
    };
    play(&shared, 1, 0.0);
    rounds.round(&shared, &spent, &mut output);
    shared.lose();
    // Tried at once, then every RETRY + 1 rounds: on the third try it opens.
    let mut tries = Vec::new();
    for n in 0..=2 * (RETRY + 1) {
        let before = output.calls.len();
        rounds.round(&shared, &spent, &mut output);
        if output.calls.len() > before {
            tries.push(n);
        }
    }
    assert_eq!(tries, vec![0, RETRY + 1, 2 * (RETRY + 1)]);
    assert!(output.audio.is_some());
    let answer = shared.poll(Instant::now());
    assert_eq!(
        answer
            .matches("would not reopen (no output); trying again")
            .count(),
        1,
        "{answer}"
    );
}

#[test]
fn a_failed_output_is_not_reopened_while_nothing_plays_but_on_the_next_play() {
    let (shared, _audio, spent) = rig();
    let shared = Arc::new(shared);
    let mut rounds = Rounds::default();
    let mut output = Pretend::default();
    shared.lose();
    for _ in 1..IDLE {
        assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    }
    // Asleep, and not paused: there is no output to pause.
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Asleep);
    assert!(output.calls.is_empty());
    play(&shared, 1, 0.0);
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    assert_eq!(output.calls, vec!["reopen"]);
}

#[test]
fn nothing_playing_for_a_second_pauses_the_output_and_a_play_resumes_it() {
    let (shared, _audio, spent) = rig();
    let shared = Arc::new(shared);
    let mut rounds = Rounds::default();
    let mut output = Pretend::default();
    for _ in 1..IDLE {
        assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    }
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Asleep);
    assert_eq!(output.calls, vec!["pause"]);
    // Woken by a message that plays nothing: asleep again at once, not paused twice.
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Asleep);
    assert_eq!(output.calls, vec!["pause"]);
    play(&shared, 1, 0.0);
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    assert_eq!(output.calls, vec!["pause", "resume"]);
    // Stopped: a second later, paused again.
    shared.engine.lock().unwrap().stop(2);
    for _ in 1..IDLE {
        assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Round);
    }
    assert_eq!(rounds.round(&shared, &spent, &mut output), Wait::Asleep);
    assert_eq!(output.calls, vec!["pause", "resume", "pause"]);
}

#[test]
fn an_output_that_will_not_start_again_is_reopened() {
    let (shared, _audio, spent) = rig();
    let shared = Arc::new(shared);
    let mut rounds = Rounds::default();
    let mut output = Pretend {
        stuck: true,
        ..Pretend::default()
    };
    for _ in 0..IDLE {
        rounds.round(&shared, &spent, &mut output);
    }
    play(&shared, 1, 0.0);
    rounds.round(&shared, &spent, &mut output);
    rounds.round(&shared, &spent, &mut output);
    assert_eq!(output.calls, vec!["pause", "resume", "reopen"]);
}
