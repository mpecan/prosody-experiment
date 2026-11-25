//! The page's messages to the native player, as they cross the bridge, and what each does to the
//! engine (`docs/player.md` §13).
//!
//! They are the render worker's messages (`web/src/player/render-worker.ts`), so the page drives
//! both players the same way. Only the encoding differs: a web view on Android can hand Rust a
//! string and nothing else — Tauri sends every call as JSON over the web view's message interface,
//! because Android's web view cannot read a request body — so a buffer of samples or records comes as
//! base64 of its little-endian bytes rather than as a transferable buffer.

use std::collections::BTreeMap;

use prosody_player::records::{decode_cues, decode_notes};
use prosody_player::{Cast, Engine, Handed, Pace};
use prosody_synth::{Gain, Loop, Sample, Seconds, Zone};
use serde::Deserialize;

/// Who reads each line, by line number: a render key, or none for the browser's voice.
pub(crate) type Lines = Vec<(u32, Option<String>)>;

/// A message from the page.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase", rename_all_fields = "camelCase")]
pub(crate) enum Message {
    /// A drum, as instrument `id`.
    Drum {
        /// Its number.
        id: u32,
        /// Its left channel.
        left: Floats,
        /// Its right channel; empty for mono.
        right: Floats,
        /// Its rate.
        rate: f64,
    },
    /// A soundfont zone.
    Zone {
        /// The instrument's number.
        id: u32,
        /// Its place among its file's zones.
        position: u32,
        /// Its left channel.
        left: Floats,
        /// Its right channel; empty for mono.
        right: Floats,
        /// Its rate.
        rate: f64,
        /// The MIDI note it sounds at its own rate.
        root: f64,
        /// Its lowest key.
        low: u8,
        /// Its highest key.
        high: u8,
        /// Its loop's start, in its frames; none for no loop.
        loop_start: Option<f64>,
        /// Its loop's end.
        loop_end: Option<f64>,
    },
    /// A piece from its top.
    Play {
        /// The start of play.
        generation: u32,
        /// The piece.
        serial: u32,
        /// Its notes' records.
        notes: Doubles,
        /// Its bars' starts.
        bars: Doubles,
        /// Its length.
        seconds: f64,
        /// Its cues' records.
        cues: Doubles,
        /// Who reads each line.
        cast: Lines,
        /// The pace read.
        pace: String,
        /// The music's level under a line.
        duck: f64,
        /// What the notification calls it.
        title: Option<String>,
    },
    /// A piece handed over at the next bar.
    Swap {
        /// The piece.
        serial: u32,
        /// Its notes' records.
        notes: Doubles,
        /// Its bars' starts.
        bars: Doubles,
        /// Its length.
        seconds: f64,
        /// Its cues' records.
        cues: Doubles,
        /// Who reads each line.
        cast: Lines,
    },
    /// Stop.
    Stop {
        /// The start of play it begins.
        generation: u32,
    },
    /// Who reads each line, by piece.
    Cast {
        /// Each piece's cast.
        casts: Vec<(u32, Lines)>,
        /// The pace read.
        pace: String,
        /// Whether to let go of what is placed from the frontier.
        quiet: bool,
    },
    /// A recorded line.
    Line {
        /// Its render key.
        key: String,
        /// Its mono PCM.
        pcm: Floats,
        /// Its rate.
        rate: f64,
        /// Seconds before its speech begins.
        lead: f64,
    },
    /// The browser's voice finished a line.
    Said {
        /// The chain it belongs to.
        chain: u32,
        /// The playhead's frame when it ended.
        end: f64,
    },
    /// A line could not be rendered.
    Failed {
        /// Its render key.
        key: String,
    },
    /// The music's level under a line.
    Duck {
        /// The level.
        level: f64,
    },
}

/// What a message asks of the phone beyond the engine: the notification's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Session {
    /// Something plays, called this.
    Playing(String),
    /// Nothing plays.
    Stopped,
}

