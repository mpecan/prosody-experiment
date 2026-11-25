//! The reading as the player times it: which line is placed where on the output's clock, at both
//! paces, worked out by hand. The clock runs at 1 000 frames a second so a frame is a millisecond;
//! every piece below has bars a second long. (These were the render worker's TypeScript tests until
//! the reader moved here in step 5; the cases and their numbers are the same.)

use std::collections::BTreeMap;

use prosody_synth::Seconds;

use super::{Cast, Cue, Fitted, Held, Natural, Onset, Pace, Reader, Segment, Told, Voice};

const RATE: u32 = 1_000;

/// A voice that records what it is asked, every recording `length` frames long, refusing the line
/// held as `refuse`.
struct Recorder {
    spoken: Vec<[u64; 4]>,
    cuts: Vec<u64>,
    told: Vec<Told>,
    length: u64,
    refuse: Option<u32>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            spoken: Vec::new(),
            cuts: Vec::new(),
            told: Vec::new(),
            length: 600,
            refuse: None,
        }
    }

    fn spoken_lines(&self) -> Vec<u32> {
        self.told
            .iter()
            .filter_map(|told| match told {
                Told::Spoken { line, .. } => Some(*line),
                _ => None,
            })
            .collect()
    }

    fn says(&self) -> Vec<Told> {
        self.told
            .iter()
            .filter(|told| matches!(told, Told::Say { .. }))
            .cloned()
            .collect()
    }

    fn notes(&self) -> Vec<Told> {
        self.told
            .iter()
            .filter(|told| matches!(told, Told::Note { .. }))
            .cloned()
            .collect()
    }
}

impl Voice for Recorder {
    fn speak(&mut self, id: u32, start: u64, speech: u64, hold: u64) -> Option<u64> {
        if self.refuse == Some(id) {
            return None;
        }
        self.spoken.push([u64::from(id), start, speech, hold]);
        Some(start + self.length)
    }
    fn cut(&mut self, at: u64) {
        self.cuts.push(at);
    }
    fn tell(&mut self, told: Told) {
        self.told.push(told);
    }
}

fn s(seconds: f64) -> Seconds {
    Seconds::new(seconds).unwrap()
}

fn fitted(line: u32, slot: u32, at: f64) -> Cue {
    Cue {
        line,
        stanza: 0,
        pause: s(0.25),
        fitted: Some(Fitted { slot, at: s(at) }),
        natural: None,
    }
}

fn natural(line: u32, stanza: u32, pause: f64, onset: Onset, at: Option<f64>) -> Cue {
    Cue {
        line,
        stanza,
        pause: s(pause),
        fitted: None,
        natural: Some(Natural {
            onset,
            at: at.map(s),
        }),
    }
}

fn cast(entries: &[(u32, Option<&str>)]) -> Cast {
    entries
        .iter()
        .map(|(line, key)| (*line, key.map(str::to_owned)))
        .collect()
}

/// A four-second piece of one-second bars, serial 1, from the top.
fn segment(cues: Vec<Cue>, cast: Cast) -> Segment {
    Segment {
        serial: 1,
        out_start: 0,
        piece_start: 0,
        bars: vec![0, 1_000, 2_000, 3_000, 4_000],
        cues,
        cast,
    }
}

fn hold(reader: &mut Reader, key: &str, id: u32, lead: f64, seconds: f64) {
    reader.held.insert(
        key.to_owned(),
        Held {
            id,
            lead: s(lead),
            seconds,
        },
    );
}

/// Renders `from..to` in blocks of 500 frames, as the engine does.
fn run(reader: &mut Reader, voice: &mut Recorder, from: u64, to: u64) {
    let mut at = from;
    while at < to {
        reader.before(at, at + 500, voice);
        reader.at(at + 500);
        at += 500;
    }
}

// ---- The fitted pace ----

fn two_fitted() -> Vec<Cue> {
    vec![fitted(0, 0, 0.0), fitted(1, 2, 2.0)]
}

