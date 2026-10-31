//! `prosody render`: the piece through the player's synth, to a WAV file (`docs/player.md` §7,
//! steps 2 and 3): every effect the text asks for, through the master stage.
//!
//! The Performance says every note; this hands the synth the ones it can sound here and says
//! which it cannot. The drums and, since step 3, the soundfonts are FLAC files beside the page
//! (`docs/player.md` §8, decision 3): the page has the browser decode them, but decoding FLAC in
//! Rust needs a decoder, a dependency `CLAUDE.md` §10 asks to be argued for. Until it is, a render
//! here plays the oscillators and names every note it leaves out, rather than leaving them out
//! silently (`CLAUDE.md` §6).

use anyhow::{Context as _, Result};
use prosody_emit::perform::{Channel, Note as Performed, Performance, Sound, Vowel, Wave};
use prosody_synth::{
    Bank, Compression, Crush, Echo, Effects, Gain, Hertz, Losses, Midi, Note, NoteSpec, Piece,
    Renderer, Repeat, Room, STEMS, SampleRate, Seconds, Shape, Source, Stems, master::Master, wav,
};

/// The rate a render is written at.
const RATE: u32 = 48_000;

/// How long a render runs past the piece's end, for the last notes' releases and the reverb's tail.
/// The longest release the text sets is the chords' 0.6 s; the reverb rings for its `roomsize`
/// in seconds (at most 10), and the echo for its repeats, which [`tail`] adds.
const TAIL: f64 = 1.0;

/// How far below its first repeat an echo is still rendered: 2^−16, the last step a 16-bit WAV
/// holds, taking the first repeat at full scale (none is louder) — so what is cut off is below
/// what the file could say.
const ECHO_FLOOR: f64 = 1.0 / 65_536.0;

/// The render's tail: [`TAIL`], and the longest of the reverbs and echoes any note sends to, which
/// ring at once.
fn tail(performance: &Performance) -> f64 {
    let room = performance
        .notes()
        .iter()
        .filter(|note| note.sends().room().get() > 0.0)
        .map(|note| note.sends().room_size())
        .fold(0.0, f64::max);
    let echo = performance
        .notes()
        .iter()
        .filter_map(|note| note.sends().delay())
        .map(|echo| echo_tail(echo.time().get(), echo.feedback().get()))
        .fold(0.0, f64::max);
    TAIL + room.clamp(0.0, Room::MAX_SIZE).max(echo)
}

/// How long an echo `time` behind, each repeat `feedback` of the one before, rings after the last
/// note sent into it: every repeat down to [`ECHO_FLOOR`] of the first, the first `time` after the
/// note and each `time` after the last. Without it a render cut a slow piece's echo off — at tempo
/// 15 (a 3 s echo) before its first repeat.
fn echo_tail(time: f64, feedback: f64) -> f64 {
    // `level` is the next repeat's, relative to the first; capped, as the synth's line is, should a
    // feedback ever come close to 1.
    let mut repeats = 1_u32;
    let mut level = feedback;
    while level >= ECHO_FLOOR && repeats < 1_000 {
        level *= feedback;
        repeats += 1;
    }
    time * f64::from(repeats)
}

/// A rendered piece and what it could not play.
pub(crate) struct Rendered {
    /// The WAV file's bytes.
    pub(crate) wav: Vec<u8>,
    /// How long it lasts, tail included.
    pub(crate) seconds: f64,
    /// Notes played.
    pub(crate) played: usize,
    /// Drum notes left out: no FLAC decoder.
    pub(crate) drums: usize,
    /// Soundfont notes left out: no FLAC decoder.
    pub(crate) fonts: usize,
    /// What the synth itself lost.
    pub(crate) losses: Losses,
}

/// What a Performance's note `tag` asks of the effects, checked as the synth takes it: a value the
/// synth cannot mean is an error naming the note, never a silent drop (`CLAUDE.md` §6).
fn effects(tag: u32, note: &Performed) -> Result<Effects> {
    let voice = note.voice();
    let sends = note.sends();
    let bad = |what: &str| format!("note {tag}'s {what} is out of the synth's range");
    let crush = voice
        .crush()
        .map(|bits| Crush::new(f64::from(bits)).with_context(|| bad("crush")))
        .transpose()?;
    let shape = voice
        .shape()
        .map(|shape| Shape::new(shape.get()).with_context(|| bad("shape")))
        .transpose()?;
    let vowel = voice.vowel().map(|vowel| match vowel {
        Vowel::A => prosody_synth::Vowel::A,
        Vowel::E => prosody_synth::Vowel::E,
        Vowel::I => prosody_synth::Vowel::I,
        Vowel::O => prosody_synth::Vowel::O,
        Vowel::U => prosody_synth::Vowel::U,
    });
    let room = if sends.room().get() > 0.0 {
        let send = Gain::new(sends.room().get()).with_context(|| bad("room"))?;
        let size = Seconds::new(sends.room_size()).with_context(|| bad("room size"))?;
        Some(Room::new(send, size).with_context(|| bad("room size"))?)
    } else {
        None
    };
    let echo = sends
        .delay()
        .map(|echo| {
            let send = Gain::new(echo.level().get()).with_context(|| bad("delay"))?;
            let time = Seconds::new(echo.time().get()).with_context(|| bad("delay time"))?;
            Echo::new(send, time, echo.feedback().get()).with_context(|| bad("delay"))
        })
        .transpose()?;
    let compression = note
        .compressor()
        .map(|c| {
            Compression::new(c.threshold().get(), c.ratio()).with_context(|| bad("compressor"))
        })
        .transpose()?;
    Ok(Effects {
        crush,
        vowel,
        shape,
        room,
        echo,
        compression,
    })
}

