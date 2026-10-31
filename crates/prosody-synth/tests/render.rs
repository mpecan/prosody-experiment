//! The renderer's clock: where notes start, how a piece loops round, where an edit takes over, and
//! what is counted when something cannot sound. Every expected frame below is worked out by hand
//! from the note times and the 8 kHz rate.
// A test may unwrap and index (CLAUDE.md §3); its frame counts and samples are small enough to cast.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use prosody_synth::{
    Bank, Channel, Compression, Crush, Echo, Effects, Gain, Hertz, Midi, Note, NoteSpec, Piece,
    Renderer, Repeat, STEMS, Sample, SampleRate, Seconds, Shape, Source, Stems, Utterance, VOICES,
    Vowel,
};

const RATE: u32 = 8_000;

fn rate() -> SampleRate {
    SampleRate::new(RATE).unwrap()
}

fn seconds(value: f64) -> Seconds {
    Seconds::new(value).unwrap()
}

/// A drum note: instrument 0, at `onset`, on `channel`.
fn hit(tag: u32, onset: f64, channel: Channel) -> Note {
    Note::new(NoteSpec {
        tag,
        channel,
        source: Source::Drum(0),
        onset: seconds(onset),
        length: seconds(0.1),
        pitch: None,
        gain: Gain::new(1.0).unwrap(),
        postgain: Gain::new(1.0).unwrap(),
        attack: Seconds::new(0.0),
        release: None,
        lpf: Hertz::new(20_000.0).unwrap(),
        effects: Effects::DRY,
    })
    .unwrap()
}

/// A bank whose drum 0 is ten frames of full level: a burst that is easy to find.
fn burst_bank(frames: usize) -> Bank {
    let mut bank = Bank::new();
    bank.set_drum(
        0,
        Sample::new(vec![1.0; frames], None, f64::from(RATE)).unwrap(),
    )
    .unwrap();
    bank
}

fn piece(notes: Vec<Note>, bars: &[f64], length: f64, repeat: Repeat) -> Piece {
    let bars: Vec<Seconds> = bars.iter().map(|at| seconds(*at)).collect();
    Piece::new(notes, &bars, seconds(length), repeat, rate())
}

/// Renders `blocks` blocks of 100 frames, and returns each channel's left side, end to end.
fn run(renderer: &mut Renderer, blocks: usize) -> Vec<Vec<f32>> {
    let mut stems = Stems::new(100);
    let mut out = vec![Vec::new(); STEMS];
    for _ in 0..blocks {
        renderer.render(&mut stems);
        for channel in Channel::ALL {
            out[channel.index()].extend_from_slice(stems.side(channel, false));
        }
    }
    out
}

/// The frames where a burst starts: sound after silence.
fn onsets(side: &[f32]) -> Vec<usize> {
    (0..side.len())
        .filter(|&n| side[n].abs() > 1e-6 && (n == 0 || side[n - 1].abs() <= 1e-6))
        .collect()
}

#[test]
fn a_looping_piece_plays_its_notes_every_time_round() {
    // A note at 0.25 s of a one-second piece: frame 2 000, then 10 000 and 18 000.
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(
        vec![hit(0, 0.25, Channel::Drums)],
        &[0.0],
        1.0,
        Repeat::Loop,
    ));
    let out = run(&mut renderer, 200);
    assert_eq!(onsets(&out[0]), vec![2_000, 10_000, 18_000]);
    // The burst is ten frames long and then the voice is gone.
    assert!(out[0][2_009].abs() > 0.5);
    assert!(out[0][2_010].abs() < 1e-9);
}

#[test]
fn a_piece_played_once_does_not_come_round_again() {
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(
        vec![hit(0, 0.25, Channel::Drums)],
        &[0.0],
        1.0,
        Repeat::Once,
    ));
    let out = run(&mut renderer, 200);
    assert_eq!(onsets(&out[0]), vec![2_000]);
}