#[test]
fn fitted_places_a_line_on_its_bar_its_margin_before_it_and_cuts_the_line_before() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Fitted;
    hold(&mut reader, "k0", 7, 0.0, 0.6);
    hold(&mut reader, "k1", 8, 0.01, 1.5);
    reader.play(segment(
        two_fitted(),
        cast(&[(0, Some("k0")), (1, Some("k1"))]),
    ));
    run(&mut reader, &mut v, 0, 3_000);
    // Line 0 on bar 1 (frame 0); line 1 on bar 3 (frame 2 000), its recording 10 ms early.
    assert_eq!(v.spoken, vec![[7, 0, 0, 0], [8, 1_990, 2_000, 0]]);
    assert_eq!(v.cuts, vec![0, 2_000]);
    // Line 0's 0.6 s is 0.4 s short of its one-second bar; line 1's 1.48 s of speech runs 0.48 s
    // past its bar, and says so.
    let over: Vec<f64> = v
        .told
        .iter()
        .filter_map(|told| match told {
            Told::Spoken { over, .. } => *over,
            _ => None,
        })
        .collect();
    assert_eq!(over.len(), 2);
    assert!(
        (over[0] + 0.4).abs() < 1e-9 && (over[1] - 0.48).abs() < 1e-9,
        "{over:?}"
    );
}

#[test]
fn fitted_comes_round_with_the_piece() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Fitted;
    hold(&mut reader, "k0", 7, 0.0, 0.6);
    reader.play(segment(vec![fitted(0, 0, 0.0)], cast(&[(0, Some("k0"))])));
    run(&mut reader, &mut v, 0, 9_000);
    // Every 4 s.
    let starts: Vec<u64> = v.spoken.iter().map(|spoken| spoken[1]).collect();
    assert_eq!(starts, vec![0, 4_000, 8_000]);
}

#[test]
fn fitted_hands_the_browser_voice_its_line_when_due_and_a_line_not_rendered_as_late() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Fitted;
    reader.play(segment(two_fitted(), cast(&[(0, None), (1, Some("k1"))])));
    run(&mut reader, &mut v, 0, 3_000);
    assert!(v.spoken.is_empty());
    assert_eq!(
        v.says(),
        vec![
            Told::Say {
                serial: 1,
                line: 0,
                pace: Pace::Fitted,
                at: 0,
                late: false,
                chain: 0
            },
            Told::Say {
                serial: 1,
                line: 1,
                pace: Pace::Fitted,
                at: 2_000,
                late: true,
                chain: 0
            },
        ]
    );
}

#[test]
fn fitted_reads_nothing_the_cast_leaves_out() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Fitted;
    hold(&mut reader, "k0", 7, 0.0, 0.6);
    reader.play(segment(two_fitted(), Cast::new()));
    run(&mut reader, &mut v, 0, 3_000);
    assert!(v.spoken.is_empty());
    assert!(v.says().is_empty());
}

// ---- The natural pace ----

/// Stanza 1: line 0 where its bar (1) begins, then line 1 after line 0's pause of 0.25 s. Stanza 2:
/// line 2 under `breath`, its cue on bar 3 (2 s).
fn stanzas() -> Vec<Cue> {
    vec![
        natural(0, 0, 0.25, Onset::At, Some(0.0)),
        natural(1, 0, 0.5, Onset::Follows, None),
        natural(2, 1, 1.5, Onset::Breath, Some(2.0)),
    ]
}

fn abc() -> Cast {
    cast(&[(0, Some("a")), (1, Some("b")), (2, Some("c"))])
}

fn natural_reader() -> Reader {
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Natural;
    hold(&mut reader, "a", 1, 0.0, 0.6);
    hold(&mut reader, "b", 2, 0.0, 0.6);
    hold(&mut reader, "c", 3, 0.0, 0.6);
    reader
}

#[test]
fn natural_reads_a_stanza_line_after_line_and_the_next_a_silent_bar_after_it() {
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.play(segment(stanzas(), abc()));
    run(&mut r, &mut v, 0, 3_500);
    // Line 0 at 0 s, ending at 0.6 s, held its pause (250 frames) since line 1 is in its stanza.
    // Line 1 after that pause, at 0.85 s, ending at 1.45 s; the last of its stanza, so no hold.
    // Line 2's cue is at 2 s, but under `breath` a whole bar must be silent after bar 2 (where line
    // 1 ended): bar 4, at 3 s.
    assert_eq!(
        v.spoken,
        vec![[1, 0, 0, 250], [2, 850, 850, 0], [3, 3_000, 3_000, 0]]
    );
}

