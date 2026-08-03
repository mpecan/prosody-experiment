//! Property tests for the invariants in `SPEC.md` §6.
//!
//! These are written against the invariants themselves rather than against any one pass, and
//! they grow as the pipeline does. At this stage only the seeding primitives exist, so the
//! properties cover I1 (determinism) and I2 (per-unit seeding) at their source, plus the span
//! resolution that I4 rests on. I3 arrives with the style layer; see `PLAN.md`.
//!
//! Every property here must be able to fail. A property that holds by virtue of a function
//! signature rather than by virtue of the implementation is not a test, and two such properties
//! were removed from this file after review rather than left in to inflate the count.

// Property-test bodies unwrap freely, as test code may. See CLAUDE.md §3.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use proptest::prelude::{
    Strategy, prop_assert, prop_assert_eq, prop_assert_ne, prop_assume, proptest,
};
use prosody_core::{Domain, Seed, Span};
use rand_core::Rng;

/// The largest byte offset the span strategies draw. Chosen to sit somewhat beyond the longest
/// source `poem_text()` can produce, so that in-bounds, boundary and out-of-bounds spans all
/// occur at usable rates.
const MAX_OFFSET: usize = 200;

/// Text shaped roughly like a stanza: words, spaces and newlines, plus multi-byte characters at
/// two, three and four bytes wide, so the non-English case from `SPEC.md` §9 is exercised rather
/// than assumed away.
fn poem_text() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Zčšžæø€漢🔥' \n]{0,40}").unwrap()
}

fn domain() -> impl Strategy<Value = Domain> {
    proptest::sample::select(vec![Domain::Stanza, Domain::Line])
}

/// Sub-stream labels must be `'static`, so the strategy selects from a fixed set.
fn label() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        "detune",
        "sample-bank",
        "ornament",
        "velocity",
        "filter",
    ])
}

/// Seeds a whole string as one unit.
fn seed_of(domain: Domain, text: &str) -> Seed {
    Seed::for_unit(domain, Span::new(0, text.len()), text).unwrap()
}

fn draw(seed: &Seed, count: usize) -> Vec<u32> {
    let mut rng = seed.rng();
    (0..count).map(|_| rng.next_u32()).collect()
}

