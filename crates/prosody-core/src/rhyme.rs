//! Rhyme: how closely two line endings agree, and how a poem's lines partition into classes.
//!
//! `SPEC.md` §4.5 sets the mapping this feeds. A rhyme class becomes a scale degree, assigned in
//! order of first appearance, so ABAB alternates and AABB pairs. The part that earns it is the
//! next sentence of the spec: **perfect rhyme becomes exact pitch, slant rhyme becomes detune or
//! a neighbour tone** — which needs rhyme scored continuously, not as a boolean.
//!
//! # Classes are a partition, not a transitive closure
//!
//! The spec says to treat rhyme as an equivalence relation. "Scores above a threshold" is not one:
//! it is reflexive and symmetric but not transitive, and a union-find over it chains — *fade*
//! rhymes with *shade*, *shade* with *shed*, and *fade* ends up sharing a pitch with *shed*.
//!
//! So each line joins the best-scoring earlier class instead, or starts its own. That is still a
//! partition, hence still an equivalence relation, and it never chains. The cost is that class
//! membership depends on line order — deliberately, since order of first appearance is what §4.5
//! assigns scale degrees by.

use std::collections::BTreeMap;

use crate::parse::Poem;
use crate::phoneme::{Phoneme, Phonemiser, Pronunciation, Provenance};
use crate::span::Span;

/// How closely two line endings rhyme, as an exact rational in `0..=1`.
///
/// Exact for the same reason [`Variance`](crate::meter::Variance) is: this score decides which
/// lines share a class, a class decides which lines share a pitch, and `CLAUDE.md` §4 keeps
/// floats out of decisions like that.
#[derive(Debug, Clone, Copy)]
/// Widths are `u64`, not `u32`. The cross-multiplication below wraps at `u32`, which gave the
/// wrong ordering in release builds and panicked in debug ones — the same overflow lesson
/// `Variance` already learned, applied there and missed here.
pub struct RhymeScore {
    matched: u64,
    total: u64,
}

impl RhymeScore {
    /// No agreement at all.
    pub const NONE: Self = Self {
        matched: 0,
        total: 1,
    };

    /// Identical endings.
    pub const PERFECT: Self = Self {
        matched: 1,
        total: 1,
    };

    /// Whether the two endings agree completely.
    #[must_use]
    pub const fn is_perfect(self) -> bool {
        self.matched == self.total
    }

    /// The value, for display only. Never compare two of these — use [`Ord`], which is exact.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // display only; ordering uses the exact rational
    pub fn as_f64(self) -> f64 {
        self.matched as f64 / self.total as f64
    }
}

impl PartialEq for RhymeScore {
    fn eq(&self, other: &Self) -> bool {
        self.matched * other.total == other.matched * self.total
    }
}

impl Eq for RhymeScore {}

impl Ord for RhymeScore {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.matched * other.total).cmp(&(other.matched * self.total))
    }
}

