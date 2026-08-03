//! Content-derived PRNG seeds.
//!
//! Invariant I2 requires that randomness be seeded per unit, from a hash of *that unit's own
//! content*, never globally. The reason is locality: with a global seed, changing one word
//! reshuffles the whole piece, which makes the system render-and-pray rather than live-codeable.
//! With per-unit seeds, an edit perturbs only the units whose text actually changed.
//!
//! There is deliberately no way to obtain a randomly-seeded generator from this module. Every
//! [`Seed`] traces back to a [`Span`] of some source text.
//!
//! # What this module does and does not give you
//!
//! Deriving from content alone means two units with byte-identical text receive identical seeds
//! and therefore identical voices. That is the refrain row of `SPEC.md` §4.3 — "whole line
//! repeats → literal loop of that voice" — falling out of the design at no cost.
//!
//! It is **only** the refrain row. The morphological near-repetition row of the same table wants
//! the opposite response to a small edit: "burn / burning / burnt" should stay one voice with
//! modulated parameters, and that requires an identity which is *stable* under small edits.
//! Seeds are the opposite of stable by construction — every derivation below is an avalanche, so
//! a one-character change yields an unrelated stream. A voice's identity must therefore come
//! from the repetition pass that groups related units, never from these seeds; seeds supply only
//! the residual per-unit jitter within an already-established voice. Deriving voice identity
//! from a [`Seed`] would turn the highest-value mapping in the spec into three unrelated
//! instruments, which is `SPEC.md` §1's failure mode exactly. See `PLAN.md`.

use core::fmt;

use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;

use crate::span::Span;

/// The kind of unit a seed belongs to.
///
/// Seeds are domain-separated so that a line and a stanza with byte-identical text still receive
/// unrelated randomness — otherwise a single-line stanza would have its line voice and its
/// stanza voice move in lockstep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Domain {
    /// A stanza: the unit delimited by blank lines, and the default unit of role assignment.
    Stanza,
    /// A line.
    Line,
    /// The whole poem.
    ///
    /// Not a seeding unit — seeding a whole poem is exactly what invariant I2 forbids. This
    /// exists so an analysis can record *which* poem it describes, since spans are bare byte
    /// offsets and two different poems with the same line lengths have identical ones.
    Poem,
}

impl Domain {
    /// The key-derivation context string for this domain.
    ///
    /// These are versioned and must be treated as frozen: changing one changes every seed
    /// derived under it, and therefore every rendering of every existing poem. That is a
    /// deliberate wall in front of an easy way to break invariant I1 for the whole corpus.
    /// `frozen_derivations_*` in the tests below pin each one.
    const fn context(self) -> &'static str {
        match self {
            Self::Stanza => "prosody-core seed v1 stanza",
            Self::Line => "prosody-core seed v1 line",
            Self::Poem => "prosody-core seed v1 poem",
        }
    }
}

/// The key-derivation context for sub-streams. Frozen, for the same reason as [`Domain::context`].
const CHILD_CONTEXT: &str = "prosody-core seed v1 child";

/// A deterministic PRNG seed derived from a single unit's content.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seed([u8; 32]);

impl Seed {
    /// Derives the seed for the unit occupying `span` of `source`.
    ///
    /// Taking a [`Span`] rather than a `&str` is what keeps invariant I2 honest. The rule is
    /// that a unit's seed depends on that unit's own text and nothing else — but a bare `&str`
    /// parameter cannot tell a stanza from the whole poem, so a caller could pass the entire
    /// source, or a growing prefix, and the result would still be perfectly deterministic. I1
    /// would hold, I2 would be broken, and no test of this function could tell. Requiring a span
    /// means the caller has to name the bounds it is claiming, which is also the same
    /// information invariant I4 needs, so the two invariants are enforced by one argument.
    ///
    /// Returns `None` when `span` does not describe a valid slice of `source` — out of bounds,
    /// or splitting a multi-byte character. That is a bug in the caller, and it surfaces as a
    /// visibly unseeded unit in the debug view rather than as silently plausible audio.
    #[must_use]
    pub fn for_unit(domain: Domain, span: Span, source: &str) -> Option<Self> {
        let content = span.text(source)?;
        Some(Self(blake3::derive_key(
            domain.context(),
            content.as_bytes(),
        )))
    }

