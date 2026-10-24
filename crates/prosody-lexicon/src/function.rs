//! English function words: the closed class the light variant of `Mapping::Function` reads
//! (`SPEC.md` §4.2).
//!
//! # Why here
//!
//! Which words carry grammar rather than content is a fact about one language, as its word
//! frequencies are (`SPEC.md` §9): the core asks through
//! [`Lexicon::is_function_word`](prosody_core::surprisal::Lexicon::is_function_word) and never
//! names an English word. This crate is already the English lexicon, so the list lives beside the
//! frequency table — a Slovenian lexicon would bring its own.
//!
//! # What is in it
//!
//! A fixed list, written out below and never derived from data, so it is the same on every build
//! (I1) and reviewable at a glance. Closed-class English words only:
//!
//! - **articles**: a, an, the;
//! - **pronouns**, archaic ones included: I, me, we, us, you, ye, thou, thee, he, him, she, her,
//!   it, they, them, the demonstratives this, that, these, those, and the relatives and
//!   interrogatives who, whom, whose, what, which;
//! - **possessives**: my, mine, our, ours, your, yours, thy, thine, his, hers, its, their, theirs;
//! - **conjunctions**: and, or, nor, but, if, as, than, though, tho;
//! - **short prepositions**: at, by, for, from, in, of, on, to, with, through, thro, till;
//! - **auxiliaries and modals**, archaic forms included: am, is, are, was, were, wert, be, been,
//!   do, does, did, doth, dost, didst, has, have, had, hath, hast, shall, shalt, will, wilt, would,
//!   wouldst, should, shouldst, could, couldst, can, canst, may, mayst, might, must.
//!
//! Left out on purpose, because in verse they so often carry the line's contrast that a poet
//! leans on them: the negatives (`not`, `no`, `nor` aside), the quantifiers (`all`, `each`,
//! `some`), the adverbs that double as conjunctions (`so`, `yet`, `when`, `where`, `how`,
//! `while`), particles that are stressed in phrasal verbs (`up`, `down`, `out`, `off`), `there`,
//! and `art` — "thou art", but also The Tyger's "what art", a noun. A word that is sometimes a
//! noun or verb ("will", "can", "may", "might") is listed: in a short line it is mostly the
//! auxiliary, and where it is not, a strong metrical position keeps its accent anyway.
//!
//! Multi-syllable closed-class words ("upon", "into", "about") are not listed: the rule applies to
//! one-syllable words only, since a longer word's dictionary stress pattern is its own.

/// The list, lowercase and sorted, so a lookup is a binary search. A test checks both.
const WORDS: &[&str] = &[
    "a", "am", "an", "and", "are", "as", "at", "be", "been", "but", "by", "can", "canst", "could",
    "couldst", "did", "didst", "do", "does", "dost", "doth", "for", "from", "had", "has", "hast",
    "hath", "have", "he", "her", "hers", "him", "his", "i", "if", "in", "is", "it", "its", "may",
    "mayst", "me", "might", "mine", "must", "my", "nor", "of", "on", "or", "our", "ours", "shall",
    "shalt", "she", "should", "shouldst", "than", "that", "the", "thee", "their", "theirs", "them",
    "these", "they", "thine", "this", "tho", "those", "thou", "though", "thro", "through", "thy",
    "till", "to", "us", "was", "we", "were", "wert", "what", "which", "who", "whom", "whose",
    "will", "wilt", "with", "would", "wouldst", "ye", "you", "your", "yours",
];

/// Whether `word` (already lowercased, edge apostrophes trimmed) is on the list.
pub(crate) fn contains(word: &str) -> bool {
    WORDS.binary_search(&word).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{WORDS, contains};

    #[test]
    fn the_list_is_sorted_lowercase_and_without_repeats() {
        for pair in WORDS.windows(2) {
            assert!(pair[0] < pair[1], "{} before {}", pair[0], pair[1]);
        }
        for word in WORDS {
            assert_eq!(*word, word.to_lowercase(), "{word}");
            assert!(!word.is_empty() && !word.contains(' '), "{word:?}");
        }
    }

    #[test]
    fn the_words_the_tyger_strikes_too_hard_are_on_it() {
        // The step's own examples (PLAN.md, "softening function words").
        for word in [
            "or", "thy", "what", "the", "a", "and", "of", "to", "in", "thee", "thou",
        ] {
            assert!(contains(word), "{word}");
        }
    }

    #[test]
    fn content_words_and_the_deliberate_omissions_are_not() {
        for word in [
            "hand", "eye", "tyger", "burning", "art", "not", "no", "all", "so", "when", "up",
            "there", "upon",
        ] {
            assert!(!contains(word), "{word}");
        }
    }
}
