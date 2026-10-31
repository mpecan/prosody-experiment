//! What the engine says to the page, and the JSON it says it in.
//!
//! The page reads the same objects whichever engine it drives — the render worker in the browser
//! posts them, the app's native player hands them over when the page asks — so they are written in
//! one place. The shapes are the render worker's messages from before step 5 (`web/src/player`
//! `index.ts` `heard`, `speech.ts` `told`), unchanged.
//!
//! Hand-written rather than through a serialisation crate: six shapes of numbers, booleans and short
//! strings, where a dependency would be the whole of `serde` in a crate that is otherwise only the
//! synth (`CLAUDE.md` §10).

use std::fmt::Write as _;

use prosody_synth::Losses;

use crate::reader::Told;

/// Something the page is told.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Where a piece plays from: from output frame `out_start`, its own frame `piece_start`.
    Placement {
        /// The piece, by the serial the page handed it with.
        serial: u32,
        /// The output frame it takes over on.
        out_start: u64,
        /// Its own frame it plays from there.
        piece_start: u64,
    },
    /// What the synth has lost so far.
    Losses(Losses),
    /// The recorded lines to send.
    Want(Vec<String>),
    /// The recorded lines let go: to be sent again when wanted.
    Dropped(Vec<String>),
    /// What the reading did, in start of play `generation`.
    Reading {
        /// The start of play it belongs to.
        generation: u32,
        /// What.
        told: Told,
    },
    /// Something that failed, to be said.
    Error(String),
}

