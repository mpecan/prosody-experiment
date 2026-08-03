//! Parsing a poem into the units the analysis passes work on.
//!
//! This module owns the boundary between tree-sitter and the rest of the system. Everything
//! downstream sees [`Poem`], [`Stanza`], [`Line`] and [`Token`] — plain values, each carrying the
//! [`Span`] it came from — and never touches a `tree_sitter::Node`. Keeping the CST behind that
//! boundary is what lets the analysis passes stay pure and independently testable, as
//! `SPEC.md` §10 asks.
//!
//! # Spans are content spans
//!
//! A unit's span covers exactly the text that produced it: a line's span is the extent of its
//! tokens, and a stanza's is the extent of its lines. Neither includes the newline that ends it.
//!
//! This is not cosmetic. Tree-sitter gives a line's trailing newline to the *stanza break* when
//! another stanza follows and to the *line* when the stanza is last, so a raw node range would
//! make a stanza's extent — and therefore its seed — depend on whether anything comes after it.
//! Appending a new stanza would then silently re-voice the previous one, which is exactly the
//! locality failure invariant I2 exists to forbid. Deriving spans from content instead makes a
//! unit's identity depend only on the unit.
//!
//! # Incremental reparsing
//!
//! [`Poem::apply`] reuses the previous tree rather than reparsing from scratch. `SPEC.md` §7.1
//! insists on this from day one, and the reason is invariant I2 rather than speed: an edit
//! confined to one stanza must leave the other stanzas' units — and therefore their seeds and
//! their voices — untouched. That property is tested directly in `tests/invariants.rs`.

use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

use crate::seed::{Domain, Fingerprint, Seed};
use crate::span::Span;

/// The style-header delimiter (`SPEC.md` §5). TOML, so `+++` rather than YAML's `---`.
const STYLE_DELIMITER: &str = "+++";

/// What went wrong.
///
/// Note what is *not* here: a poem the grammar cannot fully understand is not an error.
/// Tree-sitter recovers, the trouble is reported by [`Poem::diagnostics`], and the rest of the
/// poem still produces sound. A live-coding session must not go silent because a line is
/// half-typed.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    /// The span handed to [`Poem::apply`] is not a valid slice of the poem.
    #[error("span {0:?} is not a valid slice of this poem")]
    InvalidSpan(Span),
    /// The grammar could not be loaded — a build problem, not a poem problem.
    #[error("could not load the prosody grammar")]
    Grammar(#[from] tree_sitter::LanguageError),
    /// Tree-sitter declined to produce a tree at all.
    #[error("the parser returned no tree")]
    NoTree,
}

/// Something about the poem worth showing the writer, which is not fatal.
///
/// `CLAUDE.md` §6 requires fallbacks to be visible. These are the visible part: every one of them
/// marks text that will not become sound, or text that is probably not doing what its author
/// intended. The debug view (`SPEC.md` §8) renders them alongside the poem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    /// The text the diagnostic is about.
    pub span: Span,
    /// What is wrong with it.
    pub kind: DiagnosticKind,
}

/// The kinds of trouble [`Poem::diagnostics`] reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticKind {
    /// The parser could not interpret this text, so it produces no voices.
    Unparsed,
    /// A line that is just `+++` turned up in the poem body.
    ///
    /// Almost always a style header that failed to be recognised — an unclosed block, or one
    /// that does not start at the very first byte. Without this diagnostic the header's TOML
    /// would quietly become score, which is invariant I3 violated in the most literal way
    /// available, and tree-sitter would report nothing because the result is a perfectly valid
    /// parse of the wrong thing.
    StrayStyleDelimiter,
}

