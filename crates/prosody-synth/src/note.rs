//! What the synth is asked to play: one note of a Performance, as the synth needs it.
//!
//! `prosody-emit`'s `Performance` says everything about a note — its words, lane, place in the
//! pattern, reverb and echo. This is the part a voice is made from, so the synth depends on no
//! other crate of the workspace and can be built for the browser on its own. A note keeps its
//! Performance's index as its `tag`: every voice is attributable to the note it plays, and through
//! the note to its words (invariant I4).

use crate::SynthError;
use crate::osc::Wave;
use crate::units::{Gain, Hertz, Midi, Seconds};

/// The player's default attack, where the text sets none (`docs/player.md` §9: a parameter the
/// text leaves unset is `None` in the Performance, and the player decides it here, once): a
/// millisecond, enough to keep a note's start from clicking. To be matched to Strudel's by
/// measurement in step 3.
pub const DEFAULT_ATTACK: f64 = 0.001;

/// The player's default release, where the text sets none: ten milliseconds, likewise.
pub const DEFAULT_RELEASE: f64 = 0.01;

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
}

impl Channel {
    /// Every channel, in stem order.
    pub const ALL: [Self; 4] = [Self::Drums, Self::Bass, Self::Melody, Self::Chords];

    /// Its stem's place in [`crate::Stems`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Drums => 0,
            Self::Bass => 1,
            Self::Melody => 2,
            Self::Chords => 3,
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
}

#[cfg(test)]
mod tests {
    use super::{Channel, Note, NoteSpec, Source};
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
    }

    #[test]
    fn channels_number_their_stems() {
        for channel in Channel::ALL {
            assert_eq!(Channel::from_index(channel.index()), Some(channel));
        }
        assert_eq!(Channel::from_index(4), None);
    }
}