    /// Derives an independent sub-seed for one particular decision within this unit.
    ///
    /// A unit generally needs several unrelated random choices — which sample from a bank, how
    /// much to detune, where to place an ornament. Drawing them all from one stream makes them
    /// order-dependent, so adding a new decision silently changes every decision after it.
    /// Naming each stream instead keeps them independent and stable under reordering.
    ///
    /// `label` is `&'static str` deliberately: a label must identify a *decision*, not carry
    /// data from the poem. Poem-derived text is never `'static`, so that misuse does not
    /// compile.
    #[must_use]
    pub fn child(&self, label: &'static str) -> Self {
        // The parent seed is a fixed 32 bytes, so the split point between it and the label is
        // fixed and the concatenation is an unambiguous encoding of the pair.
        let mut hasher = blake3::Hasher::new_derive_key(CHILD_CONTEXT);
        hasher.update(&self.0);
        hasher.update(label.as_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    /// Constructs the generator for this seed.
    ///
    /// `ChaCha8Rng` is specified to be reproducible across platforms and across releases of the
    /// crate; `StdRng` explicitly is not, and `thread_rng` is not seeded from anything at all.
    /// Invariant I1 rules both of those out, so this is the only generator in the system.
    ///
    /// Call this **once per `Seed` value**. Two decisions that each call `.rng()` on the same
    /// seed get the same draws, which is the correlation [`Seed::child`] exists to prevent —
    /// give each decision its own named child and call `.rng()` on that.
    #[must_use]
    pub fn rng(&self) -> ChaCha8Rng {
        ChaCha8Rng::from_seed(self.0)
    }

    /// The raw seed bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Renders the seed as lowercase hex, for the debug annotation view required by `SPEC.md` §8.
impl fmt::Display for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Seed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Seed({self})")
    }
}

#[cfg(test)]
mod tests {
    use rand_core::Rng;

    use super::{Domain, Seed};
    use crate::span::Span;

    /// Seeds a whole string as if it were one unit.
    fn seed_of(domain: Domain, text: &str) -> Seed {
        Seed::for_unit(domain, Span::new(0, text.len()), text).unwrap()
    }

    /// Draws a short prefix of a generator's output, as a stand-in for "the choices a unit makes".
    fn stream(seed: &Seed) -> [u32; 8] {
        let mut rng = seed.rng();
        let mut out = [0_u32; 8];
        for slot in &mut out {
            *slot = rng.next_u32();
        }
        out
    }

    #[test]
    fn derivation_is_deterministic() {
        let text = "the fire that burns\nthe fire that burnt";
        let once = seed_of(Domain::Stanza, text);
        let twice = seed_of(Domain::Stanza, text);
        assert_eq!(once, twice);
        assert_eq!(stream(&once), stream(&twice));
    }

    #[test]
    fn a_unit_is_seeded_by_its_own_slice_only() {
        // The property the Span parameter exists to enforce: the same stanza embedded in two
        // different poems must seed identically. Passing the whole source instead of the unit's
        // span is now a visibly different call, not an indistinguishable one.
        let first_poem = "the tide came in\n\nand then withdrew";
        let second_poem = "a different opening\n\nthe tide came in";

        let from_first = Seed::for_unit(Domain::Stanza, Span::new(0, 16), first_poem).unwrap();
        let from_second = Seed::for_unit(Domain::Stanza, Span::new(21, 37), second_poem).unwrap();

        assert_eq!(from_first.to_string(), from_second.to_string());
        assert_eq!(Span::new(0, 16).text(first_poem), Some("the tide came in"));
        assert_eq!(
            Span::new(21, 37).text(second_poem),
            Some("the tide came in")
        );
    }

    #[test]
    fn seeding_a_whole_poem_differs_from_seeding_one_stanza_of_it() {
        // The mistake the old `&str` API could not distinguish from correct use.
        let poem = "the tide came in\n\nand then withdrew";
        let stanza = Seed::for_unit(Domain::Stanza, Span::new(0, 16), poem).unwrap();
        let whole = Seed::for_unit(Domain::Stanza, Span::new(0, poem.len()), poem).unwrap();
        assert_ne!(stanza, whole);
    }

    #[test]
    fn an_unresolvable_span_yields_no_seed() {
        let poem = "gorči plamen";
        // Past the end.
        assert!(Seed::for_unit(Domain::Line, Span::new(0, 99), poem).is_none());
        // Inside the two-byte "č".
        assert!(Seed::for_unit(Domain::Line, Span::new(0, 4), poem).is_none());
    }

