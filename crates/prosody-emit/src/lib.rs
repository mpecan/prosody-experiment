//! Turning a [`Score`] into something that makes a sound: `SPEC.md` §7.6.
//!
//! > Backend emit — Strudel or Tidal pattern text first. Cheapest possible audition path; no DSP
//! > work needed to hear whether the mapping is any good.
//!
//! # This crate is where invariant I3 is enforced
//!
//! [`Style`] lives here, not in `prosody-core`. Since `prosody-emit` depends on `prosody-core` and
//! not the reverse, the analysis and lowering passes *cannot* import a style — the compiler will
//! not let them. I3 says "style is palette, not structure"; before this crate that was a claim in
//! a doc comment, and now it is a dependency edge, which is the same mechanism `CLAUDE.md` §2 uses
//! to keep the analysis passes free of IO.
//!
//! The division that follows: a `Score` says *what happens* and a `Style` says *what it sounds
//! like*. Every event, its bar, its beat, its subdivision, its velocity, which voice plays it and
//! which scale degree it takes — all fixed before this crate is reached. What a degree sounds
//! like, which bank the drums come from, how fast it runs: decided here and nowhere else.
//!
//! # Determinism
//!
//! Realisation is where floats finally appear as *values*, and `CLAUDE.md` §4 allows that only
//! through a fixed formatter. Everything printed goes through [`fmt_param`], at three decimal
//! places, so output is byte-identical across platforms and Rust versions.

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic
    )
)]

pub mod osc;
pub mod reading;
pub mod strudel;
pub mod style;

use prosody_core::voice::Score;

pub use osc::Osc;
pub use reading::{Cue, Reading, ReadingError};
pub use strudel::{Strudel, StrudelError};
pub use style::{
    Bpm, Kit, Note, Octave, RideFallback, Scale, Style, StyleError, Swing, TempoRange, Voicing,
};

/// A target that can play a poem.
///
/// Two implementations, and the second is why [`Backend::Output`] is associated rather than fixed
/// to `String`: [`Strudel`] emits pattern text and [`Osc`] emits a schedule of timed messages. A
/// trait that promised text would have had to be replaced rather than implemented, and the two
/// backends differ in what they can express — the OSC one honours `SPEC.md` §4.3's anchors, which
/// mini-notation structurally cannot.
pub trait Backend {
    /// What playing the score produces.
    type Output;
    /// What can go wrong.
    type Error;

    /// The backend's name, for the CLI and the debug view.
    fn name(&self) -> &'static str;

    /// Renders a score under a palette.
    ///
    /// Must be deterministic: the same score and style always produce the same output, byte for
    /// byte (invariant I1).
    ///
    /// # Errors
    ///
    /// Whatever the backend cannot express.
    fn render(&self, score: &Score, style: &Style) -> Result<Self::Output, Self::Error>;
}

/// Formats a parameter for output, at a fixed width.
///
/// The only way a float reaches a backend's output. `CLAUDE.md` §4 permits floats as values and
/// forbids them in identity or control flow, and the remaining hazard is printing: `{}` on an
/// `f64` produces the shortest representation that round-trips, which is a property of the value's
/// exact bits and so can differ where the arithmetic differs. Three decimals is finer than any
/// audible difference in a gain or a cutoff, and it is the same three decimals everywhere.
///
/// Negative zero is normalised, so a value that arrived as `-0.0` prints as `0.000` rather than
/// `-0.000`.
#[must_use]
pub fn fmt_param(value: f64) -> String {
    let rounded = if value == 0.0 { 0.0 } else { value };
    format!("{rounded:.3}")
}

#[cfg(test)]
mod tests {
    use super::fmt_param;

    #[test]
    fn parameters_print_at_a_fixed_width() {
        assert_eq!(fmt_param(1.0), "1.000");
        assert_eq!(fmt_param(0.333_333_333), "0.333");
        assert_eq!(fmt_param(0.0), "0.000");
        assert_eq!(fmt_param(-0.0), "0.000", "negative zero is normalised");
    }

    #[test]
    fn a_third_prints_the_same_however_it_was_computed() {
        // The point of the helper: `1.0/3.0` and `2.0/6.0` are different bit patterns on some
        // paths, and `{}` would print them differently. This is what keeps output byte-stable.
        assert_eq!(fmt_param(1.0 / 3.0), fmt_param(2.0 / 6.0));
        assert_eq!(fmt_param(0.1 + 0.2), fmt_param(0.3));
    }
}