#[test]
fn natural_waits_for_a_line_being_rendered_and_reads_it_from_where_the_render_has_got_to() {
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.held.remove("b");
    r.play(segment(stanzas(), abc()));
    run(&mut r, &mut v, 0, 1_500);
    assert_eq!(v.spoken, vec![[1, 0, 0, 250]]);
    let waits: Vec<&Told> = v
        .told
        .iter()
        .filter(|told| matches!(told, Told::Wait { .. }))
        .collect();
    assert_eq!(
        waits,
        vec![&Told::Wait {
            serial: 1,
            line: 1,
            key: "b".to_owned()
        }]
    );
    // It arrives with the frontier at 1.5 s, past the 0.85 s it was due: it starts there, placed
    // as the block from 1.5 s is rendered.
    hold(&mut r, "b", 2, 0.0, 0.6);
    r.arrived("b", &mut v);
    run(&mut r, &mut v, 1_500, 2_000);
    assert_eq!(v.spoken[1], [2, 1_500, 1_500, 0]);
    let late = v.told.iter().find_map(|told| match told {
        Told::Spoken {
            line: 1,
            start,
            late,
            ..
        } => Some((*start, *late)),
        _ => None,
    });
    let (start, late) = late.unwrap();
    assert_eq!(start, 1_500);
    assert!((late - 0.65).abs() < 1e-9, "{late}");
}

#[test]
fn natural_chains_a_line_of_the_browser_voice_on_when_the_page_says_it_has_finished() {
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.play(segment(
        stanzas(),
        cast(&[(0, None), (1, Some("b")), (2, Some("c"))]),
    ));
    run(&mut r, &mut v, 0, 500);
    let Some(Told::Say {
        line: 0,
        pace: Pace::Natural,
        at: 0,
        chain,
        ..
    }) = v.says().first().cloned()
    else {
        panic!("line 0 is not handed to the browser's voice: {:?}", v.told);
    };
    assert!(v.spoken.is_empty());
    // The page's playhead stood at 0.9 s when the voice finished: line 1 follows 0.25 s later,
    // placed with the block it begins in.
    r.said(chain, 900, &mut v);
    assert!(v.spoken.is_empty());
    run(&mut r, &mut v, 500, 1_500);
    assert_eq!(v.spoken, vec![[2, 1_150, 1_150, 0]]);
    // A word from an older chain changes nothing.
    r.said(chain + 7, 2_000, &mut v);
    assert_eq!(v.spoken.len(), 1);
}

#[test]
fn natural_places_a_long_stanza_a_line_at_a_time_as_the_block_each_begins_in_comes() {
    // Line 0 where bar 1 begins, then 99 lines each following the last by 0.25 s: line n begins at
    // n × 0.85 s. Placed all at once they would overflow the synth (MAX_PLACED, 64).
    let mut long = vec![natural(0, 0, 0.25, Onset::At, Some(0.0))];
    long.extend((1..100).map(|n| natural(n, 0, 0.25, Onset::Follows, None)));
    let mut v = Recorder::new();
    let mut r = Reader::new(RATE);
    r.pace = Pace::Natural;
    for n in 0..100 {
        hold(&mut r, &format!("k{n}"), n, 0.0, 0.6);
    }
    let everyone = long
        .iter()
        .map(|cue| (cue.line, Some(format!("k{}", cue.line))))
        .collect();
    r.play(Segment {
        bars: (0..=100).map(|n| n * 1_000).collect(),
        ..segment(long, everyone)
    });
    run(&mut r, &mut v, 0, 500);
    assert_eq!(v.spoken, vec![[0, 0, 0, 250]]);
    run(&mut r, &mut v, 500, 3_000);
    let starts: Vec<u64> = v.spoken.iter().map(|spoken| spoken[1]).collect();
    assert_eq!(starts, vec![0, 850, 1_700, 2_550]);
}

