//! The reading on the player's clock (`docs/player.md` §5, "The reading"): each line a voice has
//! spoken, placed on the speech stem from an output frame, and the duck the music takes under it,
//! written into the block's duck lane frame by frame from the same placements.
//!
//! The synth does not decide *when* a line is read — that is the reading's chaining, which needs
//! each line's spoken length and lives in the page's render worker — only where a line placed at a
//! frame sounds, and how the music bows under it. Both come from one list of placements, so the
//! duck cannot drift from the voice it makes room for, and both are rendered ahead with the music,
//! so a busy page can make neither late.
//!
//! The duck follows the page's Strudel-era automation (`web/src/reading`, our own code): the music
//! falls towards the duck's level from where a line's speech begins, with a time constant of
//! [`DUCK_FALL`], and rises back with one of [`DUCK_RISE`] from [`DUCK_RELEASE_DELAY`] after the
//! line ends and its hold (the pause before the stanza's next line) has passed — so the lines of a
//! stanza duck it once rather than pumping. Each is a one-pole approach to its target, as Web
//! Audio's `setTargetAtTime` is, frame by frame.

use std::sync::Arc;

use crate::SynthError;
use crate::math;
use crate::note::Channel;
use crate::sample::Sample;
use crate::stems::Stems;
use crate::units::{Gain, SampleRate};

/// The most recorded lines held at once. Lines are numbered by whoever loads them (the render
/// worker numbers them as they arrive); the cap keeps a bad number from allocating without bound.
pub const MAX_LINES: u32 = 4_096;

/// The most lines placed and not yet finished at once. The render worker places a line only when
/// the block its recording begins in is about to be rendered (`web/src/player/reader.ts`), so what is
/// placed at once is the lines sounding in that block and those whose duck has not yet risen — a
/// few; more than this is a caller's error, refused rather than allocated.
pub const MAX_PLACED: usize = 64;

/// How fast the music falls under a line: the time constant, in seconds.
pub const DUCK_FALL: f64 = 0.08;

/// How fast it rises back: the time constant, in seconds.
pub const DUCK_RISE: f64 = 0.15;

/// How long after a line (and its hold) the music starts rising back, in seconds.
pub const DUCK_RELEASE_DELAY: f64 = 0.05;

/// Frames a line that is cut off takes to fall silent: about 5 ms at 48 kHz, so a cut is not a
/// click (`web/src/player/mixer.ts` fades a flushed queue over as many).
pub const CUT_FADE: u64 = 256;

/// A line to place: recorded line `line`, its first frame on output frame `start`, its speech
/// beginning on `speech` (after the recording's quiet margin), the music kept down for `hold`
/// frames after it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utterance {
    /// The recorded line, by the number it was loaded as.
    pub line: u32,
    /// The output frame its recording begins on.
    pub start: u64,
    /// The output frame its speech begins on: where the duck begins.
    pub speech: u64,
    /// Frames the music stays down after it ends.
    pub hold: u64,
}

/// Where a placed line sounds, on the output's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spoken {
    /// Its first frame.
    pub start: u64,
    /// The frame after its last, uncut.
    pub end: u64,
}

/// A line placed, with what it plays and where it was cut.
#[derive(Debug, Clone, PartialEq)]
struct Placed {
    utterance: Utterance,
    sample: Arc<Sample>,
    /// The line's frames for each output frame.
    step: f64,
    end: u64,
    /// Where it was cut off, if it was: it fades out over [`CUT_FADE`] frames from there.
    cut: Option<u64>,
}

impl Placed {
    /// Where its sound has stopped altogether.
    fn silent_from(&self) -> u64 {
        self.cut
            .map_or(self.end, |cut| self.end.min(cut + CUT_FADE))
    }

    /// The frames the music is kept down for it, `speech..release`: from where its speech begins to
    /// `delay` after it ends and its hold has passed — or, cut, `delay` after the cut, since nothing
    /// of it is heard from there (`Renderer::cut_speech`: the music rises back from the cut). A line
    /// cut before its speech begins ducks nothing: only its quiet lead was ever heard.
    fn ducked(&self, delay: u64) -> Option<(u64, u64)> {
        let speech = self.utterance.speech;
        let release = match self.cut {
            Some(cut) if cut <= speech => return None,
            Some(cut) => (self.end + self.utterance.hold).min(cut),
            None => self.end + self.utterance.hold,
        };
        Some((speech, release + delay))
    }