    #[test]
    fn identical_content_yields_identical_seeds() {
        // The refrain rule (SPEC.md §4.3): a repeated line is a literal loop of that voice.
        let first = seed_of(Domain::Line, "and the tide came in");
        let refrain = seed_of(Domain::Line, "and the tide came in");
        assert_eq!(first, refrain);
    }

    #[test]
    fn a_one_character_edit_changes_the_seed() {
        let before = seed_of(Domain::Line, "the fire that burns");
        let after = seed_of(Domain::Line, "the fire that burnt");
        assert_ne!(before, after);
        assert_ne!(stream(&before), stream(&after));
    }

    #[test]
    fn whitespace_is_content() {
        // Stanza structure is carried by whitespace, so it must not be normalised away here.
        assert_ne!(
            seed_of(Domain::Stanza, "a b"),
            seed_of(Domain::Stanza, "a  b")
        );
    }

    #[test]
    fn domains_are_separated() {
        let text = "a single line that is also a stanza";
        assert_ne!(seed_of(Domain::Stanza, text), seed_of(Domain::Line, text));
    }

    #[test]
    fn child_seeds_are_independent_of_their_parent_and_each_other() {
        let parent = seed_of(Domain::Stanza, "burn burning burnt");
        let bank = parent.child("sample-bank");
        let detune = parent.child("detune");

        assert_ne!(bank, parent);
        assert_ne!(detune, parent);
        assert_ne!(bank, detune);
        assert_ne!(stream(&bank), stream(&detune));
    }

    #[test]
    fn child_seeds_are_stable_under_reordering() {
        // Asking for "detune" first must not change what "sample-bank" answers.
        let parent = seed_of(Domain::Stanza, "burn burning burnt");
        let bank_first = parent.child("sample-bank");
        let _ = parent.child("detune");
        let bank_again = parent.child("sample-bank");
        assert_eq!(bank_first, bank_again);
    }

    #[test]
    fn child_labels_are_unambiguous() {
        // Naive concatenation would make ("ab", "c") and ("a", "bc") collide; the parent seed's
        // fixed width is what prevents it. Guard the property rather than the implementation.
        let parent = seed_of(Domain::Stanza, "x");
        assert_ne!(parent.child("ab").child("c"), parent.child("a").child("bc"));
    }

    #[test]
    fn an_empty_child_label_is_still_a_distinct_stream() {
        let parent = seed_of(Domain::Stanza, "burn");
        assert_ne!(parent.child(""), parent);
        assert_eq!(parent.child(""), parent.child(""));
        assert_ne!(parent.child(""), parent.child("detune"));
    }

    #[test]
    fn empty_content_is_a_valid_seed() {
        let empty = seed_of(Domain::Stanza, "");
        assert_eq!(empty, seed_of(Domain::Stanza, ""));
        assert_ne!(empty, seed_of(Domain::Stanza, " "));
    }

    #[test]
    fn display_is_sixty_four_hex_characters() {
        let rendered = seed_of(Domain::Line, "burn").to_string();
        assert_eq!(rendered.len(), 64);
        assert!(
            rendered
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    // The three derivation contexts are frozen: editing one silently rerenders every existing
    // poem. These pin all three. A failure here means a context string changed — see
    // `Domain::context`. The expected values are captured from the implementation, which is the
    // only thing a KDF output can be pinned against; that is the point of a freeze test and is
    // not the "copied from the implementation" hazard CLAUDE.md §7 warns about, because there is
    // no hand-derivable correct answer to compare with.

    #[test]
    fn frozen_derivation_stanza() {
        assert_eq!(
            seed_of(Domain::Stanza, "burn").to_string(),
            "b7c9715540c6c2c56e878c6fd4afc2c00cb77dcb3bdad9904e208974c13e63eb"
        );
    }

    #[test]
    fn frozen_derivation_line() {
        assert_eq!(
            seed_of(Domain::Line, "burn").to_string(),
            "a31efd2eb0a642799bc3dbe51ad51f89f5efcb3c54739f3f7addbbc578d9ddb0"
        );
    }

    #[test]
    fn frozen_derivation_child() {
        assert_eq!(
            seed_of(Domain::Stanza, "burn").child("detune").to_string(),
            "02696af4dfaf8a60fcd071b702bd2cf2b2a831429cfe14896c6f3fcb3ec24060"
        );
    }
}
