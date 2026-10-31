//! A Performance as the page hands it to the player: every note, and every cue of its reading, as a
//! flat record of numbers (`web/src/player/notes.ts` writes them; a test on each side pins the
//! layout). Numbers rather than the Performance's JSON because the browser's render worker moves
//! them as one transferable buffer and the app's player as one base64 string, and because neither
//! the synth nor this crate needs a JSON parser for anything else.
//!
//! # A note's record
//!
//! | index | field | |
//! |---|---|---|
//! | 0 | tag | the note's index in the Performance's `notes` (what it is attributed to, I4) |
//! | 1 | channel | 0 drums, 1 bass, 2 melody, 3 chords |
//! | 2 | kind | 0 drum, 1 soundfont, 2 oscillator |
//! | 3 | id | the instrument number the page loaded it as; for an oscillator 0 sine, 1 sawtooth, 2 square, 3 triangle |
//! | 4 | onset | seconds from the start of the piece |
//! | 5 | length | seconds |
//! | 6 | pitch | MIDI, or NaN for none |
//! | 7 | gain | `gain` × `velocity` |
//! | 8 | postgain | |
//! | 9 | attack | seconds, or NaN where the text sets none |
//! | 10 | release | seconds, or NaN where the text sets none |
//! | 11 | lpf | Hz |
//! | 12 | crush | bits, or NaN for none |
//! | 13 | vowel | 0 a, 1 e, 2 i, 3 o, 4 u, or NaN for none |
//! | 14 | shape | 0..1, or NaN for none |
//! | 15 | room | the reverb send; 0 for none |
//! | 16 | roomsize | the reverb's RT60, seconds (read only with a send) |
//! | 17 | delay | the echo send, or NaN for none |
//! | 18 | delaytime | seconds |
//! | 19 | delayfeedback | 0..1 |
//! | 20 | compressor | threshold, dB, or NaN for none |
//! | 21 | compressorRatio | `n` for `n`:1 |
//!
//! # A cue's record
//!
//! | index | field | |
//! |---|---|---|
//! | 0 | line | in reading order, from 0 |
//! | 1 | stanza | from 0 |
//! | 2 | pause | seconds after the line at the natural pace |
//! | 3 | fitted slot | the bar the fitted pace reads it in, or NaN where it does not |
//! | 4 | fitted at | seconds from the piece's start to that bar |
//! | 5 | natural onset | 0 `at`, 1 `breath`, 2 `follows`, or NaN where the natural pace does not read it |
//! | 6 | natural at | seconds to the bar it may begin in, or NaN for a line that only follows |

use prosody_synth::{
    Channel, Compression, Crush, Echo, Effects, Gain, Hertz, Midi, Note, NoteSpec, Room, Seconds,
    Shape, Source, SynthError, Vowel, Wave,
};

use crate::reader::{Cue, Fitted, Natural, Onset};

/// Numbers a note's record holds.
pub const NOTE_STRIDE: usize = 22;

/// Numbers a cue's record holds.
pub const CUE_STRIDE: usize = 7;

/// The notes of a Performance from their records, in order.
///
/// # Errors
///
/// The first record that does not describe a note, by its tag.
pub fn decode_notes(records: &[f64]) -> Result<Vec<Note>, SynthError> {
    if !records.len().is_multiple_of(NOTE_STRIDE) {
        return Err(SynthError::Malformed {
            tag: 0,
            why: "the records are not a whole number of notes long",
        });
    }
    records.chunks_exact(NOTE_STRIDE).map(decode_note).collect()
}