#[test]
fn an_edit_takes_over_at_the_next_bar_from_the_same_bar_of_the_new_piece() {
    // Piece A has bars every quarter second (frames 0, 2 000, 4 000, 6 000) and drum notes at
    // 0.7 s (frame 5 600) and 0.85 s (frame 6 800). Piece B has bars at 0, 0.3, 0.6 and 0.9 s
    // (frames 0, 2 400, 4 800, 7 200) and a bass note at 0.95 s (frame 7 600).
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(
        vec![hit(0, 0.7, Channel::Drums), hit(1, 0.85, Channel::Drums)],
        &[0.0, 0.25, 0.5, 0.75],
        1.0,
        Repeat::Loop,
    ));
    let mut first = run(&mut renderer, 50);
    assert_eq!(renderer.frontier(), 5_000);
    // At frame 5 000 A is in its bar 2; its next bar, bar 3, starts at frame 6 000. B takes over
    // there from its own bar 3, frame 7 200.
    let placement = renderer.swap(piece(
        vec![hit(0, 0.95, Channel::Bass)],
        &[0.0, 0.3, 0.6, 0.9],
        1.0,
        Repeat::Loop,
    ));
    assert_eq!((placement.out_start, placement.piece_start), (6_000, 7_200));
    let rest = run(&mut renderer, 150);
    for (channel, more) in first.iter_mut().zip(rest) {
        channel.extend(more);
    }
    // A's note at 5 600 still plays (before the bar); its note at 6 800 does not (after it).
    // B's note at its frame 7 600 is 400 frames into B's part: output frame 6 400, and once round
    // B's 8 000 frames later, 14 400.
    assert_eq!(onsets(&first[Channel::Drums.index()]), vec![5_600]);
    assert_eq!(onsets(&first[Channel::Bass.index()]), vec![6_400, 14_400]);
}

#[test]
fn an_edit_at_a_bar_boundary_takes_over_there() {
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(vec![], &[0.0, 0.5], 1.0, Repeat::Loop));
    run(&mut renderer, 40); // frontier 4 000: bar 1 starts exactly there
    let placement = renderer.swap(piece(vec![], &[0.0, 0.25], 1.0, Repeat::Loop));
    assert_eq!((placement.out_start, placement.piece_start), (4_000, 2_000));
}

#[test]
fn an_edit_past_the_last_bar_takes_over_when_the_piece_comes_round() {
    // Frontier 7 000, in the last bar (it starts at 4 000): the next boundary is the loop's end,
    // frame 8 000, and the new piece starts from its top.
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(vec![], &[0.0, 0.5], 1.0, Repeat::Loop));
    run(&mut renderer, 70);
    let placement = renderer.swap(piece(vec![], &[0.0, 0.5, 0.75], 2.0, Repeat::Loop));
    assert_eq!((placement.out_start, placement.piece_start), (8_000, 0));
}

#[test]
fn a_new_piece_with_fewer_bars_carries_on_from_that_bar_counted_round() {
    // A has four bars, B two. At frame 2 500 A's next bar is bar 2 (frame 4 000); B's bar 2 counted
    // round its two is bar 0.
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    renderer.play(piece(vec![], &[0.0, 0.25, 0.5, 0.75], 1.0, Repeat::Loop));
    run(&mut renderer, 25);
    let placement = renderer.swap(piece(vec![], &[0.0, 0.5], 1.0, Repeat::Loop));
    assert_eq!((placement.out_start, placement.piece_start), (4_000, 0));
}

#[test]
fn a_full_pool_steals_and_says_so() {
    // One more drum than there are voices, all at once, each a second long: one is stolen.
    let notes: Vec<Note> = (0..=VOICES)
        .map(|tag| hit(u32::try_from(tag).unwrap(), 0.0, Channel::Drums))
        .collect();
    let mut renderer = Renderer::new(rate(), burst_bank(8_000));
    renderer.play(piece(notes, &[0.0], 4.0, Repeat::Once));
    run(&mut renderer, 1);
    assert_eq!(renderer.losses().stolen, 1);
    assert_eq!(renderer.sounding(), VOICES);
    // Every voice is attributable to the note it plays: the first was stolen, by the last.
    let mut tags = renderer.sounding_tags();
    tags.sort_unstable();
    assert_eq!(
        tags,
        (1..=u32::try_from(VOICES).unwrap()).collect::<Vec<_>>()
    );
}

