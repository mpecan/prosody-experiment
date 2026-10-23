//! Pure, deterministic analysis of a poem into musical structure.
//!
//! This crate is the analysis half of the system described in `SPEC.md`: text goes in, musical
//! structure comes out, and nothing in between touches the filesystem, the clock, the
//! environment, or the network. That restriction is not stylistic — it is what makes invariant
//! I1 (same poem + same style produces bit-identical output) checkable rather than hoped for.
//!
//! # Contents
//!
//! - [`span`] — byte ranges into the source poem, carried by every node that will ever produce
//!   sound. This is the mechanism behind invariant I4 (traceability).
//! - [`seed`] — PRNG seeds derived from a single unit's own content, never globally. This is
//!   the mechanism behind invariant I2 (per-unit seeding).
//! - [`parse`] — the tree-sitter boundary: text in, span-carrying stanzas and lines out.
//! - [`phoneme`] — how words sound, and the [`Phonemiser`] boundary `SPEC.md` §9 asks for.
//! - [`meter`] — stress vectors, feet, and the metrical variance `SPEC.md` §4.1 assigns roles by.
//! - [`rhyme`] — continuous rhyme scoring and the classes `SPEC.md` §4.5 maps to scale degrees.
//! - [`repetition`] — the figures `SPEC.md` §4.3 turns into loops, ostinati and cadences.
//! - [`sound`] — §4.3's phonetic rows: word families, alliteration, assonance.
//! - [`surprisal`] — §4.4's distinctiveness metric, which drives sharpness and brightness.
//! - [`role`] — §4.1's load-bearing decision: which stanzas are foundation and which flourish.
//! - [`voice`] — the IR of §3: sounding events, with nothing a style could supply.
//! - [`lower`] — §7.5, turning every analysis above into that IR.
//! - [`accent`] — the light function words, a candidate: a one-syllable function word struck
//!   lightly where the meter's scansion calls its position weak (§4.2's stress → accent).
//! - [`mapping`] — which variant of each candidate mapping applies, so each can be heard against
//!   the others and against its absence.
//! - [`bass`] — §4.6's bass: by vowel height, or walking on every syllable or every foot.
//! - [`harmony`] — §4.6's harmony layer, a candidate: a chord per foundation bar, from the rhyme
//!   scheme or from rhyme closure.
//! - [`arrange`] — laying that IR out in time: stanzas loop until a competing stanza transforms
//!   them or a refrain stanza overrules them.
//! - [`stack`] — the straight stack, a candidate: a line stacked under a bar's host fitted to the
//!   host's subdivision of each beat where the two would cross.
//! - [`annotate`] — §8's debug view: the poem, with what each token became written under it.
//!
//! Analysis passes are added on top of these primitives; see `PLAN.md` for the build order.

// Tests may unwrap and index freely; production code may not. A panic in a test is a test
// failure, which is exactly what it should be. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

pub mod accent;
pub mod annotate;
pub mod arrange;
pub mod bass;
pub mod harmony;
pub mod lower;
pub mod mapping;
pub mod meter;
pub mod parse;
pub mod phoneme;
pub mod repetition;
pub mod rhyme;
pub mod role;
pub mod seed;
pub mod sound;
pub mod span;
pub mod stack;
pub mod surprisal;
pub mod voice;

pub use annotate::{Annotated, AnnotatedLine, Annotation, Pass};
pub use arrange::{Arrangement, Cause, Fit, Lane, Part, Placement, Slot};
pub use bass::{BassNote, BassReason, Chromatic, Direction, Height, ScaleStep};
pub use harmony::{Chord, Function, Harmony, Reason, Rest, Silence};
pub use mapping::{Choice, Mapping, Mappings, Variant};
pub use meter::{
    Beat, Foot, FootCount, FootKind, LineMeter, PoemMeter, StanzaMeter, SyllableCount, Variance,
};
pub use parse::{
    Diagnostic, DiagnosticKind, Line, LineIdx, ParseError, Poem, Stanza, StanzaIdx, Token,
    TokenKind,
};
pub use phoneme::{
    Fallback, Inventory, Phoneme, PhonemeClass, Phonemiser, Pronunciation, Provenance, Stress,
    SyllabifyError, Syllable, Unpronounceable,
};
pub use repetition::{Figure, FigureKind, PoemRepetition, VoiceId};
pub use rhyme::{LineRhyme, PoemRhyme, RhymeClassId, RhymeScore, StanzaRhyme};
pub use role::{DifferentPoems, Layer, LineBasis, LineRole, PoemRoles, StanzaBasis, StanzaRole};
pub use seed::{Domain, Seed};
pub use sound::{Distance, SoundFigure, SoundFigureKind, SoundRepetition, WordForm, WordVoiceId};
pub use span::Span;
pub use surprisal::{Lexicon, PoemSurprisal, Rarity, Surprisal, WordSurprisal};
pub use voice::{
    Bar, BeatIndex, Detune, Event, Family, Pitch, PitchSource, Placed, ScaleDegree, Score,
    Straightened, Subdivision, Velocity, Voice,
};