impl PartialOrd for RhymeScore {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// How much agreement counts as a rhyme.
///
/// Half the ending, counting back from the last phoneme. This is *analysis*, not palette: it
/// decides which lines share a class, and therefore the poem's structure. Moving it into the
/// style header would let a style change the arrangement, which invariant I3 forbids.
///
/// The value is a tuning question `SPEC.md` §9 anticipates. It is recorded in `PLAN.md` rather
/// than left as a number nobody remembers choosing.
pub const RHYME_THRESHOLD: RhymeScore = RhymeScore {
    matched: 1,
    total: 2,
};

/// Which rhyme class a line belongs to.
///
/// Numbered by first appearance, which is the order `SPEC.md` §4.5 assigns scale degrees in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RhymeClassId(usize);

impl RhymeClassId {
    /// The class number, counting from zero.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// The conventional scheme letter: `A` for the first class, `B` for the second.
    ///
    /// Wraps past `Z`, which no real stanza reaches.
    #[must_use]
    pub fn letter(self) -> char {
        let index = u32::try_from(self.0 % 26).unwrap_or(0);
        char::from_u32('A' as u32 + index).unwrap_or('?')
    }
}

/// Scores how closely two endings rhyme.
///
/// English rhyme is reckoned from the last stressed vowel, which is what
/// [`Pronunciation::rhyme_tail`] returns, and it aligns at the *end* of the word. So the score is
/// the longest common suffix of the two tails over the longer tail: identical endings give 1,
/// and a shared coda with a different vowel gives less.
///
/// "Stressed" there means primary *or* secondary — unlike metre, which counts primary only.
/// "Cavalcade" rhymes with "brigade" on a syllable carrying only secondary stress.
/// # A known asymmetry
///
/// Because it is a *suffix* match, a difference at the very end of the tail costs everything
/// earlier. Consonance — "worth" and "breath", sharing a coda but not a vowel — scores 0.5, while
/// assonance — "lake" and "fate", sharing the stressed vowel but not the coda — scores 0. Most
/// prosody treats the vowel as rhyme's primary ingredient, so this ranks them the other way
/// round. It is left as it stands because `SPEC.md` §4.3 gives assonance its own pass, and
/// because a threshold sweep over a real corpus puts this metric near its optimum. Recorded in
/// `PLAN.md` as a question for M4 rather than as a silent choice.
#[must_use]
pub fn score(left: &Pronunciation, right: &Pronunciation) -> RhymeScore {
    score_tails(&left.rhyme_tail(), &right.rhyme_tail())
}

/// One line's rhyme.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineRhyme {
    line: Span,
    word: Option<Span>,
    tail: Vec<Phoneme>,
    class: Option<RhymeClassId>,
    provenance: Option<Provenance>,
    score: Option<RhymeScore>,
}

impl LineRhyme {
    /// The line's extent.
    #[must_use]
    pub const fn line(&self) -> Span {
        self.line
    }

    /// The word the rhyme rests on — the last word of the line.
    ///
    /// `None` when the line has no pronounceable word, in which case it has no class either.
    #[must_use]
    pub const fn word(&self) -> Option<Span> {
        self.word
    }

    /// The phonemes rhyme is scored on, from the last stressed vowel onward.
    #[must_use]
    pub fn tail(&self) -> &[Phoneme] {
        &self.tail
    }

    /// Which class this line belongs to, if it has one.
    #[must_use]
    pub const fn class(&self) -> Option<RhymeClassId> {
        self.class
    }

    /// How closely this line rhymes with the best of its class-mates.
    ///
    /// `None` when the line rhymes with nothing — a class of one. An earlier version reported
    /// [`RhymeScore::PERFECT`] there, which made "agrees with itself" indistinguishable from
    /// "genuinely matches another line".
    ///
    /// Measured against the *whole* class, not the line that founded it, and computed after
    /// every line has been placed. Scoring against the founder made this number depend on
    /// reading order: "fade" and "shade" rhyme perfectly, but with "shed" read first the pair
    /// reported 0.5 — and `SPEC.md` §4.5 turns this number into pitch precision, so exact pitch
    /// and detune would have swapped on an unrelated third line.
    #[must_use]
    pub const fn score(&self) -> Option<RhymeScore> {
        self.score
    }

    /// Whether the pronunciation behind this rhyme was guessed.
    ///
    /// A rhyme resting on `Provenance::LetterToSound` is only as good as a ruleset that gets a
    /// quarter of whole words exactly right, and whatever assigns pitch should know that.
    #[must_use]
    pub fn rests_on_a_guess(&self) -> bool {
        self.provenance.is_some_and(Provenance::is_guess)
    }
}

/// A stanza's rhymes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanzaRhyme {
    span: Span,
    lines: Vec<LineRhyme>,
}

impl StanzaRhyme {
    /// The stanza's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The stanza's lines, in order.
    #[must_use]
    pub fn lines(&self) -> &[LineRhyme] {
        &self.lines
    }

    /// The rhyme scheme, as conventional letters — `"ABAB"`, `"AABB"`.
    ///
    /// Letters are relative to this stanza, so the first class to appear in it is `A` whatever its
    /// poem-wide number. A line with no class contributes `-`.
    #[must_use]
    pub fn scheme(&self) -> String {
        let mut seen: BTreeMap<RhymeClassId, char> = BTreeMap::new();
        let mut next = 0_usize;
        let mut scheme = String::with_capacity(self.lines.len());
        for line in &self.lines {
            match line.class {
                None => scheme.push('-'),
                Some(class) => {
                    let letter = *seen.entry(class).or_insert_with(|| {
                        let letter = RhymeClassId(next).letter();
                        next += 1;
                        letter
                    });
                    scheme.push(letter);
                }
            }
        }
        scheme
    }
}