#[test]
fn a_note_whose_sound_is_not_loaded_is_counted_not_guessed() {
    let mut renderer = Renderer::new(rate(), Bank::new());
    let font = Note::new(NoteSpec {
        source: Source::Font(3),
        pitch: Midi::new(60.0),
        ..*hit(0, 0.0, Channel::Melody).spec()
    })
    .unwrap();
    renderer.play(piece(
        vec![hit(1, 0.0, Channel::Drums), font],
        &[0.0],
        1.0,
        Repeat::Once,
    ));
    let out = run(&mut renderer, 10);
    assert_eq!(renderer.losses().missing, 2);
    assert!(out.iter().all(|side| side.iter().all(|x| *x == 0.0)));
}

#[test]
fn stopping_silences_everything_at_once() {
    let mut renderer = Renderer::new(rate(), burst_bank(8_000));
    renderer.play(piece(
        vec![hit(0, 0.0, Channel::Drums)],
        &[0.0],
        1.0,
        Repeat::Loop,
    ));
    run(&mut renderer, 5);
    renderer.stop();
    let out = run(&mut renderer, 5);
    assert!(out[0].iter().all(|x| *x == 0.0));
    assert_eq!(renderer.sounding(), 0);
}

#[test]
fn the_same_piece_renders_the_same_samples() {
    // I1, extended to audio (docs/player.md §2): two renderers, one piece, identical bits.
    let make = || {
        let mut renderer = Renderer::new(rate(), burst_bank(300));
        renderer.play(piece(
            (0..20)
                .map(|tag| hit(tag, f64::from(tag) * 0.037, Channel::Drums))
                .collect(),
            &[0.0, 0.5],
            1.0,
            Repeat::Loop,
        ));
        run(&mut renderer, 120)
    };
    let (a, b) = (make(), make());
    assert_eq!(bits(&a), bits(&b));
}

/// Renders `frames` frames in blocks of `block`, and returns both sides of every stem, end to end.
fn run_blocks(renderer: &mut Renderer, block: usize, frames: usize) -> Vec<Vec<f32>> {
    let mut stems = Stems::new(block);
    let mut out = vec![Vec::new(); 2 * STEMS];
    for _ in 0..frames / block {
        renderer.render(&mut stems);
        for channel in Channel::ALL {
            out[2 * channel.index()].extend_from_slice(stems.side(channel, false));
            out[2 * channel.index() + 1].extend_from_slice(stems.side(channel, true));
        }
    }
    out
}

fn bits(out: &[Vec<f32>]) -> Vec<u32> {
    out.iter().flatten().map(|x| x.to_bits()).collect()
}

/// `note` with `effects`.
fn with(note: &Note, effects: Effects) -> Note {
    Note::new(NoteSpec {
        effects,
        ..*note.spec()
    })
    .unwrap()
}

fn compressed(threshold: f64, ratio: f64) -> Effects {
    Effects {
        compression: Compression::new(threshold, ratio),
        ..Effects::DRY
    }
}

/// A dry, looping piece of one drum at its top: what each history below is compared on.
fn dry_piece() -> Piece {
    piece(vec![hit(0, 0.0, Channel::Drums)], &[0.0], 1.0, Repeat::Loop)
}

#[test]
fn a_piece_after_a_stop_sounds_as_it_does_on_a_fresh_renderer() {
    // A compressed piece, stopped, then a dry one: the dry piece's stems are the bits a renderer
    // that had played nothing gives it. A compressor kept from the first piece would lift the dry
    // drum by its makeup (9.77 dB at −24 dB, 7:1).
    let mut fresh = Renderer::new(rate(), burst_bank(300));
    fresh.play(dry_piece());
    let expected = run_blocks(&mut fresh, 100, 4_000);
    let mut used = Renderer::new(rate(), burst_bank(300));
    used.play(piece(
        vec![with(&hit(0, 0.0, Channel::Drums), compressed(-24.0, 7.0))],
        &[0.0],
        1.0,
        Repeat::Loop,
    ));
    run_blocks(&mut used, 100, 2_000);
    used.stop();
    used.play(dry_piece());
    assert_eq!(bits(&run_blocks(&mut used, 100, 4_000)), bits(&expected));
}