proptest! {
    /// I1 — derivation is a pure function of its inputs, so repeated runs agree, and the
    /// generator built from a seed replays the same stream.
    #[test]
    fn i1_seed_derivation_is_reproducible(domain in domain(), content in poem_text()) {
        let first = seed_of(domain, &content);
        let second = seed_of(domain, &content);
        prop_assert_eq!(first, second);
        prop_assert_eq!(draw(&first, 32), draw(&second, 32));
    }

    /// I2 — a unit's seed depends on the unit's own bytes and on nothing about where those bytes
    /// sit. The same stanza text embedded at different offsets in different poems must seed
    /// identically; if it did not, an edit anywhere before a stanza would move that stanza's
    /// voice, which is precisely the locality failure I2 exists to prevent.
    ///
    /// This is the property that the `Span` parameter of `Seed::for_unit` makes testable: the
    /// call site has to name its bounds, so a test can vary everything outside them.
    #[test]
    fn i2_a_unit_seeds_the_same_wherever_it_sits(
        domain in domain(),
        unit in poem_text(),
        before in poem_text(),
        after in poem_text(),
    ) {
        let alone = seed_of(domain, &unit);

        let embedded = format!("{before}{unit}{after}");
        let span = Span::new(before.len(), before.len() + unit.len());
        // The surrounding text may end mid-character; only a span on real boundaries resolves.
        prop_assume!(span.text(&embedded) == Some(unit.as_str()));

        prop_assert_eq!(Seed::for_unit(domain, span, &embedded).unwrap(), alone);
    }

    /// I2 — widening a span past the unit changes the seed. This is the mistake the old `&str`
    /// API could not distinguish from correct use: passing the whole poem where a stanza was
    /// meant stays deterministic, so only a locality property like this one catches it.
    #[test]
    fn i2_widening_a_span_changes_the_seed(
        unit in poem_text(),
        extra in poem_text(),
    ) {
        prop_assume!(!extra.is_empty());
        let source = format!("{unit}{extra}");
        let narrow = Span::new(0, unit.len());
        let wide = Span::new(0, source.len());
        prop_assume!(narrow.text(&source).is_some());

        prop_assert_ne!(
            Seed::for_unit(Domain::Stanza, narrow, &source).unwrap(),
            Seed::for_unit(Domain::Stanza, wide, &source).unwrap()
        );
    }

    /// I2 — distinct unit content gives distinct seeds.
    #[test]
    fn i2_distinct_content_gives_distinct_seeds(
        domain in domain(),
        a in poem_text(),
        b in poem_text(),
    ) {
        // `prop_assume!` rather than an `if`, so proptest tracks how often the case is vacuous
        // instead of silently asserting nothing.
        prop_assume!(a != b);
        prop_assert_ne!(seed_of(domain, &a), seed_of(domain, &b));
    }

    /// I2 — the two domains never collide for the same text.
    #[test]
    fn i2_domains_stay_separated(content in poem_text()) {
        prop_assert_ne!(seed_of(Domain::Stanza, &content), seed_of(Domain::Line, &content));
    }

    /// I2 — named sub-streams within a unit are independent, and asking for one does not disturb
    /// another.
    #[test]
    fn i2_child_seeds_are_independent(
        content in poem_text(),
        left in label(),
        right in label(),
    ) {
        let parent = seed_of(Domain::Stanza, &content);
        prop_assert_ne!(parent.child(left), parent);

        if left == right {
            prop_assert_eq!(parent.child(left), parent.child(right));
        } else {
            prop_assert_ne!(parent.child(left), parent.child(right));
            prop_assert_ne!(draw(&parent.child(left), 8), draw(&parent.child(right), 8));
        }
    }

    /// I4 — a span resolves to exactly the bytes it claims, or declines for a reason.
    ///
    /// Both halves matter. Without the `None` arm, an implementation that never resolved
    /// anything would pass; without the offset check, one that returned a correctly-sized slice
    /// from the wrong position would pass. Offsets are drawn near the source length so that
    /// in-bounds, boundary and out-of-bounds cases all actually occur — drawing them from the
    /// whole `usize` range would make the resolving case vanishingly rare.
    #[test]
    fn i4_spans_resolve_to_their_claimed_bytes_or_decline(
        source in poem_text(),
        start in 0_usize..=MAX_OFFSET,
        end in 0_usize..=MAX_OFFSET,
    ) {
        let span = Span::new(start, end);
        match span.text(&source) {
            Some(text) => {
                prop_assert_eq!(text.len(), span.len());
                // The slice must begin at the offset the span claims, not merely occur somewhere
                // in the source.
                let offset = text.as_ptr().addr() - source.as_ptr().addr();
                prop_assert_eq!(offset, span.start());
            }
            None => {
                // Declining is only allowed for a span that genuinely cannot be sliced.
                prop_assert!(
                    span.end() > source.len()
                        || !source.is_char_boundary(span.start())
                        || !source.is_char_boundary(span.end()),
                    "declined a span that was in bounds and on character boundaries"
                );
            }
        }
    }

    /// I4 — a node's span covers its children's spans, which is what makes attribution
    /// transitive: if an event traces to a token, it traces to the line containing that token.
    #[test]
    fn i4_join_covers_both_operands(
        a_start in 0_usize..200,
        a_len in 0_usize..50,
        b_start in 0_usize..200,
        b_len in 0_usize..50,
    ) {
        let a = Span::new(a_start, a_start + a_len);
        let b = Span::new(b_start, b_start + b_len);
        let joined = a.join(b);

        prop_assert!(joined.contains(a));
        prop_assert!(joined.contains(b));
        prop_assert_eq!(joined, b.join(a));
        prop_assert!(joined.len() >= a.len().max(b.len()));
    }

    /// I4 — containment is reflexive and transitive, so a token inside a line inside a stanza is
    /// attributable to that stanza.
    #[test]
    fn i4_containment_is_transitive(
        outer_start in 0_usize..100,
        outer_len in 0_usize..100,
        inner_offset in 0_usize..100,
        inner_len in 0_usize..100,
        leaf_offset in 0_usize..100,
        leaf_len in 0_usize..100,
    ) {
        let outer = Span::new(outer_start, outer_start + outer_len);
        prop_assert!(outer.contains(outer));

        let inner = Span::new(outer_start + inner_offset, outer_start + inner_offset + inner_len);
        let leaf = Span::new(inner.start() + leaf_offset, inner.start() + leaf_offset + leaf_len);

        if outer.contains(inner) && inner.contains(leaf) {
            prop_assert!(outer.contains(leaf));
        }
    }
}
