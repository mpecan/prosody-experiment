//! Byte ranges into the source poem.
//!
//! Invariant I4 says every audible event must be attributable to a specific token span. The way
//! that invariant is kept honest is boring and structural: every node in the intermediate
//! representation carries a [`Span`] as a required field, so a value that cannot say where it
//! came from cannot be constructed in the first place.

use core::ops::Range;

/// A half-open byte range `[start, end)` into the source poem.
///
/// Byte offsets, not character offsets: they are what Tree-sitter reports and what `str`
/// slicing accepts, so no conversion sits between the parser and the debug view where an
/// off-by-one could hide.
///
/// Ordering is by `start`, then `end` — the total order used to break ties when sorting events
/// for emission (see `CLAUDE.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    start: usize,
    end: usize,
}

impl Span {
    /// Creates a span covering `[start, end)`, for callers where `start <= end` is structurally
    /// guaranteed — a range handed over by the parser, or a pair of literals in a test.
    ///
    /// An inverted range is normalised to the empty span at `start` rather than panicking, since
    /// a malformed span must never take down a live-reload session. That normalisation is silent,
    /// which makes this the wrong constructor whenever the bounds come from arithmetic: a
    /// reversed-argument bug would collapse to an innocuous-looking zero-length token instead of
    /// announcing itself. Use [`Span::try_new`] there.
    #[must_use]
    pub const fn new(start: usize, end: usize) -> Self {
        if end < start {
            return Self { start, end: start };
        }
        Self { start, end }
    }

    /// Creates a span covering `[start, end)`, or `None` if the range is inverted.
    ///
    /// The constructor to reach for when `start` and `end` are computed rather than given — a
    /// caesura offset, a phoneme boundary, a window around a token. An inverted range there is a
    /// bug, and `None` forces it into the open at the point it happens.
    #[must_use]
    pub const fn try_new(start: usize, end: usize) -> Option<Self> {
        if end < start {
            return None;
        }
        Some(Self { start, end })
    }

    /// Creates the empty span at `offset`.
    ///
    /// Useful for anchoring a node to a position that has no extent of its own, such as an
    /// implied stanza break at end of input.
    #[must_use]
    pub const fn empty_at(offset: usize) -> Self {
        Self {
            start: offset,
            end: offset,
        }
    }

    /// The first byte offset covered by this span.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// The byte offset one past the last one covered by this span.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// The length of this span in bytes.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Whether this span covers no bytes.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// This span as a [`Range`], for use with `str::get` and friends.
    #[must_use]
    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }

    /// The smallest span covering both `self` and `other`.
    ///
    /// Used when a node's extent is the extent of its children — a line's span is the join of
    /// its tokens' spans.
    #[must_use]
    pub const fn join(self, other: Self) -> Self {
        let start = if self.start < other.start {
            self.start
        } else {
            other.start
        };
        let end = if self.end > other.end {
            self.end
        } else {
            other.end
        };
        Self { start, end }
    }

    /// Whether `other` lies entirely within `self`.
    ///
    /// An empty span is contained by any span covering its offset, so a zero-width anchor inside
    /// a line still reports as belonging to that line.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }

    /// The source text this span covers, or `None` if it does not describe a valid slice of
    /// `source`.
    ///
    /// Returns `None` rather than panicking when the span runs past the end of `source` or falls
    /// inside a multi-byte character. Both are bugs, but they are bugs that should surface as a
    /// visible gap in the debug view rather than as a crash mid-performance — which is also why
    /// `clippy::indexing_slicing` is denied workspace-wide.
    #[must_use]
    pub fn text(self, source: &str) -> Option<&str> {
        source.get(self.range())
    }
}

#[cfg(test)]
mod tests {
    use super::Span;

    #[test]
    fn covers_the_expected_bytes() {
        let span = Span::new(4, 9);
        assert_eq!(span.start(), 4);
        assert_eq!(span.end(), 9);
        assert_eq!(span.len(), 5);
        assert!(!span.is_empty());
    }

    #[test]
    fn inverted_range_collapses_instead_of_panicking() {
        let span = Span::new(9, 4);
        assert_eq!(span, Span::empty_at(9));
        assert!(span.is_empty());
        assert_eq!(span.len(), 0);
    }