/// A poem's rhymes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoemRhyme {
    stanzas: Vec<StanzaRhyme>,
    classes: usize,
}

impl PoemRhyme {
    /// The poem's stanzas.
    #[must_use]
    pub fn stanzas(&self) -> &[StanzaRhyme] {
        &self.stanzas
    }

    /// How many distinct rhyme classes the poem has.
    ///
    /// `SPEC.md` §4.5 draws a scale degree per class, so this is how many pitches the poem needs.
    #[must_use]
    pub const fn class_count(&self) -> usize {
        self.classes
    }
}

/// The ending a line offers for rhyming.
struct Ending {
    word: Span,
    pronunciation: Pronunciation,
}

/// Finds a poem's rhymes.
///
/// Classes are poem-wide, not per-stanza: a refrain returning three stanzas later is the same
/// rhyme, and a villanelle's repeated lines have to land together for §4.5's "two-note obsession"
/// to come out.
#[must_use]
pub fn analyse(poem: &Poem, phonemiser: &impl Phonemiser) -> PoemRhyme {
    // Pass one: place every line, in reading order, so classes number by first appearance.
    let mut classes: Vec<Vec<Vec<Phoneme>>> = Vec::new();
    let mut stanzas: Vec<StanzaRhyme> = Vec::new();

    for stanza in poem.stanzas() {
        let mut lines = Vec::new();
        for line in stanza.lines() {
            let Some(ending) = ending_of(line, poem, phonemiser, &classes) else {
                lines.push(LineRhyme {
                    line: line.span(),
                    word: None,
                    tail: Vec::new(),
                    class: None,
                    provenance: None,
                    score: None,
                });
                continue;
            };
            let tail = ending.pronunciation.rhyme_tail();
            let class = classify_and_register(&tail, &mut classes);
            lines.push(LineRhyme {
                line: line.span(),
                word: Some(ending.word),
                tail,
                class: Some(class),
                provenance: Some(ending.pronunciation.provenance()),
                score: None,
            });
        }
        stanzas.push(StanzaRhyme {
            span: stanza.span(),
            lines,
        });
    }

    // Pass two: score each line against the *other* members of the class it ended up in. Doing
    // this after every line is placed is what makes the number independent of reading order —
    // and `SPEC.md` §4.5 turns it straight into pitch precision, so it had better be.
    for stanza in &mut stanzas {
        for line in &mut stanza.lines {
            let Some(class) = line.class else { continue };
            let Some(members) = classes.get(class.get()) else {
                continue;
            };
            line.score = members
                .iter()
                .filter(|member| member.as_slice() != line.tail.as_slice())
                .map(|member| score_tails(&line.tail, member))
                .max()
                .or_else(|| {
                    // Identical tails are the same rhyme, so a class of repeats is a perfect
                    // rhyme with itself — a refrain, not a singleton.
                    (members.len() > 1).then_some(RhymeScore::PERFECT)
                });
        }
    }

    PoemRhyme {
        stanzas,
        classes: classes.len(),
    }
}

/// The last pronounceable word of a line, and the pronunciation of it that rhymes best.
///
/// Where a source offers alternates, this is the one place with enough context to choose: the
/// dictionary lists `wind` as the verb first, but a line ending in `wind` after one ending in
/// `sinned` means the breeze. The choice is deterministic — best score, then source order — so
/// invariant I1 holds even though the answer depends on the lines before it.
fn ending_of(
    line: &crate::parse::Line,
    poem: &Poem,
    phonemiser: &impl Phonemiser,
    classes: &[Vec<Vec<Phoneme>>],
) -> Option<Ending> {
    for token in line.words().collect::<Vec<_>>().into_iter().rev() {
        let span = token.span();
        let Some(text) = span.text(poem.source()) else {
            continue;
        };
        let variants = phonemiser.variants(text);
        let mut best: Option<(RhymeScore, Pronunciation)> = None;
        for variant in variants {
            let tail = variant.rhyme_tail();
            let score = best_match(&tail, classes).map_or(RhymeScore::NONE, |(_, s)| s);
            if best.as_ref().is_none_or(|(seen, _)| score > *seen) {
                best = Some((score, variant));
            }
        }
        if let Some((_, pronunciation)) = best {
            return Some(Ending {
                word: span,
                pronunciation,
            });
        }
    }
    None
}