/// Lines 0, 1, 2 of one stanza, each following the last.
fn one_stanza() -> Vec<Cue> {
    vec![
        natural(0, 0, 0.25, Onset::At, Some(0.0)),
        natural(1, 0, 0.5, Onset::Follows, None),
        natural(2, 0, 0.25, Onset::Follows, None),
    ]
}

#[test]
fn natural_tells_no_line_as_spoken_that_the_synth_refused_and_goes_on_from_the_last_heard() {
    let mut v = Recorder::new();
    v.refuse = Some(2);
    let mut r = natural_reader();
    r.play(segment(one_stanza(), abc()));
    run(&mut r, &mut v, 0, 2_000);
    // Line 1 (due at 0.85 s) is not heard; line 2 follows line 0's end (0.6 s) by line 1's pause
    // (0.5 s), at 1.1 s.
    assert_eq!(v.spoken, vec![[1, 0, 0, 250], [3, 1_100, 1_100, 0]]);
    assert_eq!(v.spoken_lines(), vec![0, 2]);
}

#[test]
fn natural_goes_on_without_a_line_the_page_could_not_render_by_that_lines_pause() {
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.held.remove("b");
    r.play(segment(one_stanza(), abc()));
    run(&mut r, &mut v, 0, 1_000);
    assert_eq!(v.spoken, vec![[1, 0, 0, 250]]);
    // Line 1 never comes; line 2 follows line 0's end (0.6 s) by line 1's pause (0.5 s): 1.1 s.
    r.failed("b", &mut v);
    run(&mut r, &mut v, 1_000, 1_500);
    assert_eq!(v.spoken, vec![[1, 0, 0, 250], [3, 1_100, 1_100, 0]]);
    // A failure of a key nobody waits for changes nothing.
    r.failed("c", &mut v);
    run(&mut r, &mut v, 1_500, 2_000);
    assert_eq!(v.spoken.len(), 2);
}

#[test]
fn natural_asks_again_for_a_line_waited_on_whose_recording_the_cast_no_longer_names() {
    // Waiting on 'b' for line 1; the voice changes and line 1 is now cast as 'b2', which is held.
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.held.remove("b");
    r.play(segment(stanzas(), abc()));
    run(&mut r, &mut v, 0, 1_500);
    assert_eq!(v.spoken, vec![[1, 0, 0, 250]]);
    hold(&mut r, "b2", 5, 0.0, 0.6);
    let casts = BTreeMap::from([(1, cast(&[(0, Some("a")), (1, Some("b2")), (2, Some("c"))]))]);
    r.recast(&casts, Pace::Natural, false, &mut v);
    run(&mut r, &mut v, 1_500, 2_000);
    // Due at 0.85 s, the frontier at 1.5 s: it starts there.
    assert_eq!(v.spoken[1], [5, 1_500, 1_500, 0]);
}

#[test]
fn natural_comes_round_while_a_line_is_read_the_old_rounds_unread_said_the_new_round_read() {
    // A 4 s piece: line 0 on bar 1, read by the browser's voice; line 1 follows it. The voice is
    // still reading line 0 when the piece comes round at 4 s.
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.play(segment(
        stanzas()[..2].to_vec(),
        cast(&[(0, None), (1, Some("b"))]),
    ));
    run(&mut r, &mut v, 0, 4_500);
    let says = v.says();
    assert_eq!(says.len(), 1);
    let Some(Told::Say {
        line: 0,
        at: 0,
        chain,
        ..
    }) = says.first().cloned()
    else {
        panic!("{says:?}");
    };
    // It finishes at 4.1 s: line 1 of the first round was never read, and is said; the new round's
    // line 0 (its bar at 4 s) follows the voice's end by its pause, at 4.35 s.
    r.said(chain, 4_100, &mut v);
    assert_eq!(
        v.notes(),
        vec![Told::Note {
            text: "not read last time round: line 2, the piece came round before its turn"
                .to_owned(),
            loss: true
        }]
    );
    let says = v.says();
    assert_eq!(says.len(), 2);
    assert!(
        matches!(
            says[1],
            Told::Say {
                line: 0,
                at: 4_350,
                ..
            }
        ),
        "{says:?}"
    );
}

