//! The output's mix, worked by hand: the same cases and numbers as the worklet's
//! (`web/test/player.test.ts`, "the worklet mixer"), so the two stay one mix.

use super::{Block, FADE, GLIDE, Mixer};
use prosody_synth::Stems;

/// The glue's makeup: every level below is far under its −6 dB threshold, so the glue is its makeup
/// alone, 10^(1.575/20) = 1.19881, from its first frame.
const MAKEUP: f64 = 1.198_809_014_5;
const ALL: [f64; 5] = [1.0; 5];

/// A block of `frames` frames whose stem `stem` holds `left` and `right` throughout, at `duck`.
fn block(frame: u64, frames: usize, stem: usize, left: f32, right: f32, duck: f32) -> Block {
    let mut data = vec![0.0; Stems::PER_FRAME * frames];
    data[stem * 2 * frames..stem * 2 * frames + frames].fill(left);
    data[stem * 2 * frames + frames..(stem + 1) * 2 * frames].fill(right);
    data[10 * frames..11 * frames].fill(duck);
    Block {
        frame,
        generation: 0,
        data,
    }
}

/// Mixes `frames` frames, stereo, at `gains`; gives left and right.
fn mix(mixer: &mut Mixer, frames: usize, gains: [f64; 5]) -> (Vec<f64>, Vec<f64>) {
    mixer.set_gains(gains);
    let mut out = vec![0.0_f32; 2 * frames];
    mixer.mix(&mut out, 2);
    let left = out.iter().step_by(2).map(|x| f64::from(*x)).collect();
    let right = out
        .iter()
        .skip(1)
        .step_by(2)
        .map(|x| f64::from(*x))
        .collect();
    (left, right)
}

fn close(actual: f64, expected: f64, within: f64) {
    assert!(
        (actual - expected).abs() < within,
        "{actual} is not {expected}"
    );
}

#[test]
fn it_mixes_the_stems_at_their_gains_and_counts_from_the_first_block() {
    let mut mixer = Mixer::new(48_000.0);
    mixer.push(block(1_000, 4, 0, 0.1, 0.2, 1.0));
    let (left, right) = mix(&mut mixer, 2, ALL);
    close(left[0], 0.1 * MAKEUP, 1e-6);
    close(left[1], 0.1 * MAKEUP, 1e-6);
    close(right[0], 0.2 * MAKEUP, 1e-6);
    let report = mixer.report();
    assert_eq!((report.played, report.underruns), (1_002, 0));
}

#[test]
fn it_glides_a_stem_to_a_new_gain_and_starts_at_the_gains_asked_for() {
    let mut mixer = Mixer::new(48_000.0);
    // A stem muted before play is silent from its first frame: nothing sounded to glide from.
    mixer.push(block(0, 1_000, 0, 0.2, 0.2, 1.0));
    let (two, _) = mix(&mut mixer, 2, [0.0, 1.0, 1.0, 1.0, 1.0]);
    assert_eq!(two, vec![0.0, 0.0]);
    // Unmuted, it rises by 1/GLIDE a frame: 0.2 × 1/256, then × 2/256 (each through the makeup).
    let glide = f64::from(GLIDE);
    let (two, _) = mix(&mut mixer, 2, ALL);
    close(two[0], 0.2 / glide * MAKEUP, 1e-7);
    close(two[1], 0.2 * 2.0 / glide * MAKEUP, 1e-7);
    // After GLIDE frames it is all the way up, and stays there.
    let (long, _) = mix(&mut mixer, 256, ALL);
    close(long[253], 0.2 * MAKEUP, 1e-7);
    close(long[255], 0.2 * MAKEUP, 1e-7);
    // Muted again: down by 1/GLIDE a frame, no step to 0.
    let (two, _) = mix(&mut mixer, 2, [0.0, 1.0, 1.0, 1.0, 1.0]);
    close(two[0], 0.2 * (glide - 1.0) / glide * MAKEUP, 1e-7);
    close(two[1], 0.2 * (glide - 2.0) / glide * MAKEUP, 1e-7);
}

