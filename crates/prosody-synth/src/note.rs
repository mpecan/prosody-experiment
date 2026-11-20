//! What the synth is asked to play: one note of a Performance, as the synth needs it.
//!
//! `prosody-emit`'s `Performance` says everything about a note — its words, lane, place in the
//! pattern, reverb and echo. This is the part a voice is made from, so the synth depends on no
//! other crate of the workspace and can be built for the browser on its own. A note keeps its
//! Performance's index as its `tag`: every voice is attributable to the note it plays, and through
//! the note to its words (invariant I4).

use crate::SynthError;
use crate::effects::Effects;
use crate::osc::Wave;
use crate::units::{Gain, Hertz, Midi, Seconds};

/// The player's default attack, where the text sets none (`docs/player.md` §9: a parameter the
/// text leaves unset is `None` in the Performance, and the player decides it here, once): a
/// millisecond. Strudel's, measured (`docs/player.md` §11): a sine with no attack is at full level
/// within its first period.
pub const DEFAULT_ATTACK: f64 = 0.001;

/// The player's default release, where the text sets none: ten milliseconds, Strudel's, measured
/// (a sine with no release falls silent 10 ms after its end, in a straight line).
pub const DEFAULT_RELEASE: f64 = 0.01;

/// The decay an oscillator note falls through when the text sets neither its attack nor its
/// release: 50 ms, to [`DEFAULT_SUSTAIN`]. Strudel's, measured: a sine with no envelope set starts
/// at full level and falls in a straight line to 0.6 of it over 50 ms; one with either set holds
/// at full level. A soundfont note measured no such fall, and a drum plays its sample through.
pub const DEFAULT_DECAY: f64 = 0.05;

/// The level that decay holds at.
pub const DEFAULT_SUSTAIN: f64 = 0.6;

/// The stem a note plays on: what mute and solo silence together (`docs/player.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Channel {
    /// Both drum lanes.
    Drums,
    /// The bass.
    Bass,
    /// The rhyme melody.
    Melody,
    /// The chord lane.
    Chords,
    /// The reading. No note of a Performance plays on it: its lines are placed by the renderer
    /// (`Renderer::speak`). The master treats it apart from the music: the duck lowers the music
    /// under it, never it (`crate::master`).
    Speech,
}

impl Channel {
    /// Every channel, in stem order.
    pub const ALL: [Self; 5] = [
        Self::Drums,
        Self::Bass,
        Self::Melody,
        Self::Chords,
        Self::Speech,
    ];

    /// Its stem's place in [`crate::Stems`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Drums => 0,
            Self::Bass => 1,
            Self::Melody => 2,
            Self::Chords => 3,
            Self::Speech => 4,
        }
    }

    /// The channel at stem `index`.
    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Drums),
            1 => Some(Self::Bass),
            2 => Some(Self::Melody),
            3 => Some(Self::Chords),
            4 => Some(Self::Speech),
            _ => None,
        }
    }
}

/// What makes a note's sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Source {
    /// The drum loaded as this instrument number ([`crate::Bank`]).
    Drum(u32),
    /// The soundfont loaded as this instrument number.
    Font(u32),
    /// An oscillator.
    Wave(Wave),
}

/// A note as it is handed over, before it is checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteSpec {
    /// Its index in the Performance's notes: what it is attributed to.
    pub tag: u32,
    /// Its stem.
    pub channel: Channel,
    /// Its sound.
    pub source: Source,
    /// When it starts, from the start of the piece.
    pub onset: Seconds,
    /// How long it is held. A drum ignores it and plays its sample through.
    pub length: Seconds,
    /// Its pitch; a drum has none.
    pub pitch: Option<Midi>,
    /// Its level: the text's `gain` times `velocity`.
    pub gain: Gain,
    /// Its level after its effects: the text's `postgain`.
    pub postgain: Gain,
    /// Its attack, or the player's default.
    pub attack: Option<Seconds>,
    /// Its release, or the player's default.
    pub release: Option<Seconds>,
    /// Its low-pass cutoff.
    pub lpf: Hertz,
    /// Its colour and its sends.
    pub effects: Effects,
}

/// A note the synth can play: a [`NoteSpec`] that has been checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note(NoteSpec);