#[test]
fn natural_says_a_stanza_it_cannot_reach_before_the_piece_ends() {
    // Line 2's cue is on bar 4 (3 s), but line 1 ends at 1.45 s and pauses 2 s after it, to 3.45 s:
    // past bar 4's start, so line 2 would begin on bar 5 — the piece's next round. It is said, not
    // read.
    let late = vec![
        natural(0, 0, 0.25, Onset::At, Some(0.0)),
        natural(1, 0, 2.0, Onset::Follows, None),
        natural(2, 1, 0.25, Onset::Breath, Some(3.0)),
    ];
    let mut v = Recorder::new();
    let mut r = natural_reader();
    r.play(segment(late, abc()));
    run(&mut r, &mut v, 0, 3_500);
    let ids: Vec<u64> = v.spoken.iter().map(|spoken| spoken[0]).collect();
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(
        v.notes(),
        vec![Told::Note {
            text: "not read this time round: line 3, the piece ends before its turn".to_owned(),
            loss: false
        }]
    );
}

// ---- A hand-over and a change of setting ----

#[test]
fn a_hand_over_cuts_the_old_reading_at_the_new_pieces_first_frame_and_reads_the_new_from_there() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Fitted;
    hold(&mut reader, "k", 4, 0.0, 0.6);
    let line = vec![fitted(0, 0, 0.0)];
    reader.play(segment(line.clone(), cast(&[(0, Some("k"))])));
    run(&mut reader, &mut v, 0, 1_000);
    // The edit takes over at 1.25 s, from its own bar 2 (1 s in): its bar 1 next comes at 4.25 s.
    reader.swap(Segment {
        serial: 2,
        out_start: 1_250,
        piece_start: 1_000,
        ..segment(line, cast(&[(0, Some("k"))]))
    });
    run(&mut reader, &mut v, 1_000, 5_000);
    assert_eq!(v.cuts, vec![0, 1_250, 4_250]);
    let cuts: Vec<u64> = v
        .told
        .iter()
        .filter_map(|told| match told {
            Told::Cut { at } => Some(*at),
            _ => None,
        })
        .collect();
    assert_eq!(cuts, vec![0, 1_250, 4_250]);
    assert_eq!(v.spoken, vec![[4, 0, 0, 0], [4, 4_250, 4_250, 0]]);
    assert_eq!(reader.serials(), vec![2]);
}

#[test]
fn a_hand_over_keeps_the_breath_the_new_stanza_waits_its_silent_bar_after_the_old_last_line() {
    // Stanza 1 is line 0 alone (bar 1). The edit takes over at 1.5 s from its own 1.5 s; its stanza 2
    // is cued on bar 3 (2 s), but under `breath` a whole bar must be silent after the bar the old
    // piece's last line ended in. Ending at 0.6 s (bar 1), bar 3 qualifies: 2 s. Ending at 1.2 s
    // (bar 2) — and cut at the hand-over, 1.5 s, also in bar 2 — the new stanza waits for bar 4: 3 s.
    let stanzas = vec![
        natural(0, 0, 0.25, Onset::At, Some(0.0)),
        natural(1, 1, 0.25, Onset::Breath, Some(2.0)),
    ];
    let run2 = |length: u64| {
        let mut v = Recorder::new();
        v.length = length;
        let mut reader = Reader::new(RATE);
        reader.pace = Pace::Natural;
        #[allow(clippy::cast_precision_loss)] // a test's few frames
        let seconds = length as f64 / 1_000.0;
        hold(&mut reader, "a", 1, 0.0, seconds);
        hold(&mut reader, "b", 2, 0.0, 0.6);
        let ab = cast(&[(0, Some("a")), (1, Some("b"))]);
        reader.play(segment(stanzas.clone(), ab.clone()));
        run(&mut reader, &mut v, 0, 1_500);
        reader.swap(Segment {
            serial: 2,
            out_start: 1_500,
            piece_start: 1_500,
            ..segment(stanzas.clone(), ab)
        });
        run(&mut reader, &mut v, 1_500, 3_500);
        v.spoken
    };
    assert_eq!(run2(600), vec![[1, 0, 0, 0], [2, 2_000, 2_000, 0]]);
    assert_eq!(run2(1_200), vec![[1, 0, 0, 0], [2, 3_000, 3_000, 0]]);
}