    /// Whether it still has anything to do from output frame `frame` on: sound, or a duck.
    fn lasts_past(&self, frame: u64, delay: u64) -> bool {
        self.silent_from() > frame
            || self
                .ducked(delay)
                .is_some_and(|(_, release)| release > frame)
    }

    /// Its sound at output frame `frame`, which it covers.
    fn at(&self, frame: u64) -> f64 {
        #[allow(clippy::cast_precision_loss)] // a frame count far below 2^52
        let into = (frame - self.utterance.start) as f64;
        let fade = match self.cut {
            Some(cut) if frame >= cut => {
                #[allow(clippy::cast_precision_loss)] // at most CUT_FADE
                let gone = (frame - cut) as f64;
                #[allow(clippy::cast_precision_loss)]
                let fade = 1.0 - gone / CUT_FADE as f64;
                fade
            }
            _ => 1.0,
        };
        self.sample.at(into * self.step).0 * fade
    }
}

/// The reading's lines and their placements, and the duck's state.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Speech {
    rate: f64,
    lines: Vec<Option<Arc<Sample>>>,
    /// Sorted by start, then by line: the order they are summed in is total.
    placed: Vec<Placed>,
    /// The music's level under a line: 1 for no duck.
    level: f64,
    /// The duck now.
    duck: f64,
    fall: f64,
    rise: f64,
    delay: u64,
}

impl Speech {
    pub(crate) fn new(rate: SampleRate) -> Self {
        let delay =
            crate::units::Seconds::new(DUCK_RELEASE_DELAY).map_or(0, |delay| delay.frames(rate));
        let rate = f64::from(rate.get());
        Self {
            rate,
            lines: Vec::new(),
            placed: Vec::with_capacity(MAX_PLACED),
            level: 1.0,
            duck: 1.0,
            fall: math::exp(-1.0 / (DUCK_FALL * rate)),
            rise: math::exp(-1.0 / (DUCK_RISE * rate)),
            delay,
        }
    }

    /// Loads `sample` as line `id`, in place of whatever had that number. A line already placed
    /// keeps the recording it was placed with.
    pub(crate) fn add_line(&mut self, id: u32, sample: Sample) -> Result<(), SynthError> {
        *self.slot(id)? = Some(Arc::new(sample));
        Ok(())
    }

    /// Lets line `id` go; placements of it already made still play.
    pub(crate) fn drop_line(&mut self, id: u32) {
        if let Ok(slot) = self.slot(id) {
            *slot = None;
        }
    }

    fn slot(&mut self, id: u32) -> Result<&mut Option<Arc<Sample>>, SynthError> {
        if id >= MAX_LINES {
            return Err(SynthError::Line(id));
        }
        let index = usize::try_from(id).map_err(|_| SynthError::Line(id))?;
        if self.lines.len() <= index {
            self.lines.resize(index + 1, None);
        }
        self.lines.get_mut(index).ok_or(SynthError::Line(id))
    }

    /// Places `utterance`, and says where it sounds.
    pub(crate) fn speak(&mut self, utterance: Utterance) -> Result<Spoken, SynthError> {
        let sample = usize::try_from(utterance.line)
            .ok()
            .and_then(|index| self.lines.get(index))
            .and_then(Option::as_ref)
            .ok_or(SynthError::Line(utterance.line))?
            .clone();
        if self.placed.len() >= MAX_PLACED {
            return Err(SynthError::Speech("too many lines placed at once"));
        }
        if utterance.speech < utterance.start {
            return Err(SynthError::Speech("its speech begins before its recording"));
        }
        let step = sample.rate() / self.rate;
        // The output frames its recording lasts: its frames, at the output's rate.
        #[allow(clippy::cast_precision_loss)] // a recording far shorter than 2^52 frames
        let length = (sample.frames() as f64 / step).ceil();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // positive and far below 2^64 frames
        let end = utterance.start + length as u64;
        let placed = Placed {
            utterance,
            sample,
            step,
            end,
            cut: None,
        };
        let at = self.placed.partition_point(|other| {
            (other.utterance.start, other.utterance.line) <= (utterance.start, utterance.line)
        });
        self.placed.insert(at, placed);
        Ok(Spoken {
            start: utterance.start,
            end,
        })
    }

