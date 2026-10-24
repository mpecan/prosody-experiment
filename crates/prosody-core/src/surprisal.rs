//! Distinctiveness: which words stand out, and how far.
//!
//! `SPEC.md` §4.4 asks for a real metric rather than vibes, and gives one:
//!
//! ```text
//! surprisal(word) = α · lexical_rarity(word)
//!                 + β · phoneme_set_distance(word, local_window)
//! ```
//!
//! It drives transient sharpness, brightness, attack time and dry/wet — so a word that is
//! unusual, or that sounds unlike the words around it, cuts through. The spec is emphatic that
//! this stays "fully offline, small tables, deterministic. No model required", and it does.
//!
//! # The two halves measure different things
//!
//! Rarity is a property of the word by itself: "phosphorescent" is striking wherever it appears.
//! Phoneme-set distance is a property of the word *here*: "burn" among "turn", "urn" and "spurn"
//! is ordinary, and among "glass", "cheek" and "afternoon" it is not. A poem can surprise either
//! way, so both are kept and reported separately as well as combined.
//!
//! # Exact, like everything that decides
//!
//! [`Surprisal`] is a rational, not a float. `CLAUDE.md` §4 keeps floats out of anything that
//! decides, and this number will pick filter cutoffs and attack times.

use std::collections::BTreeSet;

use crate::parse::{Line, Poem};
use crate::phoneme::{Phoneme, Phonemiser, Provenance};
use crate::span::Span;

/// How uncommon a word is, as a band rather than a count.
///
/// Bands, not raw frequencies: the exact count in some corpus is not a fact about the poem, and
/// carrying it would invite float arithmetic into a decision. Band 0 is the commonest words —
/// "the", "and", "of" — and [`Rarity::RAREST`] is everything the lexicon has never seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rarity(u8);

impl Rarity {
    /// The number of bands, from commonest to rarest.
    pub const BANDS: u8 = 9;

    /// The commonest words.
    pub const COMMONEST: Self = Self(0);

    /// A word the lexicon does not know at all — a coinage, a proper noun, a foreign word.
    pub const RAREST: Self = Self(Self::BANDS - 1);

    /// Builds a band, clamping to the available range.
    #[must_use]
    pub const fn band(value: u8) -> Self {
        if value >= Self::BANDS {
            Self::RAREST
        } else {
            Self(value)
        }
    }

    /// The band number, counting from zero.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// Somewhere to ask how common a word is.
///
/// A trait for the same reason [`Phonemiser`] is one: a rarity table is language data, and
/// `SPEC.md` §9 expects a second language eventually. The English table lives in its own crate so
/// nothing that depends on the core inherits it.
pub trait Lexicon {
    /// How uncommon `word` is. Unknown words are [`Rarity::RAREST`].
    fn rarity(&self, word: &str) -> Rarity;

    /// Whether `word` is a *function word* of the language — one of the closed class of articles,
    /// pronouns, possessives, conjunctions, short prepositions and auxiliaries that carry grammar
    /// rather than content.
    ///
    /// Read by the light variant of
    /// [`Mapping::Function`](crate::mapping::Mapping::Function) (`SPEC.md` §4.2), which strikes
    /// a one-syllable function word lightly where the metre does not need its stress — a
    /// pronunciation dictionary gives "or" and "thy" the same primary stress as "hand".
    ///
    /// On the lexicon, not on the phonemiser or in the core, because which words are closed-class
    /// is a fact about one language's grammar, exactly as a frequency table is (`SPEC.md` §9): a
    /// Slovenian lexicon answers it for Slovenian. Required rather than defaulted, so a second
    /// language cannot inherit "no function words" without saying so. Unknown words are not
    /// function words — the class is closed, so anything a list does not name is open-class.
    fn is_function_word(&self, word: &str) -> bool;
}

/// How much a word stands out, as an exact rational in `0..=1`.
#[derive(Debug, Clone, Copy)]
pub struct Surprisal {
    numerator: u64,
    denominator: u64,
}

impl Surprisal {
    /// Nothing unusual at all.
    pub const NONE: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    /// Builds a ratio, clamping above one and treating a zero denominator as [`Surprisal::NONE`].
    #[must_use]
    fn ratio(numerator: u64, denominator: u64) -> Self {
        if denominator == 0 {
            return Self::NONE;
        }
        Self {
            numerator: numerator.min(denominator),
            denominator,
        }
    }