#[test]
fn a_change_of_pace_lets_go_of_what_is_placed_from_the_frontier() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.pace = Pace::Natural;
    reader.play(segment(Vec::new(), Cast::new()));
    run(&mut reader, &mut v, 0, 1_000);
    reader.recast(&BTreeMap::new(), Pace::Fitted, false, &mut v);
    assert_eq!(v.cuts, vec![1_000]);
    assert_eq!(reader.pace, Pace::Fitted);
}

#[test]
fn it_wants_every_recording_the_pieces_playing_and_to_play_are_cast_with() {
    let mut v = Recorder::new();
    let mut reader = Reader::new(RATE);
    reader.play(segment(Vec::new(), cast(&[(0, Some("a")), (1, None)])));
    reader.swap(Segment {
        serial: 2,
        ..segment(Vec::new(), cast(&[(0, Some("b"))]))
    });
    let wanted: Vec<String> = reader.wanted().into_iter().collect();
    assert_eq!(wanted, vec!["a", "b"]);
    reader.recast(
        &BTreeMap::from([(1, cast(&[(0, Some("c"))]))]),
        Pace::Natural,
        false,
        &mut v,
    );
    let wanted: Vec<String> = reader.wanted().into_iter().collect();
    assert_eq!(wanted, vec!["b", "c"]);
}

#[test]
fn a_bar_boundary_is_decided_in_whole_frames() {
    // Line 0 ends exactly on bar 2's first frame (1 000): it ended in bar 2, not bar 1, so the
    // breath's silent bar is bar 3 and the next stanza begins on bar 4 (3 s) though cued on bar 3.
    let stanzas = vec![
        natural(0, 0, 0.0, Onset::At, Some(0.0)),
        natural(1, 1, 0.0, Onset::Breath, Some(2.0)),
    ];
    let mut v = Recorder::new();
    v.length = 1_000;
    let mut r = Reader::new(RATE);
    hold(&mut r, "a", 1, 0.0, 1.0);
    hold(&mut r, "b", 2, 0.0, 1.0);
    r.play(segment(stanzas, cast(&[(0, Some("a")), (1, Some("b"))])));
    run(&mut r, &mut v, 0, 3_500);
    assert_eq!(v.spoken, vec![[1, 0, 0, 0], [2, 3_000, 3_000, 0]]);
}

#[test]
fn bars_repeat_round_the_loop_and_are_found_from_a_frame_in_any_round() {
    // Bars of two, two and four seconds (frames at 1 000 a second), this round's first at 10 s.
    let chain = super::Chain {
        serial: 1,
        segment: 1,
        bars: vec![0, 2_000, 4_000, 8_000],
        lines: Vec::new(),
        next: 0,
        round: None,
        wrap_to: None,
        origin: 10_000,
        anchors: BTreeMap::new(),
        prev_end: None,
        prev_pause: 0,
        busy: super::Busy::No,
    };
    assert_eq!(chain.bar_time(0), 10_000);
    assert_eq!(chain.bar_time(2), 14_000);
    // The next round: its first bar, then its second.
    assert_eq!(chain.bar_time(3), 18_000);
    assert_eq!(chain.bar_time(4), 20_000);
    // The round before.
    assert_eq!(chain.bar_time(-1), 6_000);
    assert_eq!(chain.bar_at(10_000), 0);
    assert_eq!(chain.bar_at(15_000), 2);
    assert_eq!(chain.bar_at(19_000), 3);
    assert_eq!(chain.bar_at(9_000), -1);
    // A bar's first frame is in it; the frame before is in the bar before.
    assert_eq!(chain.bar_at(11_999), 0);
    assert_eq!(chain.bar_at(12_000), 1);
}