    #[test]
    fn try_new_reports_an_inverted_range_rather_than_hiding_it() {
        assert_eq!(Span::try_new(4, 9), Some(Span::new(4, 9)));
        assert_eq!(Span::try_new(4, 4), Some(Span::empty_at(4)));
        assert_eq!(Span::try_new(9, 4), None);
    }

    #[test]
    fn range_matches_the_bounds() {
        assert_eq!(Span::new(3, 7).range(), 3..7);
        assert_eq!(Span::empty_at(3).range(), 3..3);
    }

    #[test]
    fn join_is_the_hull_of_both_spans() {
        // Two tokens with a space between them; the line covering them includes the gap.
        let first = Span::new(0, 4);
        let second = Span::new(5, 11);
        assert_eq!(first.join(second), Span::new(0, 11));
        assert_eq!(second.join(first), Span::new(0, 11));
    }

    #[test]
    fn join_of_a_nested_span_changes_nothing() {
        let outer = Span::new(0, 20);
        let inner = Span::new(5, 8);
        assert_eq!(outer.join(inner), outer);
        assert_eq!(inner.join(outer), outer);
    }

    #[test]
    fn join_of_widely_disjoint_spans_covers_the_gap() {
        // Two stanzas far apart: the span covering both includes everything between them.
        let first = Span::new(0, 12);
        let second = Span::new(140, 160);
        assert_eq!(first.join(second), Span::new(0, 160));
        assert_eq!(second.join(first), Span::new(0, 160));
    }

    #[test]
    fn containment_includes_zero_width_anchors() {
        let line = Span::new(10, 20);
        assert!(line.contains(Span::new(10, 20)));
        assert!(line.contains(Span::empty_at(15)));
        assert!(line.contains(Span::empty_at(10)));
        assert!(line.contains(Span::empty_at(20)));
        assert!(!line.contains(Span::new(9, 15)));
        assert!(!line.contains(Span::new(15, 21)));
        assert!(!line.contains(Span::empty_at(21)));
    }

    #[test]
    fn text_extracts_the_covered_source() {
        let source = "burn burning burnt";
        assert_eq!(Span::new(0, 4).text(source), Some("burn"));
        assert_eq!(Span::new(5, 12).text(source), Some("burning"));
        assert_eq!(Span::empty_at(4).text(source), Some(""));
    }

    #[test]
    fn text_declines_out_of_bounds_spans() {
        let source = "burn";
        assert_eq!(Span::new(0, 5).text(source), None);
        assert_eq!(Span::new(9, 12).text(source), None);
    }

    #[test]
    fn text_declines_spans_inside_a_two_byte_character() {
        // "č" occupies bytes 3..5, so offset 4 is not a character boundary.
        let source = "gorči plamen";
        assert_eq!(source.len(), 13);
        assert_eq!(Span::new(0, 4).text(source), None);
        assert_eq!(Span::new(4, 6).text(source), None);
        assert_eq!(Span::new(0, 5).text(source), Some("gorč"));
    }

    #[test]
    fn text_declines_spans_inside_a_three_byte_character() {
        // "€" occupies bytes 0..3.
        let source = "€ and";
        assert_eq!(source.len(), 7);
        assert_eq!(Span::new(0, 1).text(source), None);
        assert_eq!(Span::new(0, 2).text(source), None);
        assert_eq!(Span::new(1, 3).text(source), None);
        assert_eq!(Span::new(0, 3).text(source), Some("€"));
    }

    #[test]
    fn text_declines_spans_inside_a_four_byte_character() {
        // "🔥" occupies bytes 0..4 — the widest boundary case, and the one most likely to have
        // an off-by-one distinct from the narrower ones.
        let source = "🔥 burns";
        assert_eq!(source.len(), 10);
        for split in 1..4 {
            assert_eq!(Span::new(0, split).text(source), None, "split at {split}");
            assert_eq!(Span::new(split, 6).text(source), None, "split at {split}");
        }
        assert_eq!(Span::new(0, 4).text(source), Some("🔥"));
    }

    #[test]
    fn ordering_is_by_start_then_end() {
        let mut spans = [
            Span::new(5, 6),
            Span::new(0, 9),
            Span::new(0, 4),
            Span::new(5, 5),
        ];
        spans.sort_unstable();
        assert_eq!(
            spans,
            [
                Span::new(0, 4),
                Span::new(0, 9),
                Span::new(5, 5),
                Span::new(5, 6)
            ]
        );
    }
}
