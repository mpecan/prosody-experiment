//! Repetition: the figures a poem repeats, and the voices they become.
//!
//! `SPEC.md` §4.3 calls this the highest-value pass, and is emphatic that repetition is not one
//! thing. A whole line returning is a literal loop; a shared line opening is an ostinato anchored
//! to the downbeat; a shared line ending is a cadence. Different figures, different musical
//! results, so they are detected and reported separately rather than collapsed into "these lines
//! are similar".
//!
//! This module covers the exact figures, which are facts about text and need no phonemiser.
//! Alliteration, assonance and morphological near-repetition are phonetic and come next; §7.4
//! says to ship exact first, and it is right — the exact cases are what the varied ones will be
//! measured *against*.
//!
//! # Why a voice is not a seed
//!
//! A [`VoiceId`] says "these lines are the same instrument". It is assigned here, from the text,
//! and deliberately not derived from a [`Seed`](crate::Seed): seeds avalanche, so a one-character
//! edit produces an unrelated stream. That is exactly right for §4.3's refrain row, where
//! identical text should give an identical voice — and exactly wrong for the morphological row,
//! where "burn", "burning" and "burnt" must stay *one* voice with varied parameters. A voice has
//! to survive small edits; a seed must not. Deriving one from the other would turn the mapping
//! the spec says earns the project into three unrelated instruments.

use std::collections::BTreeMap;

use crate::parse::{Line, Poem};
use crate::span::Span;

/// Which voice a line belongs to — which instrument plays it.
///
/// Numbered in order of first appearance, so the first distinct line in a poem is voice 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceId(usize);

impl VoiceId {
    /// The voice number, counting from zero.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// The kind of repetition a figure is.
///
/// Named for the rhetorical figures `SPEC.md` §4.3 names, because each maps to a different
/// musical result and collapsing them would lose that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FigureKind {
    /// A whole line repeated. Becomes a literal loop of that voice.
    Refrain,
    /// A shared line *opening*. Becomes an ostinato anchored to the downbeat.
    Anaphora,
    /// A shared line *ending*. Becomes a cadence figure, or a turnaround.
    Epistrophe,
}

impl FigureKind {
    /// The conventional name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Refrain => "refrain",
            Self::Anaphora => "anaphora",
            Self::Epistrophe => "epistrophe",
        }
    }
}

/// One place a figure occurs.
///
/// Holds each repeated word's span rather than a single hull over them. A hull silently absorbs
/// whatever sits between the words: `"and, the tide"` and `"and the tide"` are the same
/// three-word figure, but their hulls cover different text, so a span would have claimed text
/// nobody repeated. Invariant I4 promises an event traces to *its* span, and a hull does not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    words: Vec<Span>,
}

impl Occurrence {
    /// The repeated words, in order.
    #[must_use]
    pub fn words(&self) -> &[Span] {
        &self.words
    }

    /// The smallest span covering all of them.
    ///
    /// Convenient for a debug view, but it may include punctuation that is not part of the
    /// figure — use [`Occurrence::words`] when the exact text matters.
    #[must_use]
    pub fn hull(&self) -> Option<Span> {
        self.words.iter().copied().reduce(Span::join)
    }
}

/// One repeated figure, and everywhere it occurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Figure {
    kind: FigureKind,
    occurrences: Vec<Occurrence>,
}

impl Figure {
    /// Which figure this is.
    #[must_use]
    pub const fn kind(&self) -> FigureKind {
        self.kind
    }

    /// How many words the repeated part is.
    ///
    /// `SPEC.md` §4.3 detects anaphora and epistrophe on repeated n-grams; this is the n. For a
    /// refrain the repeated part is the whole line, so this is the line's word count — the same
    /// meaning, applied to a longer run.
    #[must_use]
    pub fn words(&self) -> usize {
        self.occurrences.first().map_or(0, |o| o.words.len())
    }

    /// Where the figure occurs, in source order.
    ///
    /// Each occurrence names only the repeated words, not the whole line — a two-word anaphora in
    /// a nine-word line names those two words, which is what invariant I4 asks for and what §4.3's
    /// downbeat anchoring actually attaches to.
    #[must_use]
    pub fn occurrences(&self) -> &[Occurrence] {
        &self.occurrences
    }
}

