//! `prosody render`: the piece through the player's synth, to a WAV file (`docs/player.md` §7,
//! step 2).
//!
//! The Performance says every note; this hands the synth the ones it can sound here and says
//! which it cannot. The drums are FLAC files and the soundfonts MP3 zones inside webaudiofont
//! files: the page has the browser decode both, but decoding either in Rust needs a decoder, and
//! a decoder is a dependency `CLAUDE.md` §10 asks to be argued for. Until it is (with the soundfont
//! conversion of `docs/player.md` §8, decision 3), a render here plays the oscillators and names
//! every note it leaves out, rather than leaving them out silently (`CLAUDE.md` §6).

use anyhow::{Context as _, Result};
use prosody_emit::perform::{Channel, Note as Performed, Performance, Sound, Wave};
use prosody_synth::{
    Bank, Gain, Hertz, Losses, Midi, Note, NoteSpec, Piece, Renderer, Repeat, STEMS, SampleRate,
    Seconds, Source, Stems, master, wav,
};

/// The rate a render is written at.
const RATE: u32 = 48_000;

/// How long a render runs past the piece's end, for the last notes' releases. The longest release
/// the text sets is the chords' 0.6 s.
const TAIL: f64 = 1.0;

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
    /// Soundfont notes left out: no MP3 decoder.
    pub(crate) fonts: usize,
    /// What the synth itself lost.
    pub(crate) losses: Losses,
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
    let end = Seconds::new(length.get() + TAIL)
        .context("the render's length")?
        .frames(rate);
    let mut stems = Stems::new(2_048);
    let mut pcm = Vec::new();
    while renderer.frontier() < end {
        renderer.render(&mut stems);
        master::mix(&stems, [1.0; STEMS], &mut pcm);
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