/// The best a tail scores against any line already placed, and which class that line is in.
///
/// `None` when nothing has been placed yet, or when nothing scores above zero. Deliberately says
/// nothing about thresholds or class founding: variant choice needs the raw quality of the match,
/// and an earlier version conflated "founds its own class" with "matches perfectly", so a
/// heteronym's wrong sense won whenever the right sense's rival happened to match nothing.
fn best_match(tail: &[Phoneme], classes: &[Vec<Vec<Phoneme>>]) -> Option<(usize, RhymeScore)> {
    let mut best: Option<(usize, RhymeScore)> = None;
    for (index, members) in classes.iter().enumerate() {
        for member in members {
            let score = score_tails(tail, member);
            if best.is_none_or(|(_, seen)| score > seen) {
                best = Some((index, score));
            }
        }
    }
    best
}

/// Places a tail in a class, founding one if nothing established rhymes with it.
///
/// Greedy: the best-scoring existing class above the threshold wins, ties going to the earliest —
/// which is the order `SPEC.md` §4.5 assigns scale degrees in. A class is compared against *all*
/// its members rather than only its founder, so joining does not depend on which line happened
/// to open the class.
fn classify(tail: &[Phoneme], classes: &[Vec<Vec<Phoneme>>]) -> RhymeClassId {
    match best_match(tail, classes) {
        Some((index, score)) if score >= RHYME_THRESHOLD => RhymeClassId(index),
        _ => RhymeClassId(classes.len()),
    }
}

/// Like [`score`], but over already-extracted tails.
fn score_tails(a: &[Phoneme], b: &[Phoneme]) -> RhymeScore {
    let longest = a.len().max(b.len());
    if longest == 0 {
        return RhymeScore::NONE;
    }
    let matched = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    RhymeScore {
        matched: matched as u64,
        total: longest as u64,
    }
}

/// Places a tail in a class and records it as a member.
fn classify_and_register(tail: &[Phoneme], classes: &mut Vec<Vec<Vec<Phoneme>>>) -> RhymeClassId {
    let class = classify(tail, classes);
    if class.get() == classes.len() {
        classes.push(Vec::new());
    }
    if let Some(members) = classes.get_mut(class.get()) {
        members.push(tail.to_vec());
    }
    class
}