#[test]
fn a_dry_edit_after_a_compressed_piece_sounds_as_it_does_on_a_fresh_renderer() {
    // The compressed piece has one bar, so an edit at frame 3 200 takes over when it comes round,
    // at frame 8 000, from the dry piece's top. Its 300-frame drum ended long before, so from 8 000
    // on the stems must be the dry piece's own, as a fresh renderer plays it from frame 0.
    let mut fresh = Renderer::new(rate(), burst_bank(300));
    fresh.play(dry_piece());
    let expected = run_blocks(&mut fresh, 100, 4_000);
    let mut used = Renderer::new(rate(), burst_bank(300));
    used.play(piece(
        vec![with(&hit(0, 0.0, Channel::Drums), compressed(-24.0, 7.0))],
        &[0.0],
        1.0,
        Repeat::Loop,
    ));
    run_blocks(&mut used, 100, 3_200);
    let placement = used.swap(dry_piece());
    assert_eq!((placement.out_start, placement.piece_start), (8_000, 0));
    run_blocks(&mut used, 100, 4_800);
    assert_eq!(bits(&run_blocks(&mut used, 100, 4_000)), bits(&expected));
}

#[test]
fn an_edit_that_changes_a_bus_sounds_the_same_whatever_the_block_size() {
    // A one-second drum, compressed at −24 dB 7:1 and echoed every 0.25 s, is still sounding when
    // an edit takes over at frame 4 000 (A's bar 1, from B's bar 1, frame 4 000). B's drum at its
    // 0.55625 s, output frame 4 450, sets the channel's compressor to −10 dB 2:1 and its echo to
    // 0.3 s. Frame 4 450 falls inside a block at both 100 frames (4 400..4 500) and 128
    // (4 352..4 480); the change must land on frame 4 450 either way, so the bits are the same.
    let render = |block: usize| {
        let mut renderer = Renderer::new(rate(), burst_bank(8_000));
        let mut first = compressed(-24.0, 7.0);
        first.echo = Echo::new(Gain::new(0.5).unwrap(), seconds(0.25), 0.35);
        let mut second = compressed(-10.0, 2.0);
        second.echo = Echo::new(Gain::new(0.5).unwrap(), seconds(0.3), 0.35);
        renderer.play(piece(
            vec![with(&hit(0, 0.0, Channel::Drums), first)],
            &[0.0, 0.5],
            1.0,
            Repeat::Loop,
        ));
        let mut out = run_blocks(&mut renderer, block, 3_200);
        let placement = renderer.swap(piece(
            vec![with(&hit(0, 0.556_25, Channel::Drums), second)],
            &[0.0, 0.5],
            1.0,
            Repeat::Loop,
        ));
        assert_eq!((placement.out_start, placement.piece_start), (4_000, 4_000));
        for (side, more) in out.iter_mut().zip(run_blocks(&mut renderer, block, 9_600)) {
            side.extend(more);
        }
        out
    };
    let (a, b) = (render(100), render(128));
    // The edit's drum did start at 4 450: the stem steps up there, by the second drum's level.
    let drums = &a[2 * Channel::Drums.index()];
    assert!((drums[4_450] - drums[4_449]).abs() > 0.1);
    assert_eq!(bits(&a), bits(&b));
}

#[test]
fn a_mono_sample_sounds_as_the_same_sample_in_stereo() {
    // A mono sample runs the low-pass and the vowel once for both sides; the same frames given as
    // a stereo sample run them on each. Through a low-pass, a vowel, a crush and a shape, the two
    // must give the same bits, on both sides.
    let wave: Vec<f32> = (0..2_000)
        .map(|n| ((n * 37 % 101) as f32 / 50.0 - 1.0) * 0.8)
        .collect();
    let render = |stereo: bool| {
        let mut bank = Bank::new();
        let right = stereo.then(|| wave.clone());
        bank.set_drum(
            0,
            Sample::new(wave.clone(), right, f64::from(RATE)).unwrap(),
        )
        .unwrap();
        let effects = Effects {
            vowel: Some(Vowel::A),
            crush: Crush::new(6.0),
            shape: Shape::new(0.4),
            ..Effects::DRY
        };
        let note = Note::new(NoteSpec {
            lpf: Hertz::new(1_500.0).unwrap(),
            effects,
            ..*hit(0, 0.0, Channel::Drums).spec()
        })
        .unwrap();
        let mut renderer = Renderer::new(rate(), bank);
        renderer.play(piece(vec![note], &[0.0], 1.0, Repeat::Once));
        run_blocks(&mut renderer, 100, 3_000)
    };
    let (mono, stereo) = (render(false), render(true));
    let drums = 2 * Channel::Drums.index();
    assert!(mono[drums].iter().any(|x| x.abs() > 0.01));
    assert_eq!(bits(&mono), bits(&stereo));
    assert_eq!(
        bits(&mono[drums..=drums]),
        bits(&mono[drums + 1..=drums + 1])
    );
}