/// One line's place in the poem's repetitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRepetition {
    line: Span,
    voice: VoiceId,
}

impl LineRepetition {
    /// The line's extent.
    #[must_use]
    pub const fn line(&self) -> Span {
        self.line
    }

    /// Which voice plays this line.
    #[must_use]
    pub const fn voice(&self) -> VoiceId {
        self.voice
    }
}

/// A stanza's repetitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanzaRepetition {
    span: Span,
    lines: Vec<LineRepetition>,
}

impl StanzaRepetition {
    /// The stanza's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The stanza's lines, in order.
    #[must_use]
    pub fn lines(&self) -> &[LineRepetition] {
        &self.lines
    }
}

/// A poem's repetitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoemRepetition {
    poem: Option<crate::seed::Seed>,
    stanzas: Vec<StanzaRepetition>,
    figures: Vec<Figure>,
    voices: usize,
}

impl PoemRepetition {
    /// The poem's stanzas.
    #[must_use]
    pub fn stanzas(&self) -> &[StanzaRepetition] {
        &self.stanzas
    }

    /// Which poem this describes; see [`Poem::fingerprint`](crate::parse::Poem::fingerprint).
    #[must_use]
    pub const fn poem(&self) -> Option<crate::seed::Seed> {
        self.poem
    }

    /// Every repeated figure found: by kind, then longest first, then by first occurrence.
    #[must_use]
    pub fn figures(&self) -> &[Figure] {
        &self.figures
    }

    /// How many distinct voices the poem needs.
    #[must_use]
    pub const fn voice_count(&self) -> usize {
        self.voices
    }

    /// The figures of one kind.
    pub fn figures_of(&self, kind: FigureKind) -> impl Iterator<Item = &Figure> + '_ {
        self.figures.iter().filter(move |f| f.kind == kind)
    }
}

/// A line reduced to the words it is made of, for comparison.
struct Words {
    line: Span,
    /// Lowercased word text, punctuation and caesurae dropped.
    text: Vec<String>,
    /// The whole line, lowercased and trimmed — the identity of a line with no words at all.
    raw: String,
    /// Where each word sits, so a figure can point at the real text.
    spans: Vec<Span>,
}

impl Words {
    fn of(line: &Line, source: &str) -> Self {
        let mut words = Vec::new();
        let mut spans = Vec::new();
        for token in line.words() {
            if let Some(text) = token.span().text(source) {
                words.push(text.to_lowercase());
                spans.push(token.span());
            }
        }
        Self {
            line: line.span(),
            text: words,
            raw: line
                .span()
                .text(source)
                .unwrap_or_default()
                .trim()
                .to_lowercase(),
            spans,
        }
    }

    /// The spans of words `range`, or `None` if the range is empty or out of bounds.
    fn occurrence(&self, range: core::ops::Range<usize>) -> Option<Occurrence> {
        let words = self.spans.get(range)?;
        (!words.is_empty()).then(|| Occurrence {
            words: words.to_vec(),
        })
    }

    /// The whole line as one occurrence — its words, or the line itself if it has none.
    fn whole(&self) -> Occurrence {
        if self.spans.is_empty() {
            Occurrence {
                words: vec![self.line],
            }
        } else {
            Occurrence {
                words: self.spans.clone(),
            }
        }
    }
}