/// What kind of thing a token is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A word. Hyphens and apostrophes stay inside it: `half-light` is one word.
    Word,
    /// A number, including internal separators, so `1,000` and `3:14` are single tokens rather
    /// than a digit, a caesura and another digit.
    Number,
    /// A mid-line pause — `,` `;` `:` `--` `—` `–`.
    ///
    /// `SPEC.md` §4.1 wants a caesura to be able to split a voice. Marking it here is what makes
    /// that possible later; whether it *does* split a voice is a role-assignment decision.
    Caesura,
    /// Sentence punctuation: `.` `...` `!` `?` and quotes or brackets.
    Punctuation,
}

/// A single token of the poem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    span: Span,
    kind: TokenKind,
}

impl Token {
    /// Where this token sits in the source.
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// What kind of token this is.
    #[must_use]
    pub const fn kind(self) -> TokenKind {
        self.kind
    }
}

/// The position of a stanza within a poem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StanzaIdx(pub usize);

/// The position of a line within a stanza.
///
/// Distinct from [`StanzaIdx`] so that the two cannot be transposed at a call site — the bug
/// would otherwise return a valid seed for the wrong unit, which nothing could detect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LineIdx(pub usize);

/// One line of the poem — a bar, or a phrase (`SPEC.md` §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    span: Span,
    tokens: Vec<Token>,
}

impl Line {
    /// The extent of the line's tokens, excluding the newline that ends it.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The line's tokens, in source order.
    #[must_use]
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }

    /// The line's words only, skipping numbers, caesurae and punctuation.
    pub fn words(&self) -> impl Iterator<Item = &Token> {
        self.tokens.iter().filter(|t| t.kind == TokenKind::Word)
    }

    /// The spans between caesurae, which are the candidate sub-voice boundaries of
    /// `SPEC.md` §4.1's second tier.
    ///
    /// A line with no caesura yields one segment covering the whole line. A line with nothing but
    /// caesurae yields none, there being no content to segment.
    #[must_use]
    pub fn segments(&self) -> Vec<Span> {
        let mut segments = Vec::new();
        let mut current: Option<Span> = None;
        for token in &self.tokens {
            if token.kind == TokenKind::Caesura {
                if let Some(span) = current.take() {
                    segments.push(span);
                }
            } else {
                current = Some(current.map_or(token.span, |span| span.join(token.span)));
            }
        }
        if let Some(span) = current {
            segments.push(span);
        }
        segments
    }
}

/// A stanza — the unit delimited by blank lines, and the default unit of role assignment
/// (`SPEC.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stanza {
    span: Span,
    lines: Vec<Line>,
}

impl Stanza {
    /// The extent of the stanza's lines, excluding any trailing newline or blank lines.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The stanza's lines, in source order.
    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }
}

/// A parsed poem, and the tree it came from.
#[derive(Debug)]
pub struct Poem {
    source: String,
    tree: Tree,
    frontmatter: Option<Span>,
    stanzas: Vec<Stanza>,
    diagnostics: Vec<Diagnostic>,
}

impl Poem {
    /// Parses a poem from scratch.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError`] only if the grammar cannot be loaded or tree-sitter returns no
    /// tree. A poem the grammar cannot fully understand still parses; see [`Poem::diagnostics`].
    pub fn parse(source: impl Into<String>) -> Result<Self, ParseError> {
        let source = source.into();
        let tree = parse_with(&source, None)?;
        Ok(Self::from_tree(source, tree))
    }

