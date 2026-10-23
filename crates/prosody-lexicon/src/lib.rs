//! How common an English word is.
//!
//! `SPEC.md` §4.4 wants lexical rarity to drive transient sharpness and brightness, from "a static
//! unigram table, offline". This is that table.
//!
//! # Its own crate
//!
//! Rarity is language data, exactly as a pronunciation dictionary is, and `prosody-core` holds the
//! language-agnostic model. The [`Lexicon`] trait lives there; this crate is one implementation of
//! it, so a second language can arrive without anything inheriting English word frequencies.
//!
//! # Where the numbers come from
//!
//! Fourteen long-public-domain English works — nine of verse, five of prose — counted together:
//! 1,909,419 tokens, 43,119 distinct words. The commonest twenty thousand are stored in log-spaced
//! bands; everything else is [`Rarity::RAREST`] by absence. Only the derived counts ship, never the
//! texts. See `data/SOURCES.md`.
//!
//! Each source contributes its own *relative* frequencies with equal weight. Counted raw, the King
//! James Bible is four fifths of the corpus and decided the table on its own — "thou" ranked
//! alongside "not", while `coffee`, `hemlock` and `gossamer` all fell into the same bucket as
//! invented gibberish.
//!
//! A public-domain corpus rather than a web corpus, deliberately. What should sound striking in a
//! poem is a word that is rare *in poems*; web frequencies would call "thee" and "hath" exotic and
//! "click" ordinary, which is backwards for this instrument. The cost is that nothing after 1929
//! is in it, so `phone` and `jazz` score as maximally rare.
//!
//! # Function words
//!
//! The lexicon also answers which words are English *function words* — articles, pronouns,
//! possessives, conjunctions, short prepositions, auxiliaries — for the light variant of
//! `Mapping::Function` (`SPEC.md` §4.2). A fixed, written-out list, not derived from the counts:
//! frequency and word class are different facts ("thou" is rare in prose and closed-class
//! everywhere). See `src/function.rs` for the list and what was left out of it.

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

mod function;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use prosody_core::surprisal::{Lexicon, Rarity};

/// The band table, vendored at `data/rarity.txt`.
const TABLE: &str = include_str!("../data/rarity.txt");

/// English word frequency, as bands.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct EnglishLexicon;

impl EnglishLexicon {
    /// Creates the lexicon.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// How many words it knows.
    #[must_use]
    pub fn len(self) -> usize {
        bands().len()
    }

    /// Whether the table failed to load, which would be a build problem.
    #[must_use]
    pub fn is_empty(self) -> bool {
        bands().is_empty()
    }
}

impl Lexicon for EnglishLexicon {
    /// Unknown words are [`Rarity::RAREST`] — a coinage or a proper noun really is unusual, and
    /// `SPEC.md` §4.4 wants exactly that to cut through.
    fn rarity(&self, word: &str) -> Rarity {
        bands().get(&key(word)).copied().unwrap_or(Rarity::RAREST)
    }

    /// The closed class listed in `src/function.rs`, matched like [`Lexicon::rarity`]: any case, edge
    /// apostrophes ignored.
    fn is_function_word(&self, word: &str) -> bool {
        function::contains(&key(word))
    }
}

/// A word as both tables are keyed: lowercase, with edge apostrophes (straight or curly) trimmed.
fn key(word: &str) -> String {
    word.trim_matches(['\'', '\u{2019}']).to_lowercase()
}

/// A `BTreeMap` rather than a `HashMap`, per `CLAUDE.md` §4 — nothing iterates it today, but the
/// ordered container cannot introduce nondeterminism if something later does.
fn bands() -> &'static BTreeMap<String, Rarity> {
    static BANDS: OnceLock<BTreeMap<String, Rarity>> = OnceLock::new();
    BANDS.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let word = fields.next()?;
                let band: u8 = fields.next()?.parse().ok()?;
                Some((word.to_owned(), Rarity::band(band)))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use prosody_core::surprisal::{Lexicon, Rarity};

    use super::EnglishLexicon;

    #[test]
    fn the_table_loads() {
        let lexicon = EnglishLexicon::new();
        assert!(!lexicon.is_empty());
        assert_eq!(lexicon.len(), 20_000);
    }

    #[test]
    fn the_commonest_words_sit_in_the_lowest_bands() {
        let lexicon = EnglishLexicon::new();
        // The bands are log-spaced, so only the very commonest few reach zero.
        for word in ["the", "and", "of"] {
            assert_eq!(lexicon.rarity(word), Rarity::COMMONEST, "{word}");
        }
        for word in ["to", "in", "that", "his", "with"] {
            assert!(
                lexicon.rarity(word).get() <= 1,
                "{word} should be near-commonest"
            );
        }
    }

    #[test]
    fn ordinary_words_sit_between_the_extremes() {
        let lexicon = EnglishLexicon::new();
        for word in [
            "love", "burn", "tide", "raven", "hemlock", "gossamer", "coffee",
        ] {
            let band = lexicon.rarity(word);
            assert!(band > Rarity::COMMONEST, "{word} is not that common");
            assert!(band < Rarity::RAREST, "{word} is not that rare");
        }
    }

    #[test]
    fn a_word_the_corpus_never_saw_is_the_rarest() {
        let lexicon = EnglishLexicon::new();
        for word in ["krapplesnaffle", "phosphorescent", "gorči"] {
            assert_eq!(lexicon.rarity(word), Rarity::RAREST, "{word}");
        }
    }

    #[test]
    fn rarity_rises_as_words_get_less_common() {
        let lexicon = EnglishLexicon::new();
        assert!(lexicon.rarity("love") < lexicon.rarity("raven"));
        assert!(lexicon.rarity("the") < lexicon.rarity("love"));
    }

    #[test]
    fn lookup_ignores_case_and_edge_apostrophes() {
        let lexicon = EnglishLexicon::new();
        assert_eq!(lexicon.rarity("Love"), lexicon.rarity("love"));
        assert_eq!(lexicon.rarity("LOVE"), lexicon.rarity("love"));
        assert_eq!(lexicon.rarity("'love'"), lexicon.rarity("love"));
    }

    #[test]
    fn a_poetry_corpus_knows_poetry_words() {
        // The point of not using a web corpus: these are ordinary in verse and exotic online.
        let lexicon = EnglishLexicon::new();
        for word in ["thee", "thy", "hath", "doth"] {
            assert!(
                lexicon.rarity(word) < Rarity::RAREST,
                "{word} should be known"
            );
        }
    }

    #[test]
    fn function_words_are_matched_like_rarity() {
        let lexicon = EnglishLexicon::new();
        for word in ["or", "Thy", "WHAT", "'the'", "I", "thee\u{2019}"] {
            assert!(lexicon.is_function_word(word), "{word}");
        }
        for word in ["hand", "Tyger", "symmetry", "krapplesnaffle", ""] {
            assert!(!lexicon.is_function_word(word), "{word}");
        }
    }

    #[test]
    fn lookups_are_deterministic() {
        let lexicon = EnglishLexicon::new();
        for word in ["the", "burn", "krapplesnaffle"] {
            assert_eq!(lexicon.rarity(word), lexicon.rarity(word));
        }
    }
}
