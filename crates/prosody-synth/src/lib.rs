//! `prosody-synth` — the player's synth: notes and decoded samples in, PCM out
//! (`docs/player.md` §4).
//!
//! A [`Piece`] is a Performance's notes placed on frames; a [`Renderer`] renders it block by block
//! into [`Stems`], one stereo stem per [`Channel`], looping it round as Strudel's pattern loops and
//! handing over to an edited piece at a bar. Each note becomes a voice from a fixed pool: a
//! sampler (a drum played through, or a soundfont zone pitched to the key and looped while held)
//! or a band-limited oscillator, through a linear-attack, exponential-release envelope, an RBJ
//! low-pass and its level. A voice that has to be cut off for want of room, and a note whose sound
//! was never loaded, are counted ([`Losses`]): a fallback is visible (`CLAUDE.md` §6).
//!
//! # Purity
//!
//! No IO, no clock, no allocation once rendering has warmed up, and no randomness: the same notes
//! and samples give the same PCM, bit for bit, on every platform (`docs/player.md` §2 extends I1
//! from text to audio). That last part is why the synth computes its own sines and powers
//! ([`math`]) rather than calling the platform's: those differ in the last bit between systems.
//!
//! # Clean room
//!
//! Strudel's synth (`superdough`) is AGPL-3.0, and this crate is MIT. Nothing here is translated
//! from it; every stage is from published technique, cited where it is used, and is to be matched
//! to Strudel's *sound* by measuring its output (`docs/player.md` §4).

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp, // a test may compare a sample with the exact value it must have
        clippy::cast_precision_loss, // a test's frame counts are small
        clippy::cast_possible_truncation // a test's expected samples are written as f32
    )
)]

mod env;
mod filter;
pub mod master;
pub mod math;
mod note;
mod osc;
mod render;
mod sample;
mod stems;
mod units;
mod voice;
pub mod wav;

pub use env::Envelope;
pub use filter::{BUTTERWORTH_Q, Biquad, Coefficients};
pub use note::{Channel, DEFAULT_ATTACK, DEFAULT_RELEASE, Note, NoteSpec, Source};
pub use osc::{Oscillator, Wave};
pub use render::{Losses, Piece, Placement, Renderer, Repeat};
pub use sample::{Bank, Instrument, Loop, MAX_INSTRUMENTS, Sample, Zone};
pub use stems::{STEMS, Stems};
pub use units::{Gain, Hertz, Midi, SampleRate, Seconds};
pub use voice::VOICES;

/// Why something handed to the synth cannot be used. Each says what it was about: a note by its
/// tag — its index in the Performance, and so its words (I4) — an instrument by its number.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SynthError {
    /// A sample rate outside what the synth runs at.
    #[error("a sample rate of {0} frames a second is outside 8000..=384000")]
    Rate(u32),
    /// A sample that cannot be played.
    #[error("a sample cannot be played: {0}")]
    Sample(&'static str),
    /// A soundfont zone that is not one.
    #[error("a soundfont zone for keys {low}..={high} is not a zone")]
    Zone {
        /// Its lowest key.
        low: u8,
        /// Its highest key.
        high: u8,
    },
    /// An instrument number past the bank's end.
    #[error("instrument {0} is past the bank's {max}", max = MAX_INSTRUMENTS)]
    Instrument(u32),
    /// A pitched sound with no pitch.
    #[error("note {tag} asks for a pitched sound and has no pitch")]
    Unpitched {
        /// The note's index in the Performance.
        tag: u32,
    },
    /// A note handed over in a form that does not describe one.
    #[error("note {tag} is not a note: {why}")]
    Malformed {
        /// The note's index in the Performance.
        tag: u32,
        /// What is wrong with it.
        why: &'static str,
    },
}