    /// Cuts every line off at output frame `at`: one sounding there fades out from it, one placed
    /// to start at or after it is let go, and the duck of each is released from there (one whose
    /// speech had not begun ducks nothing).
    pub(crate) fn cut(&mut self, at: u64) {
        self.placed.retain(|placed| placed.utterance.start < at);
        for placed in &mut self.placed {
            placed.cut = Some(placed.cut.map_or(at, |cut| cut.min(at)));
        }
    }

    /// The music's level under a line from now on: 1 for none.
    pub(crate) fn set_level(&mut self, level: Gain) {
        self.level = level.get();
    }

    /// Every placement let go and the music back up at once: a stop, or a start from the top.
    pub(crate) fn clear(&mut self) {
        self.placed.clear();
        self.duck = 1.0;
    }

    /// Renders output frames `from..` into the speech stem and the duck lane of `stems`, which
    /// hold silence and a duck of 1 for them.
    pub(crate) fn render(&mut self, from: u64, stems: &mut Stems) {
        let frames = stems.frames();
        let mut duck = self.duck;
        for n in 0..frames {
            let frame = from + n as u64;
            let mut sound = 0.0;
            let mut ducked = false;
            for placed in &self.placed {
                if frame >= placed.utterance.start && frame < placed.silent_from() {
                    sound += placed.at(frame);
                }
                ducked |= placed
                    .ducked(self.delay)
                    .is_some_and(|(from, to)| frame >= from && frame < to);
            }
            let target = if ducked { self.level } else { 1.0 };
            let pole = if target < duck { self.fall } else { self.rise };
            duck = target + (duck - target) * pole;
            if let Some((left, right)) = stems.sides_mut(Channel::Speech)
                && let (Some(l), Some(r)) = (left.get_mut(n), right.get_mut(n))
            {
                #[allow(clippy::cast_possible_truncation)] // a sample, to the stem's precision
                let sample = sound as f32;
                *l = sample;
                *r = sample;
            }
            if let Some(lane) = stems.duck_mut().get_mut(n) {
                #[allow(clippy::cast_possible_truncation)]
                let level = duck as f32;
                *lane = level;
            }
        }
        self.duck = duck;
        let to = from + frames as u64;
        let delay = self.delay;
        self.placed.retain(|placed| placed.lasts_past(to, delay));
    }
}

#[cfg(test)]
mod tests {
    use super::{CUT_FADE, Speech, Utterance};
    use crate::note::Channel;
    use crate::sample::Sample;
    use crate::stems::Stems;
    use crate::units::{Gain, SampleRate};

    fn speech() -> Speech {
        Speech::new(SampleRate::new(48_000).unwrap())
    }

    /// A line of `frames` frames, each its own index + 1, at `rate`.
    fn ramp(frames: usize, rate: f64) -> Sample {
        Sample::new((1..=frames).map(|n| n as f32).collect(), None, rate).unwrap()
    }

    fn at(line: u32, start: u64) -> Utterance {
        Utterance {
            line,
            start,
            speech: start,
            hold: 0,
        }
    }