#[test]
fn a_line_of_the_reading_sounds_on_its_own_stem_over_the_music_and_ducks_it() {
    // A drum hit every 0.1 s (frames 0, 800, …) and a line of 50 frames placed at frame 250, its
    // speech from 260 (a 10-frame margin). The drums keep their frames; the line is on the speech
    // stem only, from 250 to 300; the duck starts falling at 260, not before.
    let mut renderer = Renderer::new(rate(), burst_bank(10));
    let hits = (0..4)
        .map(|n| hit(n, 0.1 * f64::from(n), Channel::Drums))
        .collect();
    renderer.play(piece(hits, &[0.0], 0.4, Repeat::Once));
    renderer
        .add_line(
            3,
            Sample::new(vec![0.5; 50], None, f64::from(RATE)).unwrap(),
        )
        .unwrap();
    renderer.set_duck(Gain::new(0.6).unwrap());
    let spoken = renderer
        .speak(Utterance {
            line: 3,
            start: 250,
            speech: 260,
            hold: 0,
        })
        .unwrap();
    assert_eq!((spoken.start, spoken.end), (250, 300));
    let mut stems = Stems::new(400);
    renderer.render(&mut stems);
    assert_eq!(onsets(stems.side(Channel::Drums, false)), vec![0]);
    let speech = stems.side(Channel::Speech, false);
    assert_eq!(onsets(speech), vec![250]);
    assert!(speech[250..300].iter().all(|x| (*x - 0.5).abs() < 1e-6));
    assert!(speech[300..].iter().all(|x| *x == 0.0));
    assert!(
        stems.side(Channel::Speech, true)[250..300]
            .iter()
            .all(|x| (*x - 0.5).abs() < 1e-6)
    );
    let duck = stems.duck();
    assert!(duck[..260].iter().all(|x| (*x - 1.0).abs() < 1e-9));
    assert!(duck[260] < 1.0);
    // The music's stems are the bits they are without the reading: the duck is the master's to
    // apply, not the stems'.
    let mut alone = Renderer::new(rate(), burst_bank(10));
    let hits = (0..4)
        .map(|n| hit(n, 0.1 * f64::from(n), Channel::Drums))
        .collect();
    alone.play(piece(hits, &[0.0], 0.4, Repeat::Once));
    let mut without = Stems::new(400);
    alone.render(&mut without);
    assert_eq!(
        stems.side(Channel::Drums, false),
        without.side(Channel::Drums, false)
    );
}

#[test]
fn a_stop_lets_the_reading_go_with_the_music() {
    let mut renderer = Renderer::new(rate(), Bank::new());
    renderer
        .add_line(
            0,
            Sample::new(vec![0.5; 1_000], None, f64::from(RATE)).unwrap(),
        )
        .unwrap();
    renderer.set_duck(Gain::new(0.6).unwrap());
    renderer
        .speak(Utterance {
            line: 0,
            start: 0,
            speech: 0,
            hold: 0,
        })
        .unwrap();
    let mut stems = Stems::new(100);
    renderer.render(&mut stems);
    assert!(stems.side(Channel::Speech, false).iter().all(|x| *x > 0.4));
    renderer.stop();
    renderer.render(&mut stems);
    assert!(stems.side(Channel::Speech, false).iter().all(|x| *x == 0.0));
    assert!(stems.duck().iter().all(|x| (*x - 1.0).abs() < 1e-9));
}