/// Finds a poem's repetitions.
///
/// Pure text: refrains, anaphora and epistrophe are all facts about words, so no phonemiser is
/// needed. The phonetic figures of `SPEC.md` §4.3 come with M4b.
#[must_use]
pub fn analyse(poem: &Poem) -> PoemRepetition {
    let lines: Vec<Words> = poem
        .stanzas()
        .iter()
        .flat_map(crate::parse::Stanza::lines)
        .map(|line| Words::of(line, poem.source()))
        .collect();

    let (voices, refrains) = assign_voices(&lines);
    let mut figures = refrains;
    figures.extend(edge_figures(&lines, FigureKind::Anaphora));
    figures.extend(edge_figures(&lines, FigureKind::Epistrophe));

    // A total order before anything downstream reads them, per `CLAUDE.md` §4. Longest figure
    // first within a kind, since a longer shared run is the stronger signal.
    figures.sort_by(|a, b| {
        a.kind.cmp(&b.kind).then(b.words().cmp(&a.words())).then(
            a.occurrences
                .first()
                .and_then(Occurrence::hull)
                .cmp(&b.occurrences.first().and_then(Occurrence::hull)),
        )
    });

    let voice_count = voices.iter().map(|v| v.get()).max().map_or(0, |m| m + 1);
    let mut assigned = voices.into_iter();
    let stanzas = poem
        .stanzas()
        .iter()
        .map(|stanza| StanzaRepetition {
            span: stanza.span(),
            lines: stanza
                .lines()
                .iter()
                .map(|line| LineRepetition {
                    line: line.span(),
                    voice: assigned.next().unwrap_or(VoiceId(0)),
                })
                .collect(),
        })
        .collect();

    PoemRepetition {
        poem: poem.fingerprint(),
        stanzas,
        figures,
        voices: voice_count,
    }
}

/// Gives each line a voice, and reports the lines that repeat verbatim.
///
/// Lines with the same words share a voice — `SPEC.md` §4.3's refrain row, where a repeated line
/// is a literal loop. Case and punctuation are ignored: "The tide came in." and "the tide came in"
/// are the same refrain to a reader, and the loop they become is the same loop.
fn assign_voices(lines: &[Words]) -> (Vec<VoiceId>, Vec<Figure>) {
    let mut seen: BTreeMap<Vec<String>, (VoiceId, Vec<Occurrence>)> = BTreeMap::new();
    let mut next_voice = 0_usize;
    let mut voices = Vec::with_capacity(lines.len());

    for line in lines {
        // A line with no words still has an identity — two lines of "..." are the same line, and
        // §4.3 defines a refrain on whole-line repetition without saying the words must be
        // pronounceable. Keying on the raw text keeps them together and keeps a "..." line apart
        // from a "," line.
        let key = if line.text.is_empty() {
            vec![line.raw.clone()]
        } else {
            line.text.clone()
        };

        if let Some((voice, occurrences)) = seen.get_mut(&key) {
            voices.push(*voice);
            occurrences.push(line.whole());
        } else {
            let voice = VoiceId(next_voice);
            next_voice += 1;
            seen.insert(key, (voice, vec![line.whole()]));
            voices.push(voice);
        }
    }

    let refrains = seen
        .into_values()
        .filter(|(_, occurrences)| occurrences.len() > 1)
        .map(|(_, occurrences)| Figure {
            kind: FigureKind::Refrain,
            occurrences,
        })
        .collect();
    (voices, refrains)
}

