//! `prosody-player` — the player around the synth: what keeps it rendering ahead of the output, times
//! the reading on it, and mixes its stems for the speakers (`docs/player.md` §5, §13).
//!
//! The synth (`prosody-synth`) turns notes and samples into stems. Something has to drive it: hand it
//! each piece, render it about half a second ahead of what has been heard, place the reading's lines
//! as their cues come round, ask the page for the recordings it needs, and tell the page where each
//! piece plays from. In the browser that was the render worker's TypeScript until step 5; the app's
//! native player needs the same, and two copies of the reading's timing would drift. So it is here,
//! once, and both run it: the browser's render worker through `prosody-synth-wasm`, the app's render
//! thread directly.
//!
//! - [`Engine`]: the synth rendering ahead, and the reading placed on it.
//! - [`reader`]: when each line of the reading is read, at both paces.
//! - [`Mixer`]: the output's queue and master stage, for the app (the browser's worklet does the same
//!   in TypeScript, `web/src/player/mixer.ts`, pinned by the same numbers).
//! - [`records`]: a Performance's notes and cues as the page hands them over.
//! - [`Event`]: what the page is told, and its JSON.
//!
//! Pure, as the synth is: no IO, no clock, no threads. The host owns those.

// Tests may unwrap, index and fail with a message; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic // a test that finds the wrong case says what it found
    )
)]

mod engine;
mod event;
mod mix;
pub mod reader;
pub mod records;

pub use engine::{Engine, Handed};
pub use event::Event;
pub use mix::{Block, FADE, GLIDE, Mixer, Report};
pub use reader::{Cast, Cue, Pace};