/// The note the synth plays for a Performance's note `tag`, or `None` for one it cannot sound
/// here (a sample it cannot decode).
fn synth_note(tag: u32, note: &Performed) -> Result<Option<Note>> {
    let Sound::Wave(wave) = note.sound() else {
        return Ok(None);
    };
    let wave = match wave {
        Wave::Sine => prosody_synth::Wave::Sine,
        Wave::Sawtooth => prosody_synth::Wave::Sawtooth,
        Wave::Square => prosody_synth::Wave::Square,
        Wave::Triangle => prosody_synth::Wave::Triangle,
    };
    let channel = match note.channel() {
        Channel::Drums => prosody_synth::Channel::Drums,
        Channel::Bass => prosody_synth::Channel::Bass,
        Channel::Melody => prosody_synth::Channel::Melody,
        Channel::Chords => prosody_synth::Channel::Chords,
    };
    let time = |seconds: f64, what: &str| {
        Seconds::new(seconds).with_context(|| format!("note {tag}'s {what} is not a time"))
    };
    let voice = note.voice();
    let spec = NoteSpec {
        tag,
        channel,
        source: Source::Wave(wave),
        onset: time(note.onset().get(), "onset")?,
        length: time(note.length().get(), "length")?,
        pitch: note.pitch().and_then(|pitch| Midi::new(pitch.get())),
        gain: Gain::new(note.gain().get()).with_context(|| format!("note {tag}'s gain"))?,
        postgain: Gain::new(note.postgain().get())
            .with_context(|| format!("note {tag}'s postgain"))?,
        attack: voice.attack().and_then(|attack| Seconds::new(attack.get())),
        release: voice
            .release()
            .and_then(|release| Seconds::new(release.get())),
        lpf: Hertz::new(voice.lpf().get()).with_context(|| format!("note {tag}'s cutoff"))?,
        effects: effects(tag, note)?,
    };
    Ok(Some(Note::new(spec)?))
}

/// Renders `performance` once through, with a tail for the releases.
pub(crate) fn render(performance: &Performance) -> Result<Rendered> {
    let rate = SampleRate::new(RATE)?;
    let mut notes = Vec::new();
    let (mut drums, mut fonts) = (0, 0);
    for (index, performed) in performance.notes().iter().enumerate() {
        let tag = u32::try_from(index).context("more notes than a render can number")?;
        match synth_note(tag, performed)? {
            Some(note) => notes.push(note),
            None if matches!(performed.sound(), Sound::Drum { .. }) => drums += 1,
            None => fonts += 1,
        }
    }
    let played = notes.len();
    let bars: Vec<Seconds> = performance
        .bars()
        .iter()
        .filter_map(|bar| Seconds::new(bar.at().get()))
        .collect();
    let length = Seconds::new(performance.seconds().get()).context("the piece's length")?;
    let mut renderer = Renderer::new(rate, Bank::new());
    renderer.play(Piece::new(notes, &bars, length, Repeat::Once, rate));
    let end = Seconds::new(length.get() + tail(performance))
        .context("the render's length")?
        .frames(rate);
    let mut stems = Stems::new(2_048);
    let mut master = Master::new(f64::from(RATE));
    let mut pcm = Vec::new();
    while renderer.frontier() < end {
        renderer.render(&mut stems);
        master.mix(&stems, [1.0; STEMS], &mut pcm);
    }
    let wav = wav::encode(&pcm, RATE).context("the render is too long for a WAV file")?;
    #[allow(clippy::cast_precision_loss)] // a frame count far below 2^52
    let seconds = (pcm.len() / 2) as f64 / f64::from(RATE);
    Ok(Rendered {
        wav,
        seconds,
        played,
        drums,
        fonts,
        losses: renderer.losses(),
    })
}

#[cfg(test)]
mod tests {
    use super::echo_tail;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "{actual} is not {expected}"
        );
    }

    #[test]
    fn an_echo_rings_until_a_repeat_is_below_what_a_wav_holds() {
        // At the text's feedback, 0.35: 0.35^10 = 2.76e-5 is still above 2^−16 = 1.53e-5, and
        // 0.35^11 = 9.65e-6 is not, so the first eleven repeats are heard, the last 11 × the time
        // after the note: 8.25 s at tempo 60's 0.75 s.
        close(echo_tail(0.75, 0.35), 8.25);
        // At 0.5, 0.5^16 is 2^−16 exactly, kept: seventeen repeats.
        close(echo_tail(1.0, 0.5), 17.0);
        // No feedback: the one repeat.
        close(echo_tail(0.5, 0.0), 0.5);
    }
}
