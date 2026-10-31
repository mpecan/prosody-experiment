//! Golden audio (`docs/player.md` §6): fixed scenes rendered, their PCM hashed, and the hash
//! snapshotted with the scene's peak and loudness beside it, so a diff says how much changed as well
//! as that something did.
//!
//! The synth computes its own transcendental functions so these hashes are the same on every
//! platform (`prosody_synth::math`). A changed hash is a changed sound: listen before accepting it
//! (`prosody render`, or render the scene to WAV with `PROSODY_GOLDEN_WAV=dir`).
// A test may unwrap and index (CLAUDE.md §3); its frame counts and samples are small enough to cast.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation
)]

use prosody_synth::{
    Bank, Channel, Gain, Hertz, Loop, Midi, Note, NoteSpec, Piece, Renderer, Repeat, STEMS, Sample,
    SampleRate, Seconds, Source, Stems, Wave, Zone, master, wav,
};

const RATE: u32 = 48_000;

fn rate() -> SampleRate {
    SampleRate::new(RATE).unwrap()
}

#[allow(clippy::too_many_arguments)] // one note's fields, spelled out per scene
fn note(
    tag: u32,
    channel: Channel,
    source: Source,
    onset: f64,
    length: f64,
    pitch: Option<f64>,
    gain: f64,
    lpf: f64,
) -> Note {
    Note::new(NoteSpec {
        tag,
        channel,
        source,
        onset: Seconds::new(onset).unwrap(),
        length: Seconds::new(length).unwrap(),
        pitch: pitch.and_then(Midi::new),
        gain: Gain::new(gain).unwrap(),
        postgain: Gain::new(1.0).unwrap(),
        attack: Seconds::new(0.005),
        release: Seconds::new(0.15),
        lpf: Hertz::new(lpf).unwrap(),
    })
    .unwrap()
}

/// A deterministic noise burst decaying over `frames`: integer noise (a 32-bit xorshift), scaled
/// and shaped with nothing but multiplication, so it is the same everywhere.
fn burst(frames: usize, seed: u32) -> Vec<f32> {
    let mut state = seed;
    (0..frames)
        .map(|n| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = f64::from(state) / f64::from(u32::MAX) * 2.0 - 1.0;
            let fall = 1.0 - n as f64 / frames as f64;
            (noise * fall * fall) as f32
        })
        .collect()
}

/// A plucked tone for a soundfont zone: a sine at `cycles` per `frames`-frame sample, from the
/// synth's own sine.
fn tone(frames: usize, period: f64) -> Vec<f32> {
    (0..frames)
        .map(|n| {
            let phase = std::f64::consts::TAU * n as f64 / period;
            (0.6 * prosody_synth::math::sin(phase)) as f32
        })
        .collect()
}