    /// The value, for display only. Never compare two of these — use [`Ord`], which is exact.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // display only; ordering uses the exact rational
    pub fn as_f64(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

impl PartialEq for Surprisal {
    fn eq(&self, other: &Self) -> bool {
        self.numerator * other.denominator == other.numerator * self.denominator
    }
}

impl Eq for Surprisal {}

impl Ord for Surprisal {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.numerator * other.denominator).cmp(&(other.numerator * self.denominator))
    }
}

impl PartialOrd for Surprisal {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// How many unheard sounds count as maximally novel.
///
/// Four is about the length of an ordinary English content word, so a word whose sounds are all
/// new to its window reaches the top of the scale while a short function word contributing one
/// stray vowel does not.
const SALIENT: u64 = 4;

/// How many words either side of a word count as its neighbours.
///
/// `SPEC.md` §4.4 says "local window", and the line is only local when the line is short. Measured
/// on a twenty-seven-word Whitman line, seventeen words scored no novelty at all: with a wide
/// enough window almost every English phoneme is covered by something, and the term stops saying
/// anything. A bounded neighbourhood keeps it discriminating in long-lined and free verse.
const NEIGHBOURS: usize = 4;

/// The weight on lexical rarity, `α` in `SPEC.md` §4.4.
///
/// Equal to `BETA`, deliberately. The spec gives the shape of the formula but no magnitudes, and
/// nothing has been auditioned yet — an unequal split would be a guess dressed as a decision.
/// `SPEC.md` §9 expects these ranges to need tuning once there is something to listen to; that is
/// recorded in `PLAN.md` rather than settled here.
pub const ALPHA: u64 = 1;

/// The weight on phoneme-set distance, `β` in `SPEC.md` §4.4.
pub const BETA: u64 = 1;

/// One word's distinctiveness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSurprisal {
    span: Span,
    rarity: Rarity,
    novelty: Surprisal,
    surprisal: Surprisal,
    provenance: Provenance,
    function: bool,
}

impl WordSurprisal {
    /// Where the word sits in the source.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// How uncommon the word is by itself.
    #[must_use]
    pub const fn rarity(&self) -> Rarity {
        self.rarity
    }

    /// How much of the word's sound is not heard elsewhere in its line.
    ///
    /// The `phoneme_set_distance` half of `SPEC.md` §4.4, reported separately because it says
    /// something different from rarity: this is the word being strange *here*.
    #[must_use]
    pub const fn novelty(&self) -> Surprisal {
        self.novelty
    }

    /// The two halves combined, weighted by [`ALPHA`] and [`BETA`].
    #[must_use]
    pub const fn surprisal(&self) -> Surprisal {
        self.surprisal
    }

    /// Whether the pronunciation behind the sound half was guessed.
    #[must_use]
    pub fn rests_on_a_guess(&self) -> bool {
        self.provenance.is_guess()
    }

    /// Whether the lexicon calls the word a function word ([`Lexicon::is_function_word`]).
    ///
    /// Carried here because this is the pass that asks the lexicon about each word; it plays no
    /// part in the word's surprisal. The lowering reads it for `SPEC.md` §4.2's light function
    /// words (`crate::accent`).
    #[must_use]
    pub const fn is_function_word(&self) -> bool {
        self.function
    }
}

/// A poem's distinctiveness, word by word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoemSurprisal {
    poem: crate::seed::Fingerprint,
    words: Vec<WordSurprisal>,
}

impl PoemSurprisal {
    /// Which poem this describes; see [`Poem::fingerprint`](crate::parse::Poem::fingerprint).
    ///
    /// Spans are bare byte offsets, so two poems with the same line lengths have identical ones.
    /// Any pass combining several analyses must check this rather than compare spans.
    #[must_use]
    pub const fn poem(&self) -> crate::seed::Fingerprint {
        self.poem
    }

    /// Every pronounceable word, in source order.
    #[must_use]
    pub fn words(&self) -> &[WordSurprisal] {
        &self.words
    }