    /// Replaces the bytes covered by `span` with `replacement`, reparsing incrementally.
    ///
    /// # Errors
    ///
    /// [`ParseError::InvalidSpan`] if `span` is not a valid slice of the current source, or the
    /// variants [`Poem::parse`] returns. On any error the poem is left exactly as it was.
    pub fn apply(&mut self, span: Span, replacement: &str) -> Result<(), ParseError> {
        let invalid = || ParseError::InvalidSpan(span);
        let prefix = self.source.get(..span.start()).ok_or_else(invalid)?;
        let through_end = self.source.get(..span.end()).ok_or_else(invalid)?;
        let suffix = self.source.get(span.end()..).ok_or_else(invalid)?;

        let start_position = point_of(prefix);
        let edit = InputEdit {
            start_byte: span.start(),
            old_end_byte: span.end(),
            new_end_byte: span.start() + replacement.len(),
            start_position,
            old_end_position: point_of(through_end),
            new_end_position: advance(start_position, replacement),
        };
        let new_source = format!("{prefix}{replacement}{suffix}");

        // Edit a clone. `Tree::edit` shifts the tree to describe an edit that has not happened
        // yet; if the reparse then fails, an in-place edit would leave this poem holding a tree
        // that no longer corresponds to its own source, and the *next* reparse would silently
        // build wrong spans from it. Cloning is cheap — the tree is reference-counted.
        let mut edited = self.tree.clone();
        edited.edit(&edit);

        let tree = parse_with(&new_source, Some(&edited))?;
        *self = Self::from_tree(new_source, tree);
        Ok(())
    }

    /// The poem's source text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The style header's extent, if the poem has one (`SPEC.md` §5).
    ///
    /// The block is opaque here: it is TOML, and parsing it belongs to the style layer. Keeping
    /// it out of the stanza list is what stops a style header from being read as score. When a
    /// header is present but malformed, this is `None` and a
    /// [`DiagnosticKind::StrayStyleDelimiter`] says so.
    #[must_use]
    pub const fn frontmatter(&self) -> Option<Span> {
        self.frontmatter
    }

    /// The poem's stanzas, in source order.
    #[must_use]
    pub fn stanzas(&self) -> &[Stanza] {
        &self.stanzas
    }

    /// Everything worth telling the writer about this poem.
    ///
    /// Non-empty is not fatal — the rest of the poem is still usable — but these regions either
    /// produce no voices or probably do not mean what they appear to.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// A stanza, by index.
    #[must_use]
    pub fn stanza(&self, index: StanzaIdx) -> Option<&Stanza> {
        self.stanzas.get(index.0)
    }

    /// A line, by stanza and line index.
    #[must_use]
    pub fn line(&self, stanza: StanzaIdx, line: LineIdx) -> Option<&Line> {
        self.stanzas.get(stanza.0)?.lines.get(line.0)
    }

    /// The seed for a stanza.
    ///
    /// Returns `None` if there is no such stanza, or if its span does not resolve — which would
    /// be a bug in this module rather than in the poem.
    #[must_use]
    pub fn stanza_seed(&self, index: StanzaIdx) -> Option<Seed> {
        let stanza = self.stanza(index)?;
        Seed::for_unit(Domain::Stanza, stanza.span(), &self.source)
    }

    /// A fingerprint of the whole poem, identifying which poem an analysis describes.
    ///
    /// **Never a seed.** Seeding anything from this would be a direct I2 violation — it is the
    /// global seed I2 exists to forbid. It is here because passes hand their results to each
    /// other and spans cannot tell two poems apart: "war is kind maiden" and "one two three four"
    /// occupy exactly the same bytes, so a role pass given one poem's meter and another's
    /// repetitions would invent confident refrains between lines that have never met.
    ///
    #[must_use]
    pub fn fingerprint(&self) -> Fingerprint {
        Fingerprint::of(&self.source)
    }

    /// The seed for a line.
    #[must_use]
    pub fn line_seed(&self, stanza: StanzaIdx, line: LineIdx) -> Option<Seed> {
        let line = self.line(stanza, line)?;
        Seed::for_unit(Domain::Line, line.span(), &self.source)
    }