impl Message {
    /// Does what the message says to `engine`; says what the notification should now show, for a
    /// start or a stop.
    ///
    /// # Errors
    ///
    /// What could not be read or loaded, for the page to say (`CLAUDE.md` §6: refused, not guessed).
    pub(crate) fn apply(self, engine: &mut Engine) -> Result<Option<Session>, String> {
        match self {
            Self::Drum {
                id,
                left,
                right,
                rate,
            } => {
                let sample = sample(left, right, rate)?;
                engine
                    .add_drum(id, sample)
                    .map_err(|error| error.to_string())?;
            }
            Self::Zone {
                id,
                position,
                left,
                right,
                rate,
                root,
                low,
                high,
                loop_start,
                loop_end,
            } => {
                let sample = sample(left, right, rate)?;
                let looped = Loop::new(
                    loop_start.unwrap_or(f64::NAN),
                    loop_end.unwrap_or(f64::NAN),
                    sample.frames(),
                );
                let zone = Zone::new(sample, root, low, high, looped)
                    .map_err(|error| error.to_string())?;
                engine
                    .add_zone(id, position, zone)
                    .map_err(|error| error.to_string())?;
            }
            Self::Play {
                generation,
                serial,
                notes,
                bars,
                seconds,
                cues,
                cast,
                pace,
                duck,
                title,
            } => {
                let piece = handed(serial, &notes, &bars, seconds, &cues, cast)?;
                let pace = pace_named(&pace)?;
                let duck = Gain::new(duck).ok_or("the duck's level is not a level")?;
                engine.play(generation, piece, pace, duck);
                return Ok(Some(Session::Playing(
                    title.unwrap_or_else(|| "Prosody".to_owned()),
                )));
            }
            Self::Swap {
                serial,
                notes,
                bars,
                seconds,
                cues,
                cast,
            } => engine.swap(handed(serial, &notes, &bars, seconds, &cues, cast)?),
            Self::Stop { generation } => {
                engine.stop(generation);
                return Ok(Some(Session::Stopped));
            }
            Self::Cast { casts, pace, quiet } => {
                let casts: BTreeMap<u32, Cast> = casts
                    .into_iter()
                    .map(|(serial, lines)| (serial, lines.into_iter().collect()))
                    .collect();
                engine.cast(&casts, pace_named(&pace)?, quiet);
            }
            Self::Line {
                key,
                pcm,
                rate,
                lead,
            } => {
                let sample = Sample::new(pcm.0, None, rate).map_err(|error| error.to_string())?;
                let lead = Seconds::new(lead).ok_or("the line's lead is not a time")?;
                engine
                    .line(&key, sample, lead)
                    .map_err(|error| error.to_string())?;
            }
            Self::Said { chain, end } => engine.said(chain, frame(end)?),
            Self::Failed { key } => engine.failed(&key),
            Self::Duck { level } => {
                engine.duck(Gain::new(level).ok_or("the duck's level is not a level")?);
            }
        }
        Ok(None)
    }
}

/// A sample from its channels and rate.
fn sample(left: Floats, right: Floats, rate: f64) -> Result<Sample, String> {
    let right = (!right.0.is_empty()).then_some(right.0);
    Sample::new(left.0, right, rate).map_err(|error| error.to_string())
}

/// A piece from its records.
fn handed(
    serial: u32,
    notes: &Doubles,
    bars: &Doubles,
    seconds: f64,
    cues: &Doubles,
    cast: Lines,
) -> Result<Handed, String> {
    Ok(Handed {
        serial,
        notes: decode_notes(&notes.0).map_err(|error| error.to_string())?,
        bars: bars.0.iter().filter_map(|at| Seconds::new(*at)).collect(),
        seconds: Seconds::new(seconds).ok_or("the piece's length is not a time")?,
        cues: decode_cues(&cues.0).map_err(|error| error.to_string())?,
        cast: cast.into_iter().collect(),
    })
}

fn pace_named(name: &str) -> Result<Pace, String> {
    Pace::named(name).ok_or_else(|| format!("{name} is not a pace"))
}

/// A frame from the page: whole and not negative.
fn frame(value: f64) -> Result<u64, String> {
    (value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value < 9e15)
        .then(|| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // checked above
            let frame = value as u64;
            frame
        })
        .ok_or_else(|| format!("{value} is not a frame"))
}

/// Samples, sent as base64 of their little-endian 32-bit floats (a `Float32Array`'s bytes).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Floats(pub Vec<f32>);

/// Numbers, sent as base64 of their little-endian 64-bit floats (a `Float64Array`'s bytes).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Doubles(pub Vec<f64>);

impl<'de> Deserialize<'de> for Floats {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = base64(&text).map_err(serde::de::Error::custom)?;
        if !bytes.len().is_multiple_of(4) {
            return Err(serde::de::Error::custom(
                "samples are not whole 32-bit floats",
            ));
        }
        Ok(Self(
            bytes
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap_or([0; 4])))
                .collect(),
        ))
    }
}

impl<'de> Deserialize<'de> for Doubles {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bytes = base64(&text).map_err(serde::de::Error::custom)?;
        if !bytes.len().is_multiple_of(8) {
            return Err(serde::de::Error::custom(
                "numbers are not whole 64-bit floats",
            ));
        }
        Ok(Self(
            bytes
                .chunks_exact(8)
                .map(|b| f64::from_le_bytes(b.try_into().unwrap_or([0; 8])))
                .collect(),
        ))
    }
}