#[cfg(test)]
mod tests {
    use super::{RHYME_THRESHOLD, RhymeScore, analyse, score};
    use crate::parse::Poem;
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Provenance, Stress, Unpronounceable, syllabify,
    };

    const W: Stress = Stress::Unstressed;
    const S: Stress = Stress::Primary;

    /// A table-driven phonemiser, so rhyme tests do not depend on a dictionary.
    struct Fake;

    /// One pronunciation, as symbol-and-stress pairs.
    type Phones = &'static [(&'static str, Stress)];
    /// A word and every pronunciation of it, in source order.
    type Entry = (&'static str, &'static [Phones]);

    /// Several words have alternates, to exercise variant choice.
    static WORDS: &[Entry] = &[
        (
            "burning",
            &[&[("B", W), ("ER", S), ("N", W), ("IH", W), ("NG", W)]],
        ),
        (
            "turning",
            &[&[("T", W), ("ER", S), ("N", W), ("IH", W), ("NG", W)]],
        ),
        ("burn", &[&[("B", W), ("ER", S), ("N", W)]]),
        ("turn", &[&[("T", W), ("ER", S), ("N", W)]]),
        ("love", &[&[("L", W), ("AH", S), ("V", W)]]),
        ("move", &[&[("M", W), ("UW", S), ("V", W)]]),
        ("tide", &[&[("T", W), ("AY", S), ("D", W)]]),
        ("side", &[&[("S", W), ("AY", S), ("D", W)]]),
        ("night", &[&[("N", W), ("AY", S), ("T", W)]]),
        ("light", &[&[("L", W), ("AY", S), ("T", W)]]),
        ("sinned", &[&[("S", W), ("IH", S), ("N", W), ("D", W)]]),
        ("dark", &[&[("D", W), ("AA", S), ("R", W), ("K", W)]]),
        ("the", &[&[("DH", W), ("AH", W)]]),
        ("and", &[&[("AE", W), ("N", W), ("D", W)]]),
        // Two senses: the verb (rhymes with "mind") listed first, the breeze second — the shape
        // CMUdict actually has.
        (
            "wind",
            &[
                &[("W", W), ("AY", S), ("N", W), ("D", W)],
                &[("W", W), ("IH", S), ("N", W), ("D", W)],
            ],
        ),
        ("mind", &[&[("M", W), ("AY", S), ("N", W), ("D", W)]]),
        ("crow", &[&[("K", W), ("R", W), ("OW", S)]]),
        // The bend first, the weapon second, as CMUdict lists them.
        ("bow", &[&[("B", W), ("AW", S)], &[("B", W), ("OW", S)]]),
    ];

    fn build(phones: &[(&str, Stress)], provenance: Provenance) -> Pronunciation {
        let phones: Vec<_> = phones
            .iter()
            .map(|(symbol, stress)| (ARPABET.phoneme(symbol).unwrap(), *stress))
            .collect();
        let syllables = syllabify(&phones).unwrap();
        match provenance {
            Provenance::Dictionary => Pronunciation::from_dictionary(syllables),
            Provenance::LetterToSound => Pronunciation::guessed(syllables),
        }
    }

    impl Phonemiser for Fake {
        fn inventory(&self) -> &'static Inventory {
            &ARPABET
        }

        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            self.variants(word)
                .into_iter()
                .next()
                .ok_or(Unpronounceable::NotKnown)
        }

        fn variants(&self, word: &str) -> Vec<Pronunciation> {
            let lower = word.to_lowercase();
            WORDS
                .iter()
                .find(|(w, _)| *w == lower)
                .map(|(_, all)| {
                    all.iter()
                        .map(|p| build(p, Provenance::Dictionary))
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    fn say(word: &str) -> Pronunciation {
        Fake.pronounce(word).unwrap()
    }

    fn scheme_of(source: &str) -> Vec<String> {
        let poem = Poem::parse(source).unwrap();
        analyse(&poem, &Fake)
            .stanzas()
            .iter()
            .map(super::StanzaRhyme::scheme)
            .collect()
    }

    #[test]
    fn a_heteronym_wins_even_when_its_rival_matches_nothing() {
        // "bow" is B AW (bend) then B OW (weapon) in CMUdict order. After a line ending in a word
        // that rhymes only with the second sense, the second must win — but an earlier version
        // reported "founds its own class" as a perfect score, so the first sense tied and won on
        // order. Only one of these senses matches anything at all, which is the case that broke.
        let poem = Poem::parse("the crow\nthe bow\n").unwrap();
        let rhymes = analyse(&poem, &Fake);
        let lines = rhymes.stanzas()[0].lines();
        assert_eq!(lines[0].class(), lines[1].class(), "crow and bow rhyme");
        let tail: Vec<&str> = lines[1]
            .tail()
            .iter()
            .copied()
            .map(crate::phoneme::Phoneme::symbol)
            .collect();
        assert_eq!(tail, ["OW"], "the weapon, not the bend");
    }

    #[test]
    fn a_perfect_rhyme_scores_one() {
        assert_eq!(score(&say("burning"), &say("turning")), RhymeScore::PERFECT);
        assert_eq!(score(&say("tide"), &say("side")), RhymeScore::PERFECT);
        assert!(score(&say("burn"), &say("turn")).is_perfect());
    }

    #[test]
    fn a_slant_rhyme_scores_between() {
        // "love"/"move" share their coda but not their vowel: one phoneme of two.
        let slant = score(&say("love"), &say("move"));
        assert!(!slant.is_perfect());
        assert_ne!(slant, RhymeScore::NONE);
        assert!(slant < RhymeScore::PERFECT);
        assert!((slant.as_f64() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn unrelated_endings_score_nothing() {
        let none = score(&say("burning"), &say("dark"));
        assert_eq!(none, RhymeScore::NONE);
        assert!(none < RHYME_THRESHOLD);
    }

    #[test]
    fn a_word_rhymes_perfectly_with_itself() {
        assert!(score(&say("tide"), &say("tide")).is_perfect());
    }

    #[test]
    fn scores_compare_exactly() {
        // 1/2 and 2/4 are the same score even though the fractions differ, so equality has to
        // agree with ordering — the bug this project already hit once with Variance.
        let half = score(&say("love"), &say("move"));
        assert_eq!(half.cmp(&RHYME_THRESHOLD), core::cmp::Ordering::Equal);
        assert_eq!(half, RHYME_THRESHOLD);
        assert!(RhymeScore::NONE < RHYME_THRESHOLD);
        assert!(RHYME_THRESHOLD < RhymeScore::PERFECT);
    }

    #[test]
    fn alternating_rhyme_gives_abab() {
        assert_eq!(
            scheme_of("the tide\nthe burn\nthe side\nthe turn\n"),
            ["ABAB"]
        );
    }

    #[test]
    fn paired_rhyme_gives_aabb() {
        assert_eq!(
            scheme_of("the tide\nthe side\nthe burn\nthe turn\n"),
            ["AABB"]
        );
    }

    #[test]
    fn a_stanza_of_one_rhyme_is_all_a() {
        assert_eq!(scheme_of("the tide\nthe side\nthe tide\n"), ["AAA"]);
    }

    #[test]
    fn classes_are_poem_wide_so_a_refrain_returns_to_its_class() {
        // A villanelle's repeated lines have to land together for SPEC.md §4.5's "two-note
        // obsession" to come out at all.
        let source = "the tide\nthe burn\n\nthe dark\nthe side\n";
        let poem = Poem::parse(source).unwrap();
        let rhymes = analyse(&poem, &Fake);
        let class = |s: usize, l: usize| rhymes.stanzas()[s].lines()[l].class().unwrap();
        assert_eq!(class(0, 0), class(1, 1), "tide and side share a class");
        assert_ne!(class(0, 1), class(1, 0));
        assert_eq!(rhymes.class_count(), 3);
    }

    #[test]
    fn scheme_letters_are_relative_to_their_stanza() {
        // The second stanza opens a class that is poem-wide number 2, but reads as its own "A".
        let schemes = scheme_of("the tide\nthe side\n\nthe burn\nthe turn\n");
        assert_eq!(schemes, ["AA", "AA"]);
    }

    #[test]
    fn a_score_is_against_class_mates_and_a_lone_line_has_none() {
        let poem = Poem::parse("the tide\nthe side\n").unwrap();
        let rhymes = analyse(&poem, &Fake);
        let lines = rhymes.stanzas()[0].lines();
        assert!(lines[0].score().is_some_and(RhymeScore::is_perfect));
        assert!(lines[1].score().is_some_and(RhymeScore::is_perfect));

        // A line that rhymes with nothing reports no score, rather than claiming to agree
        // perfectly with itself.
        let alone = Poem::parse("the tide\nthe dark\n").unwrap();
        let rhymes = analyse(&alone, &Fake);
        for line in rhymes.stanzas()[0].lines() {
            assert_eq!(line.score(), None, "{:?}", line.word());
        }
    }

    #[test]
    fn a_score_does_not_depend_on_which_line_founded_the_class() {
        // "tide" and "side" rhyme perfectly; "sinned" only shares their coda. Whichever is read
        // first, the perfect pair must still report a perfect score — SPEC.md §4.5 turns this
        // number into exact pitch versus detune.
        let forwards = Poem::parse("the tide\nthe side\nthe sinned\n").unwrap();
        let backwards = Poem::parse("the sinned\nthe side\nthe tide\n").unwrap();
        let best = |poem: &Poem| {
            analyse(poem, &Fake).stanzas()[0]
                .lines()
                .iter()
                .filter_map(super::LineRhyme::score)
                .max()
        };
        assert_eq!(best(&forwards), Some(RhymeScore::PERFECT));
        assert_eq!(best(&backwards), Some(RhymeScore::PERFECT));
    }

    #[test]
    fn the_rhyme_word_is_the_last_word_of_the_line() {
        let source = "the burning tide\n";
        let poem = Poem::parse(source).unwrap();
        let rhymes = analyse(&poem, &Fake);
        let line = &rhymes.stanzas()[0].lines()[0];
        assert_eq!(line.word().and_then(|s| s.text(source)), Some("tide"));
    }

    #[test]
    fn a_line_with_no_pronounceable_word_has_no_class() {
        let poem = Poem::parse("krapplesnaffle\n").unwrap();
        let rhymes = analyse(&poem, &Fake);
        let line = &rhymes.stanzas()[0].lines()[0];
        assert_eq!(line.class(), None);
        assert_eq!(line.word(), None);
        assert!(line.tail().is_empty());
        assert_eq!(rhymes.stanzas()[0].scheme(), "-");
    }

    #[test]
    fn an_unpronounceable_last_word_falls_back_to_the_one_before() {
        // The line still rhymes on the last word that has a sound, rather than losing its class.
        let source = "the tide krapplesnaffle\nthe side\n";
        let poem = Poem::parse(source).unwrap();
        let rhymes = analyse(&poem, &Fake);
        let lines = rhymes.stanzas()[0].lines();
        assert_eq!(lines[0].word().and_then(|s| s.text(source)), Some("tide"));
        assert_eq!(lines[0].class(), lines[1].class());
    }

    #[test]
    fn a_heteronym_takes_the_sense_that_rhymes() {
        // The table lists "wind" the verb first, as CMUdict does. Both senses clear the
        // threshold against either partner — "wind" the verb still shares `N D` with "sinned" —
        // so the scheme alone proves nothing. What proves it is the *score*: only the right sense
        // rhymes perfectly, and SPEC.md §4.5 turns that difference into exact pitch versus detune.
        let after_sinned = Poem::parse("the sinned\nthe wind\n").unwrap();
        let breeze = analyse(&after_sinned, &Fake);
        let line = &breeze.stanzas()[0].lines()[1];
        assert!(
            line.score().is_some_and(RhymeScore::is_perfect),
            "the breeze was meant: {:?}",
            line.score().map(RhymeScore::as_f64)
        );
        let tail: Vec<&str> = line
            .tail()
            .iter()
            .copied()
            .map(crate::phoneme::Phoneme::symbol)
            .collect();
        assert_eq!(tail, ["IH", "N", "D"], "the breeze, not the verb");

        let after_mind = Poem::parse("the mind\nthe wind\n").unwrap();
        let verb = analyse(&after_mind, &Fake);
        let line = &verb.stanzas()[0].lines()[1];
        assert!(
            line.score().is_some_and(RhymeScore::is_perfect),
            "the verb was meant"
        );
        let tail: Vec<&str> = line
            .tail()
            .iter()
            .copied()
            .map(crate::phoneme::Phoneme::symbol)
            .collect();
        assert_eq!(tail, ["AY", "N", "D"], "the verb, not the breeze");
    }

    #[test]
    fn a_rhyme_says_whether_it_rests_on_a_guess() {
        struct Guesser;
        impl Phonemiser for Guesser {
            fn inventory(&self) -> &'static Inventory {
                &ARPABET
            }
            fn pronounce(&self, _: &str) -> Result<Pronunciation, Unpronounceable> {
                Ok(build(
                    &[("T", W), ("AY", S), ("D", W)],
                    Provenance::LetterToSound,
                ))
            }
        }
        let poem = Poem::parse("anything\n").unwrap();
        let rhymes = analyse(&poem, &Guesser);
        assert!(rhymes.stanzas()[0].lines()[0].rests_on_a_guess());

        let known = Poem::parse("the tide\n").unwrap();
        assert!(!analyse(&known, &Fake).stanzas()[0].lines()[0].rests_on_a_guess());
    }

    #[test]
    fn classes_do_not_chain_through_a_middle_term() {
        // The reason classes are a partition rather than a transitive closure: a union-find over
        // "scores above threshold" would put the first and last lines together despite their
        // endings sharing nothing.
        let source = "the night\nthe light\nthe tide\n";
        let poem = Poem::parse(source).unwrap();
        let rhymes = analyse(&poem, &Fake);
        let lines = rhymes.stanzas()[0].lines();
        assert_eq!(lines[0].class(), lines[1].class(), "night and light rhyme");
        assert_ne!(lines[0].class(), lines[2].class(), "night and tide do not");
    }

    #[test]
    fn analysis_is_deterministic() {
        let source = "the tide\nthe burn\nthe side\nthe turn\n\nthe wind\nthe mind\n";
        let poem = Poem::parse(source).unwrap();
        assert_eq!(analyse(&poem, &Fake), analyse(&poem, &Fake));
    }
}