    fn from_tree(source: String, tree: Tree) -> Self {
        let mut frontmatter = None;
        let mut stanzas = Vec::new();
        let mut diagnostics = Vec::new();

        // Scoped so the cursor's borrow of `tree` ends before `tree` is moved into `Self`.
        {
            let root = tree.root_node();
            collect_diagnostics(root, &mut diagnostics);

            let mut cursor = root.walk();
            for node in root.children(&mut cursor) {
                match node.kind() {
                    "frontmatter" => frontmatter = Some(span_of(node.byte_range())),
                    "stanza" => stanzas.push(stanza_from(node)),
                    _ => {}
                }
            }
        }

        // A `+++` line that survived into the body means the style header was not recognised.
        for stanza in &stanzas {
            for line in &stanza.lines {
                if line.span().text(&source).map(str::trim) == Some(STYLE_DELIMITER) {
                    diagnostics.push(Diagnostic {
                        span: line.span(),
                        kind: DiagnosticKind::StrayStyleDelimiter,
                    });
                }
            }
        }

        diagnostics.sort_unstable_by_key(|d| d.span);

        Self {
            source,
            tree,
            frontmatter,
            stanzas,
            diagnostics,
        }
    }
}

fn parse_with(source: &str, old: Option<&Tree>) -> Result<Tree, ParseError> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_prosody::LANGUAGE.into())?;
    parser.parse(source, old).ok_or(ParseError::NoTree)
}

/// Records every region tree-sitter could not interpret, at any depth.
///
/// Depth matters: an unlexable character produces an `ERROR` nested inside a `line`, not at the
/// root, so a root-only scan reports a clean parse while the character silently disappears from
/// every token list — text with no attribution, which invariant I4 forbids.
fn collect_diagnostics(node: Node<'_>, out: &mut Vec<Diagnostic>) {
    if node.is_error() || node.is_missing() {
        out.push(Diagnostic {
            span: span_of(node.byte_range()),
            kind: DiagnosticKind::Unparsed,
        });
        return;
    }
    if !node.has_error() {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_diagnostics(child, out);
    }
}

fn stanza_from(node: Node<'_>) -> Stanza {
    let mut lines = Vec::new();
    let mut cursor = node.walk();
    for line_node in node.children(&mut cursor) {
        if line_node.kind() != "line" {
            continue;
        }
        let mut tokens = Vec::new();
        let mut token_cursor = line_node.walk();
        for token_node in line_node.children(&mut token_cursor) {
            let kind = match token_node.kind() {
                "word" => TokenKind::Word,
                "number" => TokenKind::Number,
                "caesura" => TokenKind::Caesura,
                "punctuation" => TokenKind::Punctuation,
                _ => continue,
            };
            tokens.push(Token {
                span: span_of(token_node.byte_range()),
                kind,
            });
        }
        // Content span, not the node's range: see the module doc. A line always has at least one
        // token by construction of the grammar; the fallback keeps this total regardless.
        let span =
            hull(tokens.iter().map(|t| t.span)).unwrap_or_else(|| span_of(line_node.byte_range()));
        lines.push(Line { span, tokens });
    }
    let span = hull(lines.iter().map(Line::span)).unwrap_or_else(|| span_of(node.byte_range()));
    Stanza { span, lines }
}

/// The smallest span covering all of `spans`, or `None` if there are none.
fn hull(spans: impl Iterator<Item = Span>) -> Option<Span> {
    spans.reduce(Span::join)
}

/// Tree-sitter guarantees `start <= end`, so `Span::new` is the right constructor here.
fn span_of(range: core::ops::Range<usize>) -> Span {
    Span::new(range.start, range.end)
}

/// The row and column at the end of `text`.
///
/// Tree-sitter wants a `Point` alongside each byte offset in an edit, and its columns are byte
/// counts, not character counts. Taking the text *up to* the offset rather than the offset itself
/// means there is no invalid input to handle: the caller has already sliced.
fn point_of(text: &str) -> Point {
    Point {
        row: text.matches('\n').count(),
        column: text
            .rfind('\n')
            .map_or(text.len(), |last| text.len() - last - 1),
    }
}