/// Decodes standard base64 (RFC 4648 §4, with `=` padding), as the page's `btoa` writes it.
/// Hand-written: thirty lines against a crate for one alphabet and one direction (`CLAUDE.md` §10).
///
/// # Errors
///
/// A character outside the alphabet, or a length that is not whole groups of four.
pub(crate) fn base64(text: &str) -> Result<Vec<u8>, &'static str> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return Err("base64 is not whole groups of four characters");
    }
    let value = |c: u8| -> Result<u32, &'static str> {
        match c {
            b'A'..=b'Z' => Ok(u32::from(c - b'A')),
            b'a'..=b'z' => Ok(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Ok(u32::from(c - b'0') + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("a character that is not base64"),
        }
    };
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let groups = text.len() / 4;
    for (n, group) in text.chunks_exact(4).enumerate() {
        let last = n + 1 == groups;
        let pad = match group {
            [_, _, b'=', b'='] if last => 2,
            [_, _, _, b'='] if last => 1,
            _ => 0,
        };
        let mut word = 0;
        for c in group.iter().take(4 - pad) {
            word = (word << 6) | value(*c)?;
        }
        word <<= 6 * pad;
        let bytes = word.to_be_bytes();
        out.extend(bytes.iter().skip(1).take(3 - pad));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Message, Session, base64};
    use prosody_player::Engine;
    use prosody_synth::SampleRate;

    #[test]
    fn base64_decodes_as_rfc_4648_says() {
        // RFC 4648 §10's test vectors.
        let cases = [
            ("", ""),
            ("Zg==", "f"),
            ("Zm8=", "fo"),
            ("Zm9v", "foo"),
            ("Zm9vYg==", "foob"),
            ("Zm9vYmE=", "fooba"),
            ("Zm9vYmFy", "foobar"),
        ];
        for (encoded, decoded) in cases {
            assert_eq!(base64(encoded).unwrap(), decoded.as_bytes());
        }
        assert!(base64("Zm9").is_err());
        assert!(base64("Zm9*").is_err());
        assert!(base64("Zg==Zg==").is_err());
    }

    #[test]
    fn a_buffer_is_its_little_endian_floats() {
        // 1.0f32 is 00 00 80 3f: base64 "AACAPw=="; 0.5f64 is 00 .. 00 e0 3f: "AAAAAAAA4D8=".
        let drum: Message = serde_json::from_str(
            r#"{"op":"drum","id":3,"left":"AACAPw==","right":"","rate":44100}"#,
        )
        .unwrap();
        let Message::Drum {
            id, left, right, ..
        } = drum
        else {
            unreachable!("not a drum")
        };
        assert_eq!((id, left.0, right.0), (3, vec![1.0], Vec::new()));
        let stop: Message = serde_json::from_str(
            r#"{"op":"swap","serial":2,"notes":"","bars":"AAAAAAAA4D8=","seconds":1,"cues":"","cast":[[0,"k"],[1,null]]}"#,
        )
        .unwrap();
        let Message::Swap { bars, cast, .. } = stop else {
            unreachable!("not a swap")
        };
        assert_eq!(bars.0, vec![0.5]);
        assert_eq!(cast, vec![(0, Some("k".to_owned())), (1, None)]);
        // Three bytes are not a float.
        assert!(
            serde_json::from_str::<Message>(
                r#"{"op":"line","key":"k","pcm":"AAAA","rate":1,"lead":0}"#
            )
            .is_err()
        );
    }

    #[test]
    fn a_play_and_a_stop_say_what_the_notification_shows() {
        let mut engine = Engine::new(SampleRate::new(48_000).unwrap(), 1_024, 4_096);
        let play: Message = serde_json::from_str(
            r#"{"op":"play","generation":1,"serial":1,"notes":"","bars":"","seconds":2,"cues":"","cast":[],"pace":"natural","duck":0.6,"title":"The Tyger"}"#,
        )
        .unwrap();
        assert_eq!(
            play.apply(&mut engine).unwrap(),
            Some(Session::Playing("The Tyger".to_owned()))
        );
        assert!(engine.playing());
        let duck: Message = serde_json::from_str(r#"{"op":"duck","level":1}"#).unwrap();
        assert_eq!(duck.apply(&mut engine).unwrap(), None);
        let stop: Message = serde_json::from_str(r#"{"op":"stop","generation":2}"#).unwrap();
        assert_eq!(stop.apply(&mut engine).unwrap(), Some(Session::Stopped));
        assert!(!engine.playing());
    }

    #[test]
    fn what_is_not_a_message_is_refused_with_why() {
        let mut engine = Engine::new(SampleRate::new(48_000).unwrap(), 1_024, 4_096);
        let brisk: Message = serde_json::from_str(
            r#"{"op":"cast","casts":[[1,[[0,null]]]],"pace":"brisk","quiet":false}"#,
        )
        .unwrap();
        assert!(brisk.apply(&mut engine).unwrap_err().contains("brisk"));
        let said: Message = serde_json::from_str(r#"{"op":"said","chain":1,"end":1.5}"#).unwrap();
        assert!(said.apply(&mut engine).is_err());
        // A zone with no loop (the page sends null for NaN) is a zone.
        let zone: Message = serde_json::from_str(
            r#"{"op":"zone","id":0,"position":0,"left":"AACAPwAAgD8=","right":"","rate":44100,"root":60,"low":0,"high":127,"loopStart":null,"loopEnd":null}"#,
        )
        .unwrap();
        assert_eq!(zone.apply(&mut engine), Ok(None));
    }
}