/// Finds shared line openings or endings.
///
/// Reports the *longest* shared n-gram for each group of lines, not every sub-gram: three lines
/// opening "and the tide" also share "and the" and "and", and reporting all three would triple a
/// single ostinato.
///
/// Refrains are excluded — a group of identical lines is a literal loop, which §4.3 gives a
/// different musical result, and reporting it again as a shared opening would double it.
///
/// Nested groups over *different sets of lines* are both reported: three lines sharing "and the
/// tide" and a fourth sharing only "and the" are two real figures, and a reader hears both. What
/// is suppressed is the *same* set of lines reported again at every shorter n, which would turn
/// one ostinato into several.
///
/// # A single shared word
///
/// One word is usually coincidence — English lines begin with "the" constantly — but sometimes it
/// is the whole figure: Whitman's "I", Ginsberg's "who". Measured on real text, what separates
/// them is density rather than length: Ginsberg repeats "who" on every line of a passage, Whitman
/// "I" on three of five, while Shakespeare's "And" opens 3 of 14 lines of a sonnet and nobody
/// calls that an ostinato. So a one-word figure needs at least three occurrences covering at
/// least half the lines it spans.
///
/// A sestina's rotating end-words stay invisible to this: one occurrence per stanza is far too
/// sparse. That form needs its own detector, recorded in `PLAN.md`.
fn edge_figures(lines: &[Words], kind: FigureKind) -> Vec<Figure> {
    /// Below this, a shared edge needs to prove itself by density.
    const FREELY_ACCEPTED_WORDS: usize = 2;
    /// A one-word figure needs at least this many occurrences...
    const MIN_SPARSE_OCCURRENCES: usize = 3;

    let longest = lines.iter().map(|l| l.text.len()).max().unwrap_or(0);
    let mut figures: Vec<Figure> = Vec::new();
    // Line-sets already reported at a longer n, so the same set is not reported again.
    let mut covered: Vec<Vec<Span>> = Vec::new();

    for n in (1..=longest).rev() {
        let mut groups: BTreeMap<Vec<String>, Vec<(usize, Occurrence)>> = BTreeMap::new();
        for (index, line) in lines.iter().enumerate() {
            // A shared *whole* line is a refrain, not an edge figure.
            if line.text.len() <= n {
                continue;
            }
            let (range, key) = match kind {
                FigureKind::Anaphora => (0..n, line.text.get(..n)),
                FigureKind::Epistrophe => {
                    let start = line.text.len() - n;
                    (start..line.text.len(), line.text.get(start..))
                }
                FigureKind::Refrain => continue,
            };
            let (Some(key), Some(occurrence)) = (key, line.occurrence(range)) else {
                continue;
            };
            groups
                .entry(key.to_vec())
                .or_default()
                .push((index, occurrence));
        }

        for members in groups.into_values() {
            if members.len() < 2 {
                continue;
            }
            // A group of identical lines is a refrain, already reported as one.
            let texts: Vec<&Vec<String>> = members
                .iter()
                .filter_map(|(index, _)| lines.get(*index).map(|l| &l.text))
                .collect();
            if texts.windows(2).all(|pair| pair.first() == pair.last()) {
                continue;
            }
            if n < FREELY_ACCEPTED_WORDS && !is_dense_enough(&members, MIN_SPARSE_OCCURRENCES) {
                continue;
            }

            let lines_here: Vec<Span> = members
                .iter()
                .filter_map(|(index, _)| lines.get(*index).map(|l| l.line))
                .collect();
            if covered
                .iter()
                .any(|seen| lines_here.iter().all(|line| seen.contains(line)))
            {
                continue;
            }
            covered.push(lines_here);
            figures.push(Figure {
                kind,
                occurrences: members.into_iter().map(|(_, o)| o).collect(),
            });
        }
    }
    figures
}

/// Whether a short figure repeats often enough, and closely enough, to be structural.
///
/// Density is measured over the run the figure spans, not the whole poem, so a sustained passage
/// inside a long work still counts.
fn is_dense_enough(members: &[(usize, Occurrence)], minimum: usize) -> bool {
    if members.len() < minimum {
        return false;
    }
    let (Some((first, _)), Some((last, _))) = (members.first(), members.last()) else {
        return false;
    };
    let span = last.saturating_sub(*first) + 1;
    members.len() * 2 >= span
}

#[cfg(test)]
mod tests {
    use super::{FigureKind, PoemRepetition, analyse};
    use crate::parse::Poem;

    fn repetition(source: &str) -> PoemRepetition {
        analyse(&Poem::parse(source).unwrap())
    }

    fn voices(rep: &PoemRepetition) -> Vec<usize> {
        rep.stanzas()
            .iter()
            .flat_map(super::StanzaRepetition::lines)
            .map(|l| l.voice().get())
            .collect()
    }