/// One note from its record.
fn decode_note(record: &[f64]) -> Result<Note, SynthError> {
    let field = |index: usize| record.get(index).copied().unwrap_or(f64::NAN);
    let tag = whole(field(0)).and_then(|tag| u32::try_from(tag).ok());
    let Some(tag) = tag else {
        return Err(SynthError::Malformed {
            tag: 0,
            why: "its tag is not a note's index",
        });
    };
    let malformed = |why| SynthError::Malformed { tag, why };
    // The reading's stem (4) takes no Performance note: its lines come with the reading.
    let channel = whole(field(1))
        .and_then(|index| usize::try_from(index).ok())
        .filter(|index| *index < 4)
        .and_then(Channel::from_index)
        .ok_or_else(|| malformed("its channel is not 0–3"))?;
    let id = whole(field(3))
        .and_then(|id| u32::try_from(id).ok())
        .ok_or_else(|| malformed("its sound's number is not a number"))?;
    let source = match whole(field(2)) {
        Some(0) => Source::Drum(id),
        Some(1) => Source::Font(id),
        Some(2) => Source::Wave(match id {
            0 => Wave::Sine,
            1 => Wave::Sawtooth,
            2 => Wave::Square,
            3 => Wave::Triangle,
            _ => return Err(malformed("its wave is not 0–3")),
        }),
        _ => return Err(malformed("its kind is not 0–2")),
    };
    let seconds = |index, why| Seconds::new(field(index)).ok_or_else(|| malformed(why));
    let optional = |index, why| {
        let value = field(index);
        if value.is_nan() {
            Ok(None)
        } else {
            Seconds::new(value).map(Some).ok_or_else(|| malformed(why))
        }
    };
    let pitch = if field(6).is_nan() {
        None
    } else {
        Some(Midi::new(field(6)).ok_or_else(|| malformed("its pitch is not a MIDI note"))?)
    };
    Note::new(NoteSpec {
        tag,
        channel,
        source,
        onset: seconds(4, "its onset is not a time")?,
        length: seconds(5, "its length is not a time")?,
        pitch,
        gain: Gain::new(field(7)).ok_or_else(|| malformed("its gain is not a level"))?,
        postgain: Gain::new(field(8)).ok_or_else(|| malformed("its postgain is not a level"))?,
        attack: optional(9, "its attack is not a time")?,
        release: optional(10, "its release is not a time")?,
        lpf: Hertz::new(field(11)).ok_or_else(|| malformed("its cutoff is not a frequency"))?,
        effects: decode_effects(&field, &malformed)?,
    })
}