    /// The most distinctive word, if the poem has any.
    ///
    /// Ties go to the earliest, so the answer does not depend on iteration order.
    #[must_use]
    pub fn sharpest(&self) -> Option<&WordSurprisal> {
        self.words
            .iter()
            .max_by(|a, b| a.surprisal.cmp(&b.surprisal).then(b.span.cmp(&a.span)))
    }
}

/// Scores every word of a poem.
#[must_use]
pub fn analyse(poem: &Poem, phonemiser: &impl Phonemiser, lexicon: &impl Lexicon) -> PoemSurprisal {
    let mut words = Vec::new();
    for stanza in poem.stanzas() {
        for line in stanza.lines() {
            words.extend(scan_line(line, poem, phonemiser, lexicon));
        }
    }
    PoemSurprisal {
        poem: poem.fingerprint(),
        words,
    }
}

/// Scores one line's words against their neighbours.
///
/// The window never crosses a line, so a word is only ever compared with words a reader hears
/// alongside it — and within a long line it reaches only [`NEIGHBOURS`] words either side.
fn scan_line(
    line: &Line,
    poem: &Poem,
    phonemiser: &impl Phonemiser,
    lexicon: &impl Lexicon,
) -> Vec<WordSurprisal> {
    let spoken: Vec<(Span, String, BTreeSet<Phoneme>, Provenance)> = line
        .words()
        .filter_map(|token| {
            let span = token.span();
            let text = span.text(poem.source())?;
            let pronunciation = phonemiser.pronounce(text).ok()?;
            let sounds: BTreeSet<Phoneme> = pronunciation.phonemes().collect();
            Some((
                span,
                text.to_lowercase(),
                sounds,
                pronunciation.provenance(),
            ))
        })
        .collect();

    spoken
        .iter()
        .enumerate()
        .map(|(index, (span, text, sounds, provenance))| {
            let first = index.saturating_sub(NEIGHBOURS);
            let last = index.saturating_add(NEIGHBOURS);
            let context: BTreeSet<Phoneme> = spoken
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index && *other >= first && *other <= last)
                .flat_map(|(_, (_, _, other_sounds, _))| other_sounds.iter().copied())
                .collect();
            let rarity = lexicon.rarity(text);
            let novelty = phoneme_set_distance(sounds, &context);
            WordSurprisal {
                span: *span,
                rarity,
                novelty,
                surprisal: combine(rarity, novelty),
                provenance: *provenance,
                function: lexicon.is_function_word(text),
            }
        })
        .collect()
}

/// How many sounds a word brings that its neighbours do not, up to [`SALIENT`].
///
/// Counted, not proportioned. Dividing by the word's own length looks natural and is wrong: a
/// one-phoneme word can then only score nothing or everything, so "a" and "I" reach the top of
/// the scale on a single unheard vowel. Measured on real poems, that made the sharpest word in
/// Dickinson "shorter" rather than "civility", and in Hughes "Maybe" rather than "explode" —
/// `SPEC.md` §1's failure mode, arriving through the denominator.
///
/// A word alone in its window has nothing to be unlike, so it scores nothing rather than
/// everything.
fn phoneme_set_distance(word: &BTreeSet<Phoneme>, context: &BTreeSet<Phoneme>) -> Surprisal {
    if word.is_empty() || context.is_empty() {
        return Surprisal::NONE;
    }
    let unheard = word.difference(context).count() as u64;
    Surprisal::ratio(unheard, SALIENT)
}

/// Weights the two halves into one score.
fn combine(rarity: Rarity, novelty: Surprisal) -> Surprisal {
    let bands = u64::from(Rarity::BANDS - 1);
    // Put both halves over a common denominator before weighting, so no rounding happens.
    let common = bands * novelty.denominator;
    let rarity_part = ALPHA * u64::from(rarity.get()) * novelty.denominator;
    let novelty_part = BETA * novelty.numerator * bands;
    Surprisal::ratio(rarity_part + novelty_part, (ALPHA + BETA) * common)
}