impl Note {
    /// The note `spec` describes.
    ///
    /// # Errors
    ///
    /// [`SynthError::Unpitched`] for a soundfont or oscillator note with no pitch: there is no key
    /// to play and no frequency to run at, and guessing one would be sound nobody can trace.
    pub fn new(spec: NoteSpec) -> Result<Self, SynthError> {
        if spec.pitch.is_none() && !matches!(spec.source, Source::Drum(_)) {
            return Err(SynthError::Unpitched { tag: spec.tag });
        }
        Ok(Self(spec))
    }

    /// What it was made from.
    #[must_use]
    pub const fn spec(&self) -> &NoteSpec {
        &self.0
    }

    /// Its attack: the text's, or the default.
    #[must_use]
    pub fn attack(&self) -> Seconds {
        self.0
            .attack
            .or_else(|| Seconds::new(DEFAULT_ATTACK))
            .unwrap_or(self.0.length)
    }

    /// Its release: the text's, or the default.
    #[must_use]
    pub fn release(&self) -> Seconds {
        self.0
            .release
            .or_else(|| Seconds::new(DEFAULT_RELEASE))
            .unwrap_or(self.0.length)
    }

    /// Its decay and the level it holds at: [`DEFAULT_DECAY`] to [`DEFAULT_SUSTAIN`] for an
    /// oscillator note that sets neither attack nor release, and otherwise none, held at full
    /// level — Strudel's envelope, measured (`docs/player.md` §11).
    #[must_use]
    pub fn decay(&self) -> Option<(Seconds, f64)> {
        let bare = self.0.attack.is_none() && self.0.release.is_none();
        let oscillator = matches!(self.0.source, Source::Wave(_));
        (bare && oscillator)
            .then(|| Seconds::new(DEFAULT_DECAY))
            .flatten()
            .map(|decay| (decay, DEFAULT_SUSTAIN))
    }
}

#[cfg(test)]
mod tests {
    use super::{Channel, Note, NoteSpec, Source};
    use crate::effects::Effects;
    use crate::osc::Wave;
    use crate::units::{Gain, Hertz, Midi, Seconds};

    fn spec(source: Source, pitch: Option<f64>) -> NoteSpec {
        NoteSpec {
            tag: 7,
            channel: Channel::Melody,
            source,
            onset: Seconds::new(0.0).unwrap(),
            length: Seconds::new(0.5).unwrap(),
            pitch: pitch.and_then(Midi::new),
            gain: Gain::new(1.0).unwrap(),
            postgain: Gain::new(1.0).unwrap(),
            attack: None,
            release: Seconds::new(0.2),
            lpf: Hertz::new(5_000.0).unwrap(),
            effects: Effects::DRY,
        }
    }

    #[test]
    fn a_pitched_sound_needs_a_pitch() {
        assert!(Note::new(spec(Source::Wave(Wave::Sine), Some(60.0))).is_ok());
        assert!(Note::new(spec(Source::Drum(0), None)).is_ok());
        let refused = Note::new(spec(Source::Font(2), None));
        assert_eq!(refused, Err(crate::SynthError::Unpitched { tag: 7 }));
    }

    #[test]
    fn what_the_text_leaves_unset_takes_the_players_default() {
        let note = Note::new(spec(Source::Wave(Wave::Sawtooth), Some(60.0))).unwrap();
        assert!((note.attack().get() - super::DEFAULT_ATTACK).abs() < 1e-12);
        assert!((note.release().get() - 0.2).abs() < 1e-12);
        // A release is set, so no decay; with neither set, an oscillator decays and a soundfont
        // does not.
        assert_eq!(note.decay(), None);
        let mut bare = spec(Source::Wave(Wave::Sine), Some(60.0));
        bare.release = None;
        let decay = Note::new(bare).unwrap().decay().unwrap();
        assert!((decay.0.get() - 0.05).abs() < 1e-12 && (decay.1 - 0.6).abs() < 1e-12);
        bare.source = Source::Font(0);
        assert_eq!(Note::new(bare).unwrap().decay(), None);
    }

    #[test]
    fn channels_number_their_stems() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_index(channel.index()), Some(channel));
        }
        assert_eq!(Channel::from_index(5), None);
    }
}