#[test]
fn it_fades_what_was_queued_out_over_fade_frames_when_it_is_flushed() {
    let mut mixer = Mixer::new(48_000.0);
    mixer.push(block(0, 1_000, 1, 0.4, 0.2, 1.0));
    mix(&mut mixer, 2, ALL);
    mixer.flush(1);
    // The next FADE frames of the dropped music, falling linearly: frame k at (FADE − k)/FADE.
    let (out, right) = mix(&mut mixer, FADE + 4, ALL);
    close(out[0], 0.4 * MAKEUP, 1e-6);
    close(right[0], 0.2 * MAKEUP, 1e-6);
    close(out[FADE / 2], 0.4 * 0.5 * MAKEUP, 1e-6);
    #[allow(clippy::cast_precision_loss)] // FADE is 256
    close(out[FADE - 1], 0.4 / FADE as f64 * MAKEUP, 1e-6);
    assert_eq!(&out[FADE..], &[0.0; 4]);
    // Silence after a flush is no shortfall; the blocks dropped go back to the host.
    assert_eq!(mixer.report().underruns, 0);
    assert!(mixer.take_spent().is_some());
    // A block of the start of play before the flush is stale: heard not at all, handed back.
    mixer.push(block(70, 4, 0, 0.1, 0.1, 1.0));
    assert_eq!(mixer.queued(), 0);
    let mut fresh = block(5_000, 4, 0, 0.1, 0.1, 1.0);
    fresh.generation = 1;
    mixer.push(fresh);
    assert_eq!(mixer.report().played, 5_000);
}

#[test]
fn it_carries_across_blocks_and_counts_a_buffer_it_could_not_fill() {
    let mut mixer = Mixer::new(48_000.0);
    mixer.push(block(0, 3, 1, 0.1, 0.1, 1.0));
    mixer.push(block(3, 3, 1, 0.2, 0.2, 1.0));
    let tenths = |side: Vec<f64>| -> Vec<f64> {
        side.iter()
            .map(|x| (x / MAKEUP * 10.0).round() / 10.0)
            .collect()
    };
    let (left, _) = mix(&mut mixer, 4, ALL);
    assert_eq!(tenths(left), vec![0.1, 0.1, 0.1, 0.2]);
    assert_eq!(mixer.queued(), 2);
    let (left, _) = mix(&mut mixer, 4, ALL);
    assert_eq!(tenths(left), vec![0.2, 0.2, 0.0, 0.0]);
    let report = mixer.report();
    assert_eq!((report.underruns, report.played), (1, 6));
}

#[test]
fn it_is_not_short_of_anything_before_the_first_block_nor_after_a_flush() {
    let mut mixer = Mixer::new(48_000.0);
    mix(&mut mixer, 4, ALL);
    mixer.push(block(50, 4, 0, 0.1, 0.1, 1.0));
    mixer.flush(0);
    mix(&mut mixer, 4, ALL);
    assert_eq!(mixer.report().underruns, 0);
    // After a flush the next block says where the count starts again.
    mixer.push(block(9_000, 4, 0, 0.1, 0.1, 1.0));
    assert_eq!(mixer.report().played, 9_000);
}

#[test]
fn it_ducks_the_music_under_the_reading_and_not_the_reading() {
    // Melody 0.05 at a duck of 0.6 is 0.03; the reading's 0.05 is not ducked; together 0.08, and
    // through the makeup 0.08 × 1.19881 = 0.095905: the frame prosody_synth's master test works.
    let mut mixer = Mixer::new(48_000.0);
    let mut b = block(0, 2, 2, 0.05, 0.05, 0.6);
    b.data[16..18].fill(0.05);
    mixer.push(b);
    let (left, _) = mix(&mut mixer, 1, ALL);
    close(left[0], 0.095_905, 1e-5);
}

#[test]
fn it_ducks_the_music_not_the_reading_under_the_browser_voice_as_the_renderer_falls() {
    // The page asks for 0.6 while the browser's own voice reads. At 48 kHz the fall's time constant
    // (0.08 s) is 3 840 frames: one constant in, the music has gone 1 − 1/e of the way, to
    // 0.6 + 0.4/e = 0.747152 of itself. The reading's stem is untouched, and is what its peak sees.
    let mut mixer = Mixer::new(48_000.0);
    let mut b = block(0, 4_000, 0, 0.1, 0.1, 1.0);
    b.data[32_000..36_000].fill(0.02);
    mixer.push(b);
    mixer.set_duck(0.6);
    let (left, right) = mix(&mut mixer, 4_000, ALL);
    let e = std::f64::consts::E;
    close(left[3_839], (0.1 * (0.6 + 0.4 / e) + 0.02) * MAKEUP, 1e-5);
    assert!(right[0] < 0.12 * MAKEUP);
    close(f64::from(mixer.report().speech_peak), 0.02, 1e-6);
}

#[test]
fn one_channel_hears_both_sides_and_a_third_hears_nothing() {
    let mut mixer = Mixer::new(48_000.0);
    mixer.push(block(0, 4, 0, 0.1, 0.3, 1.0));
    let mut mono = [0.0_f32; 2];
    mixer.mix(&mut mono, 1);
    close(f64::from(mono[0]), 0.2 * MAKEUP, 1e-6);
    let mut three = [9.0_f32; 3];
    mixer.mix(&mut three, 3);
    close(f64::from(three[1]), 0.3 * MAKEUP, 1e-6);
    assert!(three[2].abs() < f32::EPSILON);
}