    fn figure_texts<'s>(
        rep: &PoemRepetition,
        kind: FigureKind,
        source: &'s str,
    ) -> Vec<Vec<&'s str>> {
        rep.figures_of(kind)
            .map(|figure| {
                figure
                    .occurrences()
                    .iter()
                    .filter_map(|o| o.hull().and_then(|span| span.text(source)))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn a_single_shared_word_is_not_a_figure() {
        // Two lines both opening "the" is coincidence, not an ostinato.
        let rep = repetition("the tide came in\nthe fire that burns\n");
        assert!(rep.figures().is_empty(), "{:#?}", rep.figures());
    }

    #[test]
    fn distinct_lines_get_distinct_voices() {
        let rep = repetition("the tide came in\na fire that burns\n");
        assert_eq!(voices(&rep), [0, 1]);
        assert_eq!(rep.voice_count(), 2);
        assert!(rep.figures().is_empty());
    }

    #[test]
    fn a_repeated_line_shares_a_voice() {
        // SPEC.md §4.3's refrain row: a repeated line is a literal loop of that voice.
        let rep = repetition("the tide came in\nthe fire burns\nthe tide came in\n");
        assert_eq!(voices(&rep), [0, 1, 0]);
        assert_eq!(rep.voice_count(), 2);
    }

    #[test]
    fn a_refrain_crosses_stanza_breaks() {
        let source = "the tide came in\nthe fire burns\n\nthe dark\nthe tide came in\n";
        let rep = repetition(source);
        assert_eq!(voices(&rep), [0, 1, 2, 0]);
        let refrains = figure_texts(&rep, FigureKind::Refrain, source);
        assert_eq!(refrains.len(), 1);
        assert_eq!(refrains[0], ["the tide came in", "the tide came in"]);
    }

    #[test]
    fn a_refrain_ignores_case_and_punctuation() {
        // The same refrain to a reader, and the same loop musically.
        let rep = repetition("The tide came in.\nthe fire burns\nthe tide came in\n");
        assert_eq!(voices(&rep), [0, 1, 0]);
        assert_eq!(rep.figures_of(FigureKind::Refrain).count(), 1);
    }

    #[test]
    fn anaphora_is_a_shared_opening() {
        let source = "and the tide came in\nand the tide went out\nthe fire burns\n";
        let rep = repetition(source);
        let figures = figure_texts(&rep, FigureKind::Anaphora, source);
        assert_eq!(figures.len(), 1);
        assert_eq!(figures[0], ["and the tide", "and the tide"]);
    }

    #[test]
    fn epistrophe_is_a_shared_ending() {
        let source = "the fire came in\nthe dark tide came in\nthe silence\n";
        let rep = repetition(source);
        let figures = figure_texts(&rep, FigureKind::Epistrophe, source);
        assert_eq!(figures.len(), 1);
        assert_eq!(figures[0], ["came in", "came in"]);
    }

    #[test]
    fn only_the_longest_shared_opening_is_reported() {
        // Three lines opening "and the tide" also share "and the" and "and". Reporting all three
        // would triple a single ostinato.
        let source = "and the tide came in\nand the tide went out\nand the tide turned\n";
        let rep = repetition(source);
        let figures: Vec<_> = rep.figures_of(FigureKind::Anaphora).collect();
        assert_eq!(figures.len(), 1, "{figures:#?}");
        assert_eq!(figures[0].words(), 3);
        assert_eq!(figures[0].occurrences().len(), 3);
    }

    #[test]
    fn nested_groups_are_both_reported() {
        // Two lines share three words; all three share two. Both are real figures over different
        // sets of lines, and the longer one is reported first.
        let source = "and the tide came in\nand the tide went out\nand the fire burns\n";
        let rep = repetition(source);
        let figures: Vec<_> = rep.figures_of(FigureKind::Anaphora).collect();
        assert_eq!(figures.len(), 2, "{figures:#?}");
        assert_eq!(figures[0].words(), 3);
        assert_eq!(figures[0].occurrences().len(), 2);
        assert_eq!(figures[1].words(), 2);
        assert_eq!(figures[1].occurrences().len(), 3);
    }

    #[test]
    fn a_refrain_is_not_also_anaphora() {
        // The table gives them different musical results: a loop, not a downbeat-anchored figure.
        let rep = repetition("the tide came in\nthe tide came in\n");
        assert_eq!(rep.figures_of(FigureKind::Refrain).count(), 1);
        assert_eq!(rep.figures_of(FigureKind::Anaphora).count(), 0);
        assert_eq!(rep.figures_of(FigureKind::Epistrophe).count(), 0);
    }

    #[test]
    fn a_figure_points_at_the_repeated_words_not_the_whole_line() {
        let source = "and the tide came in\nand the tide went out\n";
        let rep = repetition(source);
        let figure = rep.figures_of(FigureKind::Anaphora).next().unwrap();
        for occurrence in figure.occurrences() {
            assert_eq!(
                occurrence.hull().and_then(|s| s.text(source)),
                Some("and the tide")
            );
            assert_eq!(occurrence.words().len(), 3);
        }
        // ...and each occurrence sits inside its own line.
        for (occurrence, line) in figure.occurrences().iter().zip(rep.stanzas()[0].lines()) {
            assert!(line.line().contains(occurrence.hull().unwrap()));
        }
    }

    #[test]
    fn a_villanelle_reuses_two_voices_for_its_refrains() {
        let source = "do not go gentle into that good night\n\
                      old age should burn and rave at close of day\n\
                      rage rage against the dying of the light\n\
                      \n\
                      though wise men at their end know dark is right\n\
                      because their words had forked no lightning they\n\
                      do not go gentle into that good night\n";
        let rep = repetition(source);
        let seen = voices(&rep);
        assert_eq!(seen[0], seen[5], "the refrain returns to its voice");
        assert_eq!(rep.figures_of(FigureKind::Refrain).count(), 1);
    }

    #[test]
    fn free_verse_repeats_nothing() {
        let rep = repetition("rain\nit came down all night\nthen nothing\n");
        assert_eq!(voices(&rep), [0, 1, 2]);
        assert!(rep.figures().is_empty());
    }

    #[test]
    fn a_line_with_no_words_still_has_an_identity() {
        // A line of punctuation has no words, but it is still a line, and two of them are still
        // the same line. An earlier version gave each its own voice and could never see the
        // repetition; the test that was meant to cover this used a blank line, which the grammar
        // reads as a stanza break, so the branch had no coverage at all.
        let rep = repetition("the tide came in\n...\nthe fire burns\n...\n");
        let seen = voices(&rep);
        assert_eq!(seen.len(), 4);
        assert_eq!(seen[1], seen[3], "the two punctuation lines are one voice");
        assert_ne!(seen[0], seen[1]);
        assert_eq!(rep.voice_count(), 3);

        // ...and different punctuation is a different line.
        let mixed = repetition("...\n,\n");
        assert_eq!(mixed.voice_count(), 2);
    }

    #[test]
    fn sustained_single_word_anaphora_is_a_figure() {
        // Whitman: three of five lines open on "I". Requiring two words made this invisible,
        // along with Ginsberg's "who" catalogue — the textbook cases for the term.
        let source = "i celebrate myself and sing myself\n\
                      and what i assume you shall assume\n\
                      i loafe and invite my soul\n\
                      i lean and loafe at my ease\n";
        let rep = repetition(source);
        let anaphora: Vec<_> = rep.figures_of(FigureKind::Anaphora).collect();
        assert!(
            anaphora
                .iter()
                .any(|f| f.words() == 1 && f.occurrences().len() == 3),
            "{anaphora:#?}"
        );
    }

    #[test]
    fn a_scattered_single_word_opening_is_not_a_figure() {
        // Shakespeare opens 3 of 14 sonnet lines with "And". Too sparse to be an ostinato.
        let source = "rough winds do shake the darling buds of may\n\
                      and summers lease hath all too short a date\n\
                      sometime too hot the eye of heaven shines\n\
                      and often is his gold complexion dimmed\n\
                      by chance or natures changing course untrimmed\n";
        let rep = repetition(source);
        assert!(
            rep.figures_of(FigureKind::Anaphora).all(|f| f.words() > 1),
            "{:#?}",
            rep.figures().to_vec()
        );
    }

    #[test]
    fn an_occurrence_names_words_not_the_text_between_them() {
        // A hull would claim "and, the tide" here, which is not what the other line repeats.
        let source = "and, the tide came in\nand the tide went out\n";
        let rep = repetition(source);
        let figure = rep.figures_of(FigureKind::Anaphora).next().unwrap();
        for occurrence in figure.occurrences() {
            let words: Vec<&str> = occurrence
                .words()
                .iter()
                .filter_map(|s| s.text(source))
                .collect();
            assert_eq!(words, ["and", "the", "tide"]);
        }
    }

    #[test]
    fn analysis_is_deterministic() {
        let source = "and the tide came in\nand the tide went out\n\nand the tide came in\n";
        assert_eq!(repetition(source), repetition(source));
    }
}