    #[test]
    fn a_line_sounds_from_its_frame_on_both_sides_of_the_speech_stem() {
        let mut speech = speech();
        speech.add_line(0, ramp(4, 48_000.0)).unwrap();
        let spoken = speech.speak(at(0, 3)).unwrap();
        assert_eq!((spoken.start, spoken.end), (3, 7));
        let mut stems = Stems::new(10);
        speech.render(0, &mut stems);
        // Frames 3..7 are the line's 1, 2, 3, 4; silence either side.
        let expected = [0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0];
        assert_eq!(stems.side(Channel::Speech, false), &expected);
        assert_eq!(stems.side(Channel::Speech, true), &expected);
        // The music stems are untouched.
        assert!(stems.side(Channel::Drums, false).iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_line_recorded_at_half_the_rate_lasts_twice_as_many_frames() {
        let mut speech = speech();
        speech.add_line(1, ramp(8, 24_000.0)).unwrap();
        let spoken = speech.speak(at(1, 0)).unwrap();
        // Eight frames at 24 kHz are sixteen at 48 kHz.
        assert_eq!(spoken.end, 16);
        let mut stems = Stems::new(16);
        speech.render(0, &mut stems);
        // Every other output frame lands on a recorded one; between, Catmull–Rom on a straight
        // line is the midpoint. (Near the last frame the ramp falls to silence and bends, so only
        // the first half is checked.)
        let left = stems.side(Channel::Speech, false);
        for (n, heard) in left.iter().take(8).enumerate() {
            let want = 1.0 + n as f64 / 2.0;
            assert!(
                (f64::from(*heard) - want).abs() < 1e-6,
                "frame {n}: {heard}"
            );
        }
    }

    #[test]
    fn a_line_placed_before_the_block_plays_on_from_where_it_has_got() {
        // Placed late — its first frame already rendered — it is heard from its fourth frame.
        let mut speech = speech();
        speech.add_line(0, ramp(6, 48_000.0)).unwrap();
        speech.speak(at(0, 97)).unwrap();
        let mut stems = Stems::new(4);
        speech.render(100, &mut stems);
        assert_eq!(stems.side(Channel::Speech, false), &[4.0, 5.0, 6.0, 0.0]);
    }

    #[test]
    fn a_cut_fades_a_line_out_and_lets_a_later_one_go() {
        let mut speech = speech();
        speech
            .add_line(0, Sample::new(vec![1.0; 1_000], None, 48_000.0).unwrap())
            .unwrap();
        speech.speak(at(0, 0)).unwrap();
        speech.speak(at(0, 600)).unwrap();
        speech.cut(100);
        let mut stems = Stems::new(800);
        speech.render(0, &mut stems);
        let left = stems.side(Channel::Speech, false);
        assert!((left[99] - 1.0).abs() < 1e-6);
        // From the cut it falls a 1/CUT_FADE step a frame: halfway down CUT_FADE/2 frames on.
        assert!((left[100] - 1.0).abs() < 1e-6);
        assert!((left[100 + 128] - 0.5).abs() < 1e-6);
        assert!(left[(100 + CUT_FADE) as usize..].iter().all(|x| *x == 0.0));
        // The line placed at 600, after the cut, never sounds.
        assert!(left[600..].iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_line_never_loaded_is_refused_by_its_number() {
        let mut speech = speech();
        let error = speech.speak(at(7, 0)).unwrap_err();
        assert!(error.to_string().contains("line 7"), "{error}");
        assert!(
            speech
                .add_line(super::MAX_LINES, ramp(2, 48_000.0))
                .is_err()
        );
    }

    #[test]
    fn a_placed_line_keeps_its_recording_when_the_line_is_let_go() {
        let mut speech = speech();
        speech.add_line(0, ramp(3, 48_000.0)).unwrap();
        speech.speak(at(0, 0)).unwrap();
        speech.drop_line(0);
        let mut stems = Stems::new(3);
        speech.render(0, &mut stems);
        assert_eq!(stems.side(Channel::Speech, false), &[1.0, 2.0, 3.0]);
        assert!(speech.speak(at(0, 10)).is_err());
    }

    #[test]
    fn the_music_falls_under_a_line_and_rises_after_it_and_its_hold() {
        let mut speech = speech();
        speech.set_level(Gain::new(0.6).unwrap());
        // 0.1 s of speech from frame 0, held 0.1 s: the music falls from frame 0 and starts
        // rising at 0.1 + 0.1 + 0.05 s = frame 12 000.
        speech
            .add_line(0, Sample::new(vec![0.0; 4_800], None, 48_000.0).unwrap())
            .unwrap();
        speech
            .speak(Utterance {
                line: 0,
                start: 0,
                speech: 0,
                hold: 4_800,
            })
            .unwrap();
        let mut stems = Stems::new(24_000);
        speech.render(0, &mut stems);
        let duck = stems.duck();
        // One time constant (0.08 s, frame 3 840) in, it has gone 1 − 1/e of the way to 0.6:
        // 0.6 + 0.4/e = 0.747152.
        assert!(
            (f64::from(duck[3_839]) - 0.747_152).abs() < 1e-4,
            "{}",
            duck[3_839]
        );
        // Still falling, not yet rising, at the release: 0.6 + 0.4·e^(−12 000/3 840).
        let at_release = 0.6 + 0.4 * (-12_000.0_f64 / 3_840.0).exp();
        assert!((f64::from(duck[11_999]) - at_release).abs() < 1e-4);
        // Rising after it with the slower constant (0.15 s, 7 200 frames): one constant on, it has
        // come 1 − 1/e of the way back to 1.
        let risen = 1.0 - (1.0 - at_release) / std::f64::consts::E;
        assert!((f64::from(duck[11_999 + 7_200]) - risen).abs() < 1e-4);
    }

    #[test]
    fn the_lines_of_a_stanza_duck_the_music_once() {
        // Two lines with a hold between them that reaches the second: the target never goes back
        // to 1 between them, so the music only falls.
        let mut speech = speech();
        speech.set_level(Gain::new(0.6).unwrap());
        speech
            .add_line(0, Sample::new(vec![0.0; 4_800], None, 48_000.0).unwrap())
            .unwrap();
        for start in [0, 9_600] {
            speech
                .speak(Utterance {
                    line: 0,
                    start,
                    speech: start,
                    hold: 4_800,
                })
                .unwrap();
        }
        let mut stems = Stems::new(14_000);
        speech.render(0, &mut stems);
        let duck = stems.duck();
        assert!(duck.windows(2).all(|w| w[1] <= w[0]));
    }

    #[test]
    fn with_no_duck_the_music_stays_at_one() {
        let mut speech = speech();
        speech.add_line(0, ramp(100, 48_000.0)).unwrap();
        speech.speak(at(0, 0)).unwrap();
        let mut stems = Stems::new(200);
        speech.render(0, &mut stems);
        assert!(stems.duck().iter().all(|x| *x == 1.0));
    }

    #[test]
    fn a_clear_lets_every_line_go_and_puts_the_music_back_up() {
        let mut speech = speech();
        speech.set_level(Gain::new(0.6).unwrap());
        speech.add_line(0, ramp(10_000, 48_000.0)).unwrap();
        speech.speak(at(0, 0)).unwrap();
        let mut stems = Stems::new(5_000);
        speech.render(0, &mut stems);
        // 0.6 + 0.4·e^(−5 000/3 840) = 0.709
        assert!(stems.duck()[4_999] < 0.75);
        speech.clear();
        speech.render(5_000, &mut stems);
        assert!(stems.side(Channel::Speech, false).iter().all(|x| *x == 0.0));
        assert!(stems.duck().iter().all(|x| *x == 1.0));
    }

    #[test]
    fn a_cut_lets_the_music_rise_from_the_cut_not_after_the_hold() {
        // A line of 0.1 s (4 800 frames) from frame 0, held 0.1 s after it, cut at 0.05 s (frame
        // 2 400). Nothing of it is heard after the cut, so the music starts rising the release
        // delay (0.05 s, 2 400 frames) after the cut — frame 4 800 — not after the line's end and
        // hold (frame 12 000).
        let mut speech = speech();
        speech.set_level(Gain::new(0.6).unwrap());
        speech
            .add_line(0, Sample::new(vec![0.0; 4_800], None, 48_000.0).unwrap())
            .unwrap();
        speech
            .speak(Utterance {
                line: 0,
                start: 0,
                speech: 0,
                hold: 4_800,
            })
            .unwrap();
        speech.cut(2_400);
        let mut stems = Stems::new(14_000);
        speech.render(0, &mut stems);
        let duck = stems.duck();
        // Falling for frames 0..4 800 with the 0.08 s constant (3 840 frames): after frame 4 799
        // it stands at 0.6 + 0.4·e^(−4 800/3 840).
        let at_release = 0.6 + 0.4 * (-4_800.0_f64 / 3_840.0).exp();
        assert!(
            (f64::from(duck[4_799]) - at_release).abs() < 1e-4,
            "{}",
            duck[4_799]
        );
        // Then rising with the 0.15 s constant (7 200 frames): one constant on, 1 − 1/e of the way
        // back to 1.
        let risen = 1.0 - (1.0 - at_release) / std::f64::consts::E;
        assert!(
            (f64::from(duck[4_799 + 7_200]) - risen).abs() < 1e-4,
            "{}",
            duck[4_799 + 7_200]
        );
    }

    #[test]
    fn a_line_cut_in_its_quiet_lead_ducks_nothing() {
        // Its recording from frame 0, its speech from frame 1 000, cut at frame 500: only the quiet
        // margin was heard, faded out, and the music never bows.
        let mut speech = speech();
        speech.set_level(Gain::new(0.6).unwrap());
        speech
            .add_line(0, Sample::new(vec![0.0; 4_800], None, 48_000.0).unwrap())
            .unwrap();
        speech
            .speak(Utterance {
                line: 0,
                start: 0,
                speech: 1_000,
                hold: 4_800,
            })
            .unwrap();
        speech.cut(500);
        let mut stems = Stems::new(12_000);
        speech.render(0, &mut stems);
        assert!(stems.duck().iter().all(|x| *x == 1.0));
        // And it is gone once its fade has played: a cut at 500 of a line that began at 0.
        speech.render(12_000, &mut stems);
        assert!(speech.placed.is_empty());
    }
}