impl Event {
    /// The event as a JSON object.
    #[must_use]
    pub fn json(&self) -> String {
        let mut out = String::new();
        match self {
            Self::Placement {
                serial,
                out_start,
                piece_start,
            } => {
                let _ = write!(
                    out,
                    r#"{{"op":"placement","outStart":{out_start},"pieceStart":{piece_start},"serial":{serial}}}"#
                );
            }
            Self::Losses(losses) => {
                let _ = write!(
                    out,
                    r#"{{"op":"losses","stolen":{},"missing":{}}}"#,
                    losses.stolen, losses.missing
                );
            }
            Self::Want(keys) => keyed(&mut out, "want", keys),
            Self::Dropped(keys) => keyed(&mut out, "dropped", keys),
            Self::Reading { generation, told } => {
                let _ = write!(out, r#"{{"op":"reading","generation":{generation},"told":"#);
                told_json(&mut out, told);
                out.push('}');
            }
            Self::Error(message) => {
                out.push_str(r#"{"op":"error","message":"#);
                string(&mut out, message);
                out.push('}');
            }
        }
        out
    }
}

/// `{"op":op,"keys":[...]}`.
fn keyed(out: &mut String, op: &str, keys: &[String]) {
    let _ = write!(out, r#"{{"op":"{op}","keys":["#);
    for (n, key) in keys.iter().enumerate() {
        if n > 0 {
            out.push(',');
        }
        string(out, key);
    }
    out.push_str("]}");
}

/// What the reader told, as `reader.ts`'s `Told` was.
fn told_json(out: &mut String, told: &Told) {
    match told {
        Told::Spoken {
            serial,
            line,
            pace,
            start,
            end,
            over,
            late,
        } => {
            let _ = write!(
                out,
                r#"{{"op":"spoken","serial":{serial},"line":{line},"pace":"{}","start":{start},"end":{end},"over":"#,
                pace.name()
            );
            number(out, *over);
            out.push_str(r#","late":"#);
            number(out, Some(*late));
            out.push('}');
        }
        Told::Say {
            serial,
            line,
            pace,
            at,
            late,
            chain,
        } => {
            let _ = write!(
                out,
                r#"{{"op":"say","serial":{serial},"line":{line},"pace":"{}","at":{at},"late":{late},"chain":{chain}}}"#,
                pace.name()
            );
        }
        Told::Cut { at } => {
            let _ = write!(out, r#"{{"op":"cut","at":{at}}}"#);
        }
        Told::Wait { serial, line, key } => {
            let _ = write!(
                out,
                r#"{{"op":"wait","serial":{serial},"line":{line},"key":"#
            );
            string(out, key);
            out.push('}');
        }
        Told::Note { text, loss } => {
            out.push_str(r#"{"op":"note","text":"#);
            string(out, text);
            let _ = write!(out, r#","loss":{loss}}}"#);
        }
        Told::At { serial, pace, line } => {
            let _ = write!(
                out,
                r#"{{"op":"at","serial":{serial},"pace":"{}","line":"#,
                pace.name()
            );
            match line {
                Some(line) => {
                    let _ = write!(out, "{line}");
                }
                None => out.push_str("null"),
            }
            out.push('}');
        }
    }
}

/// A number, or `null` for none or for one JSON cannot write (an infinity).
fn number(out: &mut String, value: Option<f64>) {
    match value.filter(|value| value.is_finite()) {
        Some(value) => {
            let _ = write!(out, "{value}");
        }
        None => out.push_str("null"),
    }
}

/// A JSON string: quoted, with the characters JSON requires escaped.
fn string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::Event;
    use crate::reader::{Pace, Told};
    use prosody_synth::Losses;

    #[test]
    fn each_event_is_the_object_the_page_reads() {
        let cases = [
            (
                Event::Placement {
                    serial: 3,
                    out_start: 48_000,
                    piece_start: 0,
                },
                r#"{"op":"placement","outStart":48000,"pieceStart":0,"serial":3}"#,
            ),
            (
                Event::Losses(Losses {
                    stolen: 2,
                    missing: 1,
                }),
                r#"{"op":"losses","stolen":2,"missing":1}"#,
            ),
            (
                Event::Want(vec!["kokoro|af|a \"b\"".to_owned(), "x".to_owned()]),
                r#"{"op":"want","keys":["kokoro|af|a \"b\"","x"]}"#,
            ),
            (Event::Dropped(Vec::new()), r#"{"op":"dropped","keys":[]}"#),
            (
                Event::Error("line\none\\".to_owned()),
                r#"{"op":"error","message":"line\none\\"}"#,
            ),
        ];
        for (event, json) in cases {
            assert_eq!(event.json(), json);
        }
    }

    #[test]
    fn what_the_reader_told_is_its_old_shape() {
        let reading = |told| Event::Reading {
            generation: 4,
            told,
        };
        let cases = [
            (
                Told::Spoken {
                    serial: 1,
                    line: 2,
                    pace: Pace::Fitted,
                    start: 100,
                    end: 700,
                    over: Some(0.5),
                    late: 0.0,
                },
                r#"{"op":"spoken","serial":1,"line":2,"pace":"fitted","start":100,"end":700,"over":0.5,"late":0}"#,
            ),
            (
                Told::Spoken {
                    serial: 1,
                    line: 2,
                    pace: Pace::Fitted,
                    start: 100,
                    end: 700,
                    over: None,
                    late: 0.25,
                },
                r#"{"op":"spoken","serial":1,"line":2,"pace":"fitted","start":100,"end":700,"over":null,"late":0.25}"#,
            ),
            (
                Told::Say {
                    serial: 1,
                    line: 0,
                    pace: Pace::Natural,
                    at: 10,
                    late: true,
                    chain: 3,
                },
                r#"{"op":"say","serial":1,"line":0,"pace":"natural","at":10,"late":true,"chain":3}"#,
            ),
            (Told::Cut { at: 9 }, r#"{"op":"cut","at":9}"#),
            (
                Told::Wait {
                    serial: 2,
                    line: 1,
                    key: "k".to_owned(),
                },
                r#"{"op":"wait","serial":2,"line":1,"key":"k"}"#,
            ),
            (
                Told::Note {
                    text: "not read".to_owned(),
                    loss: false,
                },
                r#"{"op":"note","text":"not read","loss":false}"#,
            ),
            (
                Told::At {
                    serial: 2,
                    pace: Pace::Natural,
                    line: None,
                },
                r#"{"op":"at","serial":2,"pace":"natural","line":null}"#,
            ),
        ];
        for (told, json) in cases {
            assert_eq!(
                reading(told).json(),
                format!(r#"{{"op":"reading","generation":4,"told":{json}}}"#)
            );
        }
    }
}