/// A note's effects from fields 12–21 of its record.
fn decode_effects(
    field: &dyn Fn(usize) -> f64,
    malformed: &dyn Fn(&'static str) -> SynthError,
) -> Result<Effects, SynthError> {
    // A field that is NaN is an effect the text does not set; any other value must be one the
    // synth can mean.
    let optional = |index: usize| Some(field(index)).filter(|value| !value.is_nan());
    let crush = optional(12)
        .map(|bits| Crush::new(bits).ok_or_else(|| malformed("its crush is not 1–16 bits")))
        .transpose()?;
    let vowel = optional(13)
        .map(|index| {
            whole(index)
                .and_then(|index| usize::try_from(index).ok())
                .and_then(Vowel::from_index)
                .ok_or_else(|| malformed("its vowel is not 0–4"))
        })
        .transpose()?;
    let shape = optional(14)
        .map(|amount| Shape::new(amount).ok_or_else(|| malformed("its shape is not 0–0.99")))
        .transpose()?;
    let send = Gain::new(field(15)).ok_or_else(|| malformed("its room is not a level"))?;
    let room = if send.get() > 0.0 {
        let size =
            Seconds::new(field(16)).ok_or_else(|| malformed("its roomsize is not a time"))?;
        Some(Room::new(send, size).ok_or_else(|| malformed("its roomsize is not 0–10 s"))?)
    } else {
        None
    };
    let echo = optional(17)
        .map(|level| {
            let send = Gain::new(level).ok_or_else(|| malformed("its delay is not a level"))?;
            let time =
                Seconds::new(field(18)).ok_or_else(|| malformed("its delaytime is not a time"))?;
            Echo::new(send, time, field(19)).ok_or_else(|| malformed("its delay is not an echo"))
        })
        .transpose()?;
    let compression = optional(20)
        .map(|threshold| {
            Compression::new(threshold, field(21))
                .ok_or_else(|| malformed("its compressor is not a compressor"))
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

/// Why a cue's record cannot be read: which cue, and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("cue {index} of the reading is not a cue: {why}")]
pub struct CueError {
    /// Its place among the records.
    pub index: usize,
    /// What is wrong with it.
    pub why: &'static str,
}

/// The cues of a reading from their records, in order.
///
/// # Errors
///
/// The first record that does not describe a cue.
pub fn decode_cues(records: &[f64]) -> Result<Vec<Cue>, CueError> {
    if !records.len().is_multiple_of(CUE_STRIDE) {
        return Err(CueError {
            index: records.len() / CUE_STRIDE,
            why: "the records are not a whole number of cues long",
        });
    }
    records
        .chunks_exact(CUE_STRIDE)
        .enumerate()
        .map(|(index, record)| decode_cue(record).map_err(|why| CueError { index, why }))
        .collect()
}

/// One cue from its record.
fn decode_cue(record: &[f64]) -> Result<Cue, &'static str> {
    let field = |index: usize| record.get(index).copied().unwrap_or(f64::NAN);
    let count = |index, why| {
        whole(field(index))
            .and_then(|value| u32::try_from(value).ok())
            .ok_or(why)
    };
    let fitted = if field(3).is_nan() {
        None
    } else {
        Some(Fitted {
            slot: count(3, "its fitted bar is not a bar")?,
            at: Seconds::new(field(4)).ok_or("its fitted bar's time is not a time")?,
        })
    };
    let natural = if field(5).is_nan() {
        None
    } else {
        let onset = match whole(field(5)) {
            Some(0) => Onset::At,
            Some(1) => Onset::Breath,
            Some(2) => Onset::Follows,
            _ => return Err("its onset is not 0–2"),
        };
        let at = if field(6).is_nan() {
            None
        } else {
            Some(Seconds::new(field(6)).ok_or("its natural bar's time is not a time")?)
        };
        Some(Natural { onset, at })
    };
    Ok(Cue {
        line: count(0, "its line is not a line")?,
        stanza: count(1, "its stanza is not a stanza")?,
        pause: Seconds::new(field(2)).ok_or("its pause is not a time")?,
        fitted,
        natural,
    })
}

/// `value` as a whole number, if it is one.
pub(crate) fn whole(value: f64) -> Option<i64> {
    #[allow(clippy::cast_possible_truncation)] // checked to be whole and in range first
    let whole =
        (value.is_finite() && value.fract() == 0.0 && value.abs() < 9e15).then_some(value as i64);
    whole
}

#[cfg(test)]
mod tests {
    use super::{CUE_STRIDE, NOTE_STRIDE, decode_cues, decode_notes};
    use crate::reader::{Fitted, Natural, Onset};
    use prosody_synth::{Channel, Seconds, Source, Wave};

    /// A record as the page writes it (`web/test/player.test.ts` pins the same layout).
    fn record(tag: f64, kind: f64, id: f64, pitch: f64) -> [f64; NOTE_STRIDE] {
        [
            tag,
            2.0,
            kind,
            id,
            0.25,
            0.5,
            pitch,
            0.8,
            1.0,
            f64::NAN,
            0.1,
            3_000.0,
            f64::NAN,
            f64::NAN,
            f64::NAN,
            0.0,
            2.5,
            f64::NAN,
            0.0,
            0.0,
            f64::NAN,
            1.0,
        ]
    }

    #[test]
    fn a_record_is_read_field_by_field() {
        let notes = decode_notes(&record(3.0, 2.0, 1.0, 64.0)).unwrap();
        let spec = notes[0].spec();
        assert_eq!(spec.tag, 3);
        assert_eq!(spec.channel, Channel::Melody);
        assert_eq!(spec.source, Source::Wave(Wave::Sawtooth));
        assert!((spec.onset.get() - 0.25).abs() < 1e-12);
        assert!((spec.length.get() - 0.5).abs() < 1e-12);
        assert!((spec.pitch.unwrap().get() - 64.0).abs() < 1e-12);
        assert!((spec.gain.get() - 0.8).abs() < 1e-12);
        assert!(spec.attack.is_none());
        assert!((spec.release.unwrap().get() - 0.1).abs() < 1e-12);
        assert!((spec.lpf.get() - 3_000.0).abs() < 1e-9);
        assert_eq!(spec.effects, prosody_synth::Effects::DRY);
    }

    #[test]
    fn a_records_effects_are_read_field_by_field() {
        let mut r = record(4.0, 2.0, 0.0, 60.0);
        r[12..22].copy_from_slice(&[8.0, 3.0, 0.1, 0.35, 2.5, 0.2, 0.3, 0.35, -24.0, 7.0]);
        let notes = decode_notes(&r).unwrap();
        let effects = notes[0].spec().effects;
        assert!((effects.crush.unwrap().bits() - 8.0).abs() < 1e-12);
        assert_eq!(effects.vowel, Some(prosody_synth::Vowel::O));
        assert!((effects.shape.unwrap().amount() - 0.1).abs() < 1e-12);
        let room = effects.room.unwrap();
        assert!(
            (room.send().get() - 0.35).abs() < 1e-12 && (room.size().get() - 2.5).abs() < 1e-12
        );
        let echo = effects.echo.unwrap();
        assert!((echo.send().get() - 0.2).abs() < 1e-12);
        assert!((echo.time().get() - 0.3).abs() < 1e-12 && (echo.feedback() - 0.35).abs() < 1e-12);
        let compression = effects.compression.unwrap();
        assert!((compression.threshold() + 24.0).abs() < 1e-12);
        assert!((compression.ratio() - 7.0).abs() < 1e-12);
        // Out of range is refused by the note's tag, not dropped.
        r[12] = 20.0;
        assert!(decode_notes(&r).unwrap_err().to_string().contains("note 4"));
    }

    #[test]
    fn a_record_that_is_not_a_note_is_refused_by_its_tag() {
        let error = decode_notes(&record(5.0, 7.0, 0.0, 60.0)).unwrap_err();
        assert!(error.to_string().contains("note 5"), "{error}");
        assert!(decode_notes(&record(1.5, 0.0, 0.0, f64::NAN)).is_err());
        assert!(decode_notes(&[0.0; NOTE_STRIDE - 1]).is_err());
        // A soundfont note with no pitch.
        assert!(decode_notes(&record(2.0, 1.0, 0.0, f64::NAN)).is_err());
    }

    #[test]
    fn a_cue_is_read_field_by_field() {
        // Line 3 of stanza 1, pausing 0.5 s: fitted in bar 2 at 4 s, and at the natural pace under
        // `breath` from bar 3 at 6 s. Then line 4, read only at the natural pace, following it.
        let records = [
            3.0,
            1.0,
            0.5,
            2.0,
            4.0,
            1.0,
            6.0,
            4.0,
            1.0,
            0.25,
            f64::NAN,
            f64::NAN,
            2.0,
            f64::NAN,
        ];
        let cues = decode_cues(&records).unwrap();
        assert_eq!(cues.len(), 2);
        let s = |x| Seconds::new(x).unwrap();
        assert_eq!(
            (cues[0].line, cues[0].stanza, cues[0].pause),
            (3, 1, s(0.5))
        );
        assert_eq!(
            cues[0].fitted,
            Some(Fitted {
                slot: 2,
                at: s(4.0)
            })
        );
        assert_eq!(
            cues[0].natural,
            Some(Natural {
                onset: Onset::Breath,
                at: Some(s(6.0))
            })
        );
        assert_eq!(cues[1].fitted, None);
        assert_eq!(
            cues[1].natural,
            Some(Natural {
                onset: Onset::Follows,
                at: None
            })
        );
    }

    #[test]
    fn a_record_that_is_not_a_cue_is_refused_by_its_place() {
        let mut records = [0.0; 2 * CUE_STRIDE];
        records[3] = f64::NAN;
        records[5] = f64::NAN;
        records[CUE_STRIDE + 5] = 9.0;
        let error = decode_cues(&records).unwrap_err();
        assert_eq!(error.index, 1);
        assert!(decode_cues(&records[..=CUE_STRIDE]).is_err());
    }
}
