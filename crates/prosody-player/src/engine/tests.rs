//! The engine as a host drives it: a piece played and rendered ahead, a restart said downstream,
//! the reading's lines asked for, placed and let go, and what is lost told. Worked by hand at
//! 48 kHz, 1 024-frame blocks.

use std::collections::BTreeMap;

use prosody_synth::{Channel, Gain, Losses, Sample, SampleRate, Seconds};

use super::{Engine, Handed};
use crate::event::Event;
use crate::reader::{Cast, Cue, Fitted, Pace, Told};
use crate::records::{NOTE_STRIDE, decode_notes};

const BLOCK: usize = 1_024;

fn engine() -> Engine {
    Engine::new(SampleRate::new(48_000).unwrap(), BLOCK, 24_000)
}

fn s(seconds: f64) -> Seconds {
    Seconds::new(seconds).unwrap()
}

/// A note's record: a sine (kind 2, wave 0) on the melody at `onset`, half a second long, or a drum
/// (kind 0) of instrument `id`.
fn record(kind: f64, id: f64, onset: f64, pitch: f64) -> [f64; NOTE_STRIDE] {
    let mut r = [f64::NAN; NOTE_STRIDE];
    r[..12].copy_from_slice(&[
        0.0,
        2.0,
        kind,
        id,
        onset,
        0.5,
        pitch,
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

fn handed(serial: u32, records: &[f64], cues: Vec<Cue>, cast: Cast) -> Handed {
    Handed {
        serial,
        notes: decode_notes(records).unwrap(),
        bars: vec![s(0.0), s(1.0)],
        seconds: s(2.0),
        cues,
        cast,
    }
}

fn unity() -> Gain {
    Gain::new(1.0).unwrap()
}

#[test]
fn it_plays_and_renders_blocks_of_stems() {
    let mut engine = engine();
    // A sine on the melody at a quarter of a second: frame 12 000, in block 11 (frames
    // 11 264..12 288).
    engine.play(
        1,
        handed(1, &record(2.0, 0.0, 0.25, 69.0), Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    let mut heard = Vec::new();
    for _ in 0..13 {
        let (_, stems) = engine.render();
        heard.push(
            stems
                .side(Channel::Melody, false)
                .iter()
                .any(|x| x.abs() > 1e-3),
        );
    }
    assert_eq!(heard.iter().position(|h| *h), Some(11));
    assert_eq!(engine.frontier(), 13 * 1_024);
    assert_eq!(engine.losses(), Losses::default());
}

#[test]
fn a_restart_is_said_downstream_once_and_the_lead_counts_from_it() {
    let mut engine = engine();
    assert_eq!(engine.take_flush(), None);
    assert!(!engine.due());
    engine.play(
        3,
        handed(1, &[], Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    assert_eq!(engine.take_flush(), Some(3));
    assert_eq!(engine.take_flush(), None);
    // 24 000 frames of lead is 23.4 blocks: due for 24, then full.
    let mut blocks = 0;
    while engine.due() {
        engine.render();
        blocks += 1;
    }
    assert_eq!(blocks, 24);
    // The output says it has played two blocks — of an older start of play first, which changes
    // nothing.
    engine.played(2, 2_048);
    assert!(!engine.due());
    engine.played(3, 2_048);
    assert!(engine.due());
    // A stop drops the queue, as start of play 4, and nothing is due.
    engine.stop(4);
    assert_eq!(engine.take_flush(), Some(4));
    assert!(!engine.due());
}

#[test]
fn it_places_a_recorded_line_on_its_cue_and_says_so() {
    let mut engine = engine();
    // Line 0 fitted on the bar at 0.1 s: frame 4 800, in block 4 (frames 4 096..5 120).
    let cue = Cue {
        line: 0,
        stanza: 0,
        pause: s(0.0),
        fitted: Some(Fitted {
            slot: 0,
            at: s(0.1),
        }),
        natural: None,
    };
    let cast = Cast::from([(0, Some("k".to_owned()))]);
    engine.play(
        1,
        handed(1, &[], vec![cue], cast),
        Pace::Fitted,
        Gain::new(0.6).unwrap(),
    );
    assert!(engine.drain().contains(&Event::Want(vec!["k".to_owned()])));
    // 480 frames at 24 kHz: 960 at 48 kHz, no quiet lead.
    engine
        .line(
            "k",
            Sample::new(vec![0.25; 480], None, 24_000.0).unwrap(),
            s(0.0),
        )
        .unwrap();
    for _ in 0..4 {
        engine.render();
    }
    let (frame, stems) = engine.render();
    assert_eq!(frame, 4_096);
    let speech = stems.side(Channel::Speech, false);
    // Silent before frame 4 800 (offset 704), the line's 0.25 after; the music ducked under it.
    assert!(speech[..704].iter().all(|x| *x == 0.0));
    assert!((speech[900] - 0.25).abs() < 1e-6);
    assert!(stems.duck()[1_000] < 1.0);
    let spoken = engine.drain().into_iter().find_map(|event| match event {
        Event::Reading {
            generation: 1,
            told: Told::Spoken { start, end, .. },
        } => Some((start, end)),
        _ => None,
    });
    assert_eq!(spoken, Some((4_800, 5_760)));
}

#[test]
fn it_asks_for_the_lines_cast_and_lets_go_of_those_no_piece_casts() {
    let mut engine = engine();
    let cast = |key: &str| Cast::from([(0, Some(key.to_owned()))]);
    engine.play(
        1,
        handed(1, &[], Vec::new(), cast("a")),
        Pace::Natural,
        unity(),
    );
    assert_eq!(
        engine.drain(),
        vec![
            Event::Placement {
                serial: 1,
                out_start: 0,
                piece_start: 0
            },
            Event::Want(vec!["a".to_owned()])
        ]
    );
    let line = || Sample::new(vec![0.1; 10], None, 48_000.0).unwrap();
    engine.line("a", line(), s(0.0)).unwrap();
    engine.line("b", line(), s(0.0)).unwrap();
    // Handed over to a piece cast with "c": "a" is still the playing piece's, "b" is nobody's.
    engine.swap(handed(2, &[], Vec::new(), cast("c")));
    let events = engine.drain();
    assert!(
        events.contains(&Event::Dropped(vec!["b".to_owned()])),
        "{events:?}"
    );
    assert!(
        events.contains(&Event::Want(vec!["c".to_owned()])),
        "{events:?}"
    );
    // A recast to the same cast casts "a" no more once the old piece is replaced by a recast.
    engine.cast(
        &BTreeMap::from([(1, cast("c")), (2, cast("c"))]),
        Pace::Natural,
        false,
    );
    assert!(
        engine
            .drain()
            .contains(&Event::Dropped(vec!["a".to_owned()]))
    );
}

#[test]
fn a_hand_over_with_nothing_playing_is_stale_and_ignored() {
    let mut engine = engine();
    engine.swap(handed(5, &[], Vec::new(), Cast::new()));
    assert!(engine.drain().is_empty());
    assert!(!engine.playing());
}

#[test]
fn a_hand_over_says_where_it_takes_over() {
    let mut engine = engine();
    engine.play(
        1,
        handed(1, &[], Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    for _ in 0..3 {
        engine.render();
    }
    engine.drain();
    // Bars at 0 and 1 s; the frontier at 3 072: the next bar boundary is 48 000, the new piece's
    // bar 2 (its 48 000).
    engine.swap(handed(2, &[], Vec::new(), Cast::new()));
    assert_eq!(
        engine.drain().first(),
        Some(&Event::Placement {
            serial: 2,
            out_start: 48_000,
            piece_start: 48_000
        })
    );
}

#[test]
fn a_note_whose_sound_was_never_loaded_is_counted_and_told() {
    let mut engine = engine();
    engine.play(
        1,
        handed(1, &record(0.0, 4.0, 0.0, f64::NAN), Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    engine.render();
    assert!(engine.drain().contains(&Event::Losses(Losses {
        stolen: 0,
        missing: 1
    })));
    // Told once, not every block.
    engine.render();
    assert!(
        !engine
            .drain()
            .iter()
            .any(|event| matches!(event, Event::Losses(_)))
    );
}

#[test]
fn a_line_let_go_gives_its_number_to_the_next() {
    let mut engine = engine();
    let line = || Sample::new(vec![0.1; 10], None, 48_000.0).unwrap();
    engine.line("a", line(), s(0.0)).unwrap();
    // Played with a cast of nobody: "a" is let go, and its number (0) is free.
    engine.play(
        1,
        handed(1, &[], Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    assert!(
        engine
            .drain()
            .contains(&Event::Dropped(vec!["a".to_owned()]))
    );
    engine.line("b", line(), s(0.0)).unwrap();
    assert_eq!(engine.free_lines, Vec::<u32>::new());
    assert_eq!(engine.next_line, 1);
}

#[test]
fn a_wider_lead_renders_further_ahead_and_a_narrower_one_never_below_its_own() {
    let mut engine = engine();
    engine.play(
        1,
        handed(1, &[], Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    engine.keep(10_000);
    assert_eq!(engine.lead(), 24_000);
    // Kept at 48 000 frames: 47 blocks of 1 024 (48 128) before it is full.
    engine.keep(48_000);
    let mut blocks = 0;
    while engine.due() {
        engine.render();
        blocks += 1;
    }
    assert_eq!(blocks, 47);
    // Narrowed again: back to the 24 000 it was made with, and nothing rendered is let go — the
    // output has to play 48 128 - 24 000 = 24 128 frames before another block is due.
    engine.keep(0);
    assert_eq!(engine.lead(), 24_000);
    engine.played(1, 24_128);
    assert!(!engine.due());
    engine.played(1, 24_129);
    assert!(engine.due());
}

#[test]
fn a_reopened_output_goes_on_from_the_frontier_in_the_same_start_of_play() {
    let mut engine = engine();
    engine.play(
        2,
        handed(1, &record(2.0, 0.0, 0.0, 69.0), Vec::new(), Cast::new()),
        Pace::Natural,
        unity(),
    );
    assert_eq!(engine.take_flush(), Some(2));
    // The 24 000-frame lead: 24 blocks of 1 024, to frame 24 576.
    while engine.due() {
        engine.render();
    }
    assert_eq!(engine.frontier(), 24_576);
    // The output played 2 048 of them, then died with the rest.
    engine.played(2, 2_048);
    engine.reopened();
    // A flush in start of play 2 for the new output, and the lead counted from 24 576: 24 more
    // blocks, the first from frame 24 576 (nothing re-rendered, nothing skipped in the piece).
    assert_eq!(engine.take_flush(), Some(2));
    assert_eq!(engine.generation(), 2);
    assert!(engine.playing());
    let mut first = None;
    let mut blocks = 0;
    while engine.due() {
        let (frame, _) = engine.render();
        first.get_or_insert(frame);
        blocks += 1;
    }
    assert_eq!((first, blocks), (Some(24_576), 24));
}
