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
//!
//! Analysis passes are added on top of these primitives; see `PLAN.md` for the build order.

// Tests may unwrap freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod seed;
pub mod span;

pub use seed::{Domain, Seed};
pub use span::Span;