/// The point reached by appending `added` to text that ends at `start`.
fn advance(start: Point, added: &str) -> Point {
    added.rfind('\n').map_or(
        Point {
            row: start.row,
            column: start.column + added.len(),
        },
        |last| Point {
            row: start.row + added.matches('\n').count(),
            column: added.len() - last - 1,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticKind, LineIdx, Poem, StanzaIdx, TokenKind};
    use crate::span::Span;

    const TWO_STANZAS: &str = "the fire that burns\nthe fire that burnt\n\nand the tide came in\n";

    fn words_of(poem: &Poem, stanza: usize, line: usize) -> Vec<&str> {
        poem.line(StanzaIdx(stanza), LineIdx(line))
            .unwrap()
            .words()
            .filter_map(|t| t.span().text(poem.source()))
            .collect()
    }

    #[test]
    fn splits_stanzas_on_blank_lines() {
        let poem = Poem::parse(TWO_STANZAS).unwrap();
        assert_eq!(poem.stanzas().len(), 2);
        assert_eq!(poem.stanzas()[0].lines().len(), 2);
        assert_eq!(poem.stanzas()[1].lines().len(), 1);
        assert!(poem.diagnostics().is_empty());
    }

    #[test]
    fn spans_cover_content_only() {
        let poem = Poem::parse(TWO_STANZAS).unwrap();
        assert_eq!(
            poem.stanzas()[0].span().text(poem.source()),
            Some("the fire that burns\nthe fire that burnt")
        );
        assert_eq!(
            poem.stanzas()[1].span().text(poem.source()),
            Some("and the tide came in")
        );
        assert_eq!(
            poem.stanzas()[0].lines()[0].span().text(poem.source()),
            Some("the fire that burns")
        );
    }

    #[test]
    fn a_stanza_keeps_its_seed_when_another_stanza_is_appended() {
        // The locality bug this module's span definition exists to prevent: tree-sitter hands a
        // line's trailing newline to the stanza break once a stanza follows, so a raw node range
        // would change stanza 0's extent — and its seed — without touching stanza 0's text.
        let mut poem = Poem::parse("the fire that burns\n").unwrap();
        let before = poem.stanza_seed(StanzaIdx(0)).unwrap();
        let before_span = poem.stanzas()[0].span();

        let end = Span::new(poem.source().len(), poem.source().len());
        poem.apply(end, "\nand the tide came in\n").unwrap();

        assert_eq!(poem.stanzas().len(), 2);
        assert_eq!(poem.stanzas()[0].span(), before_span);
        assert_eq!(poem.stanza_seed(StanzaIdx(0)).unwrap(), before);
    }

    #[test]
    fn a_stanza_keeps_its_seed_when_a_later_stanza_is_edited() {
        let mut poem = Poem::parse(TWO_STANZAS).unwrap();
        let before = poem.stanza_seed(StanzaIdx(0)).unwrap();
        let target = poem.stanzas()[1].lines()[0].tokens()[2].span();
        poem.apply(target, "flood").unwrap();
        assert_eq!(poem.stanza_seed(StanzaIdx(0)).unwrap(), before);
    }

    #[test]
    fn line_seeds_are_derived_and_distinct_from_stanza_seeds() {
        let poem = Poem::parse("burns\n\nburns\n").unwrap();
        let line = poem.line_seed(StanzaIdx(0), LineIdx(0)).unwrap();
        let stanza = poem.stanza_seed(StanzaIdx(0)).unwrap();

        // Same text, different domain, so the two must not collide.
        assert_eq!(
            poem.stanzas()[0].span().text(poem.source()),
            poem.line(StanzaIdx(0), LineIdx(0))
                .unwrap()
                .span()
                .text(poem.source())
        );
        assert_ne!(line, stanza);

        // A second stanza with identical text is a refrain: same seed (SPEC.md §4.3).
        assert_eq!(poem.line_seed(StanzaIdx(1), LineIdx(0)).unwrap(), line);
    }

    #[test]
    fn line_seeds_follow_the_line_that_changed() {
        let mut poem = Poem::parse("burns\nburnt\n").unwrap();
        let first = poem.line_seed(StanzaIdx(0), LineIdx(0)).unwrap();
        let second = poem.line_seed(StanzaIdx(0), LineIdx(1)).unwrap();

        let target = poem.line(StanzaIdx(0), LineIdx(1)).unwrap().span();
        poem.apply(target, "burning").unwrap();

        assert_eq!(poem.line_seed(StanzaIdx(0), LineIdx(0)).unwrap(), first);
        assert_ne!(poem.line_seed(StanzaIdx(0), LineIdx(1)).unwrap(), second);
    }

    #[test]
    fn missing_units_have_no_seed() {
        let poem = Poem::parse("burns\n").unwrap();
        assert!(poem.stanza_seed(StanzaIdx(9)).is_none());
        assert!(poem.line_seed(StanzaIdx(0), LineIdx(9)).is_none());
        assert!(poem.line_seed(StanzaIdx(9), LineIdx(0)).is_none());
    }

    #[test]
    fn a_wider_gap_is_still_one_stanza_break() {
        let poem = Poem::parse("a\n\n\n\n\nb\n").unwrap();
        assert_eq!(poem.stanzas().len(), 2);
    }

    #[test]
    fn frontmatter_is_kept_out_of_the_stanzas() {
        let source = "+++\n[style]\nroot = \"D\"\n+++\nthe fire that burns\n";
        let poem = Poem::parse(source).unwrap();
        assert_eq!(
            poem.frontmatter().and_then(|s| s.text(source)),
            Some("+++\n[style]\nroot = \"D\"\n+++\n")
        );
        // The style header must not appear as score. See invariant I3.
        assert_eq!(poem.stanzas().len(), 1);
        assert_eq!(words_of(&poem, 0, 0), ["the", "fire", "that", "burns"]);
        assert!(poem.diagnostics().is_empty());
    }

    #[test]
    fn frontmatter_tolerates_trailing_whitespace_after_its_delimiters() {
        let source = "+++  \nroot = \"D\"\n+++\t\nburns\n";
        let poem = Poem::parse(source).unwrap();
        assert!(poem.frontmatter().is_some());
        assert_eq!(poem.stanzas().len(), 1);
        assert!(poem.diagnostics().is_empty());
    }

    #[test]
    fn an_unclosed_style_header_is_reported_rather_than_silently_scored() {
        // Without the diagnostic this parses cleanly as poem text, and the TOML becomes score.
        let source = "+++\nscale = \"phrygian_dominant\"\n\nthe fire that burns\n";
        let poem = Poem::parse(source).unwrap();
        assert_eq!(poem.frontmatter(), None);
        let kinds: Vec<_> = poem.diagnostics().iter().map(|d| d.kind).collect();
        assert_eq!(kinds, [DiagnosticKind::StrayStyleDelimiter]);
        assert_eq!(
            poem.diagnostics()[0].span.text(source),
            Some(super::STYLE_DELIMITER)
        );
    }

    #[test]
    fn a_style_header_not_at_the_first_byte_is_reported() {
        let source = "\n+++\nroot = \"D\"\n+++\nburns\n";
        let poem = Poem::parse(source).unwrap();
        assert_eq!(poem.frontmatter(), None);
        assert_eq!(
            poem.diagnostics()
                .iter()
                .filter(|d| d.kind == DiagnosticKind::StrayStyleDelimiter)
                .count(),
            2
        );
    }

    #[test]
    fn a_poem_without_frontmatter_reports_none() {
        let poem = Poem::parse("the fire that burns\n").unwrap();
        assert_eq!(poem.frontmatter(), None);
        assert!(poem.diagnostics().is_empty());
    }

    #[test]
    fn unparsable_text_is_reported_even_when_nested_in_a_line() {
        // A form feed is whitespace to the lexer but is not skipped, so it produces an ERROR
        // nested inside a line. A root-only scan would report a clean parse while the byte
        // vanished from every token list.
        let source = "foo\u{0c}bar\n";
        let poem = Poem::parse(source).unwrap();
        let unparsed: Vec<_> = poem
            .diagnostics()
            .iter()
            .filter(|d| d.kind == DiagnosticKind::Unparsed)
            .collect();
        assert_eq!(unparsed.len(), 1);
        assert_eq!(unparsed[0].span.text(source), Some("\u{0c}"));
    }

    #[test]
    fn a_leading_nul_byte_is_reported() {
        // A NUL *inside* a word is absorbed into it — still attributed, so not a diagnostic —
        // but one where a token cannot start is genuinely unparsable.
        let poem = Poem::parse("\0\n").unwrap();
        assert!(
            poem.diagnostics()
                .iter()
                .any(|d| d.kind == DiagnosticKind::Unparsed)
        );

        let absorbed = Poem::parse("bu\0rns\n").unwrap();
        assert!(absorbed.diagnostics().is_empty());
        assert_eq!(absorbed.stanzas()[0].lines()[0].tokens().len(), 1);
    }

    #[test]
    fn every_token_kind_survives_the_mapping() {
        let source = "half-light, 1,000 gone... yes\n";
        let poem = Poem::parse(source).unwrap();
        let got: Vec<_> = poem.stanzas()[0].lines()[0]
            .tokens()
            .iter()
            .map(|t| (t.kind(), t.span().text(source).unwrap()))
            .collect();
        assert_eq!(
            got,
            [
                (TokenKind::Word, "half-light"),
                (TokenKind::Caesura, ","),
                (TokenKind::Number, "1,000"),
                (TokenKind::Word, "gone"),
                (TokenKind::Punctuation, "..."),
                (TokenKind::Word, "yes"),
            ]
        );
    }

    #[test]
    fn a_numeral_is_not_a_caesura() {
        let source = "1,000 miles at 3:14\n";
        let poem = Poem::parse(source).unwrap();
        let line = &poem.stanzas()[0].lines()[0];
        assert_eq!(
            line.tokens()
                .iter()
                .filter(|t| t.kind() == TokenKind::Caesura)
                .count(),
            0
        );
        assert_eq!(line.segments().len(), 1);
    }

    #[test]
    fn hyphens_and_apostrophes_stay_inside_words() {
        let poem = Poem::parse("half-light, burn'd -- gone\n").unwrap();
        assert_eq!(words_of(&poem, 0, 0), ["half-light", "burn'd", "gone"]);
        assert_eq!(
            poem.stanzas()[0].lines()[0]
                .tokens()
                .iter()
                .filter(|t| t.kind() == TokenKind::Caesura)
                .count(),
            2
        );
    }

    #[test]
    fn segments_split_a_line_at_its_caesurae() {
        let source = "gorči plamen, tiho gori\n";
        let poem = Poem::parse(source).unwrap();
        let segments = poem.stanzas()[0].lines()[0].segments();
        let texts: Vec<_> = segments.iter().filter_map(|s| s.text(source)).collect();
        assert_eq!(texts, ["gorči plamen", "tiho gori"]);
    }

    #[test]
    fn segments_handle_caesurae_at_the_edges() {
        let cases = [
            (", burns\n", vec!["burns"]),
            ("burns,\n", vec!["burns"]),
            ("burns,, gone\n", vec!["burns", "gone"]),
            (",\n", vec![]),
            (",,\n", vec![]),
        ];
        for (source, expected) in cases {
            let poem = Poem::parse(source).unwrap();
            let segments = poem.stanzas()[0].lines()[0].segments();
            let texts: Vec<_> = segments.iter().filter_map(|s| s.text(source)).collect();
            assert_eq!(texts, expected, "for {source:?}");
        }
    }

    #[test]
    fn a_line_without_a_caesura_is_one_segment() {
        let source = "the fire that burns\n";
        let poem = Poem::parse(source).unwrap();
        let segments = poem.stanzas()[0].lines()[0].segments();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text(source), Some("the fire that burns"));
    }

    #[test]
    fn non_english_text_tokenises() {
        let poem = Poem::parse("gorči plamen tiho\n").unwrap();
        assert_eq!(words_of(&poem, 0, 0), ["gorči", "plamen", "tiho"]);
    }

    #[test]
    fn degenerate_inputs_parse_without_diagnostics() {
        for source in ["", "\n", "\n\n\n", "a", "   \n  \n", "+++\nx = 1\n+++\n"] {
            let poem = Poem::parse(source).unwrap();
            assert!(
                poem.diagnostics().is_empty(),
                "unexpected diagnostic in {source:?}"
            );
        }
    }

    #[test]
    fn an_edit_reparses_and_updates_the_affected_stanza() {
        let mut poem = Poem::parse(TWO_STANZAS).unwrap();
        let target = poem.stanzas()[1].lines()[0].tokens()[2].span();
        assert_eq!(target.text(poem.source()), Some("tide"));

        poem.apply(target, "flood").unwrap();

        assert_eq!(poem.stanzas().len(), 2);
        assert_eq!(words_of(&poem, 1, 0), ["and", "the", "flood", "came", "in"]);
    }

    #[test]
    fn an_edit_may_delete_text() {
        let mut poem = Poem::parse("burns bright\n").unwrap();
        let target = poem.stanzas()[0].lines()[0].tokens()[1].span();
        poem.apply(Span::new(target.start() - 1, target.end()), "")
            .unwrap();
        assert_eq!(poem.source(), "burns\n");
        assert_eq!(words_of(&poem, 0, 0), ["burns"]);
    }

    #[test]
    fn an_edit_may_split_and_merge_stanzas() {
        let mut poem = Poem::parse("a\nb\n").unwrap();
        assert_eq!(poem.stanzas().len(), 1);

        // Split: turn the single newline between the lines into a blank line.
        poem.apply(Span::new(1, 2), "\n\n").unwrap();
        assert_eq!(poem.source(), "a\n\nb\n");
        assert_eq!(poem.stanzas().len(), 2);

        // Merge again.
        poem.apply(Span::new(1, 3), "\n").unwrap();
        assert_eq!(poem.source(), "a\nb\n");
        assert_eq!(poem.stanzas().len(), 1);
    }

    #[test]
    fn an_edit_with_an_unresolvable_span_changes_nothing() {
        let mut poem = Poem::parse(TWO_STANZAS).unwrap();
        let before = poem.source().to_owned();
        assert!(poem.apply(Span::new(0, 9_999), "x").is_err());
        assert_eq!(poem.source(), before);
        assert_eq!(poem.stanzas().len(), 2);
    }

    #[test]
    fn an_edit_inside_a_multibyte_character_is_refused() {
        let mut poem = Poem::parse("gorči\n").unwrap();
        let before = poem.source().to_owned();
        // "č" occupies bytes 3..5.
        assert!(poem.apply(Span::new(0, 4), "x").is_err());
        assert_eq!(poem.source(), before);
    }

    #[test]
    fn point_of_and_advance_track_rows_and_columns() {
        use tree_sitter::Point;
        assert_eq!(super::point_of(""), Point { row: 0, column: 0 });
        assert_eq!(super::point_of("ab"), Point { row: 0, column: 2 });
        assert_eq!(super::point_of("ab\n"), Point { row: 1, column: 0 });
        assert_eq!(super::point_of("ab\ncd"), Point { row: 1, column: 2 });

        let start = Point { row: 1, column: 2 };
        assert_eq!(super::advance(start, "xy"), Point { row: 1, column: 4 });
        assert_eq!(super::advance(start, "x\ny"), Point { row: 2, column: 1 });
        assert_eq!(super::advance(start, "\n"), Point { row: 2, column: 0 });
        // Byte columns, not character columns — which is what tree-sitter expects.
        assert_eq!(super::advance(start, "č"), Point { row: 1, column: 4 });
    }
}