/// Renders `notes` once, mixed through the master, and describes it: hash, peak, RMS.
fn describe(name: &str, bank: Bank, notes: Vec<Note>, seconds: f64) -> String {
    let mut renderer = Renderer::new(rate(), bank);
    let length = Seconds::new(seconds).unwrap();
    renderer.play(Piece::new(notes, &[], length, Repeat::Once, rate()));
    let mut stems = Stems::new(1_024);
    let mut pcm = Vec::new();
    let frames = length.frames(rate()) + u64::from(RATE) / 2;
    while renderer.frontier() < frames {
        renderer.render(&mut stems);
        master::mix(&stems, [1.0; STEMS], &mut pcm);
    }
    if let Some(dir) = std::env::var_os("PROSODY_GOLDEN_WAV") {
        let path = std::path::Path::new(&dir).join(format!("{name}.wav"));
        std::fs::write(path, wav::encode(&pcm, RATE).unwrap()).unwrap();
    }
    let bytes: Vec<u8> = pcm.iter().flat_map(|x| x.to_le_bytes()).collect();
    let peak = pcm.iter().fold(0.0_f32, |peak, x| peak.max(x.abs()));
    let rms = (pcm.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt();
    format!(
        "{name}: {} frames, peak {peak:.4}, rms {rms:.4}, stolen {}, missing {}\nblake3 {}\n",
        pcm.len() / 2,
        renderer.losses().stolen,
        renderer.losses().missing,
        blake3::hash(&bytes).to_hex()
    )
}

#[test]
fn oscillators() {
    // Each wave plays a rising arpeggio, A2 to A5, on bass and melody, one with its filter open
    // and one closed down to 800 Hz.
    let mut notes = Vec::new();
    let mut tag = 0;
    for (w, wave) in [Wave::Sine, Wave::Sawtooth, Wave::Square, Wave::Triangle]
        .into_iter()
        .enumerate()
    {
        for (i, pitch) in [45.0, 57.0, 64.0, 69.0, 81.0].into_iter().enumerate() {
            let onset = w as f64 * 1.0 + i as f64 * 0.18;
            let lpf = if i % 2 == 0 { 18_000.0 } else { 800.0 };
            let channel = if pitch < 60.0 {
                Channel::Bass
            } else {
                Channel::Melody
            };
            notes.push(note(
                tag,
                channel,
                Source::Wave(wave),
                onset,
                0.15,
                Some(pitch),
                0.4,
                lpf,
            ));
            tag += 1;
        }
    }
    insta::assert_snapshot!(describe("oscillators", Bank::new(), notes, 4.0));
}

#[test]
fn drums() {
    // Three drums made of decaying noise — a long low one through a 400 Hz filter (a kick's
    // weight), a short bright one, a mid one — on a two-bar groove, with stereo on the snare.
    let mut bank = Bank::new();
    bank.set_drum(0, Sample::new(burst(12_000, 1), None, 48_000.0).unwrap())
        .unwrap();
    bank.set_drum(1, Sample::new(burst(2_400, 2), None, 44_100.0).unwrap())
        .unwrap();
    bank.set_drum(
        2,
        Sample::new(burst(6_000, 3), Some(burst(6_000, 4)), 48_000.0).unwrap(),
    )
    .unwrap();
    let mut notes = Vec::new();
    for step in 0_u32..16 {
        let at = f64::from(step) * 0.125;
        notes.push(note(
            step * 3,
            Channel::Drums,
            Source::Drum(1),
            at,
            0.1,
            None,
            0.3,
            12_000.0,
        ));
        if step % 4 == 0 {
            notes.push(note(
                step * 3 + 1,
                Channel::Drums,
                Source::Drum(0),
                at,
                0.1,
                None,
                0.9,
                400.0,
            ));
        }
        if step % 8 == 4 {
            notes.push(note(
                step * 3 + 2,
                Channel::Drums,
                Source::Drum(2),
                at,
                0.1,
                None,
                0.6,
                6_000.0,
            ));
        }
    }
    insta::assert_snapshot!(describe("drums", bank, notes, 2.0));
}

#[test]
fn soundfont_zones() {
    // Two zones of one instrument: a sine at 220 Hz for the low keys (root A3, 57) and one at
    // 440 Hz for the high (root A4, 69), each looping its last 2 400 frames, a whole number of
    // periods, so the loop is seamless. Notes held longer than the samples keep sounding.
    let mut bank = Bank::new();
    let low = Sample::new(tone(9_600, 48_000.0 / 220.0), None, 48_000.0).unwrap();
    let high = Sample::new(tone(9_600, 48_000.0 / 440.0), None, 48_000.0).unwrap();
    bank.add_zone(
        0,
        0,
        Zone::new(low, 57.0, 0, 62, Loop::new(7_200.0, 9_600.0, 9_600)).unwrap(),
    )
    .unwrap();
    bank.add_zone(
        0,
        1,
        Zone::new(high, 69.0, 63, 127, Loop::new(7_200.0, 9_600.0, 9_600)).unwrap(),
    )
    .unwrap();
    let notes = [50.0, 57.0, 62.0, 64.0, 69.0, 76.0, 69.3]
        .into_iter()
        .enumerate()
        .map(|(i, pitch)| {
            let tag = u32::try_from(i).unwrap();
            note(
                tag,
                Channel::Chords,
                Source::Font(0),
                i as f64 * 0.3,
                0.5,
                Some(pitch),
                0.5,
                9_000.0,
            )
        })
        .collect();
    insta::assert_snapshot!(describe("soundfont_zones", bank, notes, 2.6));
}