#[cfg(test)]
mod tests {
    use super::{Lexicon, PoemSurprisal, Rarity, Surprisal, analyse};
    use crate::parse::Poem;
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };

    const W: Stress = Stress::Unstressed;
    const S: Stress = Stress::Primary;

    struct Fake;

    static WORDS: &[(&str, &[(&str, Stress)])] = &[
        ("the", &[("DH", W), ("AH", W)]),
        ("and", &[("AE", W), ("N", W), ("D", W)]),
        ("burn", &[("B", W), ("ER", S), ("N", W)]),
        ("turn", &[("T", W), ("ER", S), ("N", W)]),
        ("spurn", &[("S", W), ("P", W), ("ER", S), ("N", W)]),
        ("glass", &[("G", W), ("L", W), ("AE", S), ("S", W)]),
        ("cheek", &[("CH", W), ("IY", S), ("K", W)]),
        ("zephyr", &[("Z", W), ("EH", S), ("F", W), ("ER", W)]),
    ];

    impl Phonemiser for Fake {
        fn inventory(&self) -> &'static Inventory {
            &ARPABET
        }
        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            let lower = word.to_lowercase();
            let entry = WORDS
                .iter()
                .find(|(w, _)| *w == lower)
                .ok_or(Unpronounceable::NotKnown)?;
            let phones: Vec<_> = entry
                .1
                .iter()
                .map(|(symbol, stress)| (ARPABET.phoneme(symbol).unwrap(), *stress))
                .collect();
            Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
        }
    }

    /// A table-driven lexicon, so tests do not depend on the corpus.
    struct Bands;

    impl Lexicon for Bands {
        // No closed class: every word keeps its dictionary stress under either variant.
        fn is_function_word(&self, _: &str) -> bool {
            false
        }

        fn rarity(&self, word: &str) -> Rarity {
            match word {
                "the" | "and" => Rarity::COMMONEST,
                "burn" | "turn" | "glass" => Rarity::band(4),
                "cheek" | "spurn" => Rarity::band(6),
                _ => Rarity::RAREST,
            }
        }
    }

    fn scored(source: &str) -> PoemSurprisal {
        analyse(&Poem::parse(source).unwrap(), &Fake, &Bands)
    }

    fn by_word(rep: &PoemSurprisal, source: &str, want: &str) -> Surprisal {
        rep.words()
            .iter()
            .find(|w| w.span().text(source) == Some(want))
            .map(super::WordSurprisal::surprisal)
            .unwrap()
    }

    #[test]
    fn a_rare_word_scores_above_a_common_one() {
        let source = "the zephyr and the glass\n";
        let rep = scored(source);
        assert!(by_word(&rep, source, "zephyr") > by_word(&rep, source, "the"));
    }

    #[test]
    fn the_same_word_scores_higher_among_unlike_neighbours() {
        // SPEC.md §4.4's second half: "burn" among rhyming words is ordinary; among unrelated
        // sounds it is not. The word and its rarity are identical in both lines.
        let among_kin = "burn turn spurn\n";
        let among_strangers = "burn glass cheek\n";
        let kin = by_word(&scored(among_kin), among_kin, "burn");
        let strangers = by_word(&scored(among_strangers), among_strangers, "burn");
        assert!(
            strangers > kin,
            "{} vs {}",
            strangers.as_f64(),
            kin.as_f64()
        );
    }

    #[test]
    fn the_two_halves_are_reported_separately() {
        let source = "burn turn spurn\n";
        let rep = scored(source);
        let burn = rep
            .words()
            .iter()
            .find(|w| w.span().text(source) == Some("burn"))
            .unwrap();
        // Its rarity is unchanged by context...
        assert_eq!(burn.rarity(), Rarity::band(4));
        // ...but almost all its sounds are heard in its neighbours.
        assert!(burn.novelty() < Surprisal::ratio(1, 2));
    }

    #[test]
    fn a_word_alone_in_its_line_has_nothing_to_be_unlike() {
        let source = "zephyr\n";
        let rep = scored(source);
        assert_eq!(rep.words()[0].novelty(), Surprisal::NONE);
        // Its rarity still counts, so it is not scoreless.
        assert!(rep.words()[0].surprisal() > Surprisal::NONE);
    }

    #[test]
    fn an_unknown_word_is_the_rarest_band() {
        assert_eq!(Bands.rarity("krapplesnaffle"), Rarity::RAREST);
        assert_eq!(Rarity::band(200), Rarity::RAREST);
        assert!(Rarity::RAREST > Rarity::COMMONEST);
    }

    #[test]
    fn the_sharpest_word_is_the_most_surprising() {
        let source = "the zephyr and the glass\n";
        let rep = scored(source);
        assert_eq!(rep.sharpest().unwrap().span().text(source), Some("zephyr"));
    }

    #[test]
    fn a_short_common_word_does_not_outscore_a_content_word() {
        // Dividing novelty by the word's own length let a one-phoneme word reach the top of the
        // scale on a single unheard vowel, so "a" outscored "burn". Measured on real poems that
        // put "shorter" above "civility" in Dickinson — SPEC.md §1's failure mode.
        let source = "the burn turn\n";
        let rep = scored(source);
        assert!(by_word(&rep, source, "burn") > by_word(&rep, source, "the"));
    }

    #[test]
    fn the_window_does_not_cross_a_line() {
        // A word alone on its line has no neighbours even when the poem has other lines. The old
        // fixture used a single-line poem, where "alone in the poem" and "alone in its line" are
        // indistinguishable — so a poem-wide context passed every test.
        let source = "burn turn spurn\nzephyr\n";
        let rep = scored(source);
        let alone = rep
            .words()
            .iter()
            .find(|w| w.span().text(source) == Some("zephyr"))
            .unwrap();
        assert_eq!(
            alone.novelty(),
            Surprisal::NONE,
            "its line holds nothing else, whatever the rest of the poem says"
        );
    }

    #[test]
    fn the_window_reaches_only_so_far_along_a_line() {
        // With a whole long line as context almost every phoneme is covered by something and the
        // term stops discriminating. The bounded neighbourhood keeps it alive.
        // The two lines differ only in a word five places away from "burn" — beyond the window,
        // so it must not move the score. With the whole line as context it would.
        let with_glass = "burn the and the the glass\n";
        let with_cheek = "burn the and the the cheek\n";
        assert_eq!(
            by_word(&scored(with_glass), with_glass, "burn"),
            by_word(&scored(with_cheek), with_cheek, "burn")
        );
    }

    #[test]
    fn ties_go_to_the_earliest_word() {
        // `max_by` keeps the *last* maximum, so the tie-break is deliberately reversed. Nothing
        // tested it, and flipping it to the obvious-looking direction passed the whole suite.
        let source = "burn burn\n";
        let rep = scored(source);
        let sharpest = rep.sharpest().unwrap();
        assert_eq!(sharpest.span().start(), 0, "the first of two equal words");
        assert_eq!(
            rep.words()[0].surprisal(),
            rep.words()[1].surprisal(),
            "the two really are tied"
        );
    }

    #[test]
    fn scores_compare_exactly() {
        // The Eq/Ord agreement this project has had to fix twice already.
        assert_eq!(Surprisal::ratio(1, 2), Surprisal::ratio(2, 4));
        assert_eq!(
            Surprisal::ratio(1, 2).cmp(&Surprisal::ratio(2, 4)),
            core::cmp::Ordering::Equal
        );
        assert!(Surprisal::NONE < Surprisal::ratio(1, 100));
        // A ratio above one is clamped rather than wrapping.
        assert_eq!(Surprisal::ratio(5, 2), Surprisal::ratio(1, 1));
        assert_eq!(Surprisal::ratio(1, 0), Surprisal::NONE);
    }

    #[test]
    fn an_unpronounceable_word_is_skipped() {
        let rep = scored("burn krapplesnaffle\n");
        assert_eq!(rep.words().len(), 1);
    }

    #[test]
    fn every_word_traces_to_its_span() {
        let source = "the zephyr and the glass\n";
        let rep = scored(source);
        let texts: Vec<&str> = rep
            .words()
            .iter()
            .filter_map(|w| w.span().text(source))
            .collect();
        assert_eq!(texts, ["the", "zephyr", "and", "the", "glass"]);
    }

    #[test]
    fn analysis_is_deterministic() {
        let source = "the zephyr and the glass\nburn turn spurn\n";
        assert_eq!(scored(source), scored(source));
    }
}
