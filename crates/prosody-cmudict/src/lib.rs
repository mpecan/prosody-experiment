//! The CMU Pronouncing Dictionary, as a [`Phonemiser`].
//!
//! # Why this is its own crate
//!
//! `prosody-core` may not read a file (`CLAUDE.md` §2), and invariant I1 disfavours a
//! pronunciation that depends on what happens to be installed on the machine. So the dictionary
//! is vendored in `data/` and embedded at compile time: the pronunciation of a given poem is a
//! property of this repository, identical everywhere, with nothing to load at runtime.
//!
//! It lives outside `prosody-core` only to keep three and a half megabytes of data out of the
//! crate every analysis pass depends on.
//!
//! # Coverage
//!
//! `CMUdict` is English-only — the limitation `SPEC.md` §9 records. A word it does not contain
//! yields [`Unpronounceable::NotKnown`] rather than a guess; the letter-to-sound fallback is a
//! separate implementation whose results are marked
//! [`Provenance::LetterToSound`], so a guess is never mistaken for a lookup.
//!
//! [`Provenance::LetterToSound`]: prosody_core::Provenance::LetterToSound

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::{Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, phoneme};

/// The dictionary text, vendored at `data/cmudict.dict` (BSD-2-Clause, see `data/LICENSE`).
const CMUDICT: &str = include_str!("../data/cmudict.dict");

/// One entry's phones, with the stress `CMUdict` marks on each vowel.
type Phones = Vec<(phoneme::Phoneme, Stress)>;

/// The CMU Pronouncing Dictionary.
///
/// Cheap to construct: the parsed table is built once per process and shared.
#[derive(Debug, Clone, Copy, Default)]
pub struct CmuDict {
    _private: (),
}

impl CmuDict {
    /// Creates a handle to the dictionary.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }

    /// How many words the dictionary knows.
    #[must_use]
    pub fn len(self) -> usize {
        table().len()
    }

    /// Whether the dictionary is empty, which would mean the vendored data failed to parse.
    #[must_use]
    pub fn is_empty(self) -> bool {
        table().is_empty()
    }

    /// Whether the dictionary has an entry for `word`.
    ///
    /// Note that an entry is not a promise of a pronunciation: `CMUdict` contains vowel-less
    /// interjections such as `hm` and `shh`, which have no syllable to build. Use
    /// [`Phonemiser::pronounce`], whose error says which case applies.
    #[must_use]
    pub fn contains(self, word: &str) -> bool {
        table().contains_key(&normalise(word))
    }

    /// Every pronunciation `CMUdict` lists for `word`, in the order the file gives them.
    ///
    /// [`Phonemiser::pronounce`] returns the first, deterministically. This exposes the rest
    /// because the choice is often poetically wrong — `CMUdict` lists `wind` as `W AY N D`, the
    /// verb, before the ordinary noun `W IH N D` — and a rhyme pass has information this layer
    /// does not. Keeping the alternates costs nothing and avoids a lossy decision that would be
    /// expensive to undo later. See `PLAN.md`.
    #[must_use]
    pub fn variants(self, word: &str) -> Vec<Pronunciation> {
        table()
            .get(&normalise(word))
            .into_iter()
            .flatten()
            .filter_map(|phones| phoneme::syllabify(phones).ok())
            .map(Pronunciation::from_dictionary)
            .collect()
    }
}

impl Phonemiser for CmuDict {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let entries = table()
            .get(&normalise(word))
            .ok_or(Unpronounceable::NotKnown)?;
        let phones = entries.first().ok_or(Unpronounceable::NotKnown)?;
        let syllables = phoneme::syllabify(phones)?;
        Ok(Pronunciation::from_dictionary(syllables))
    }
}

/// The parsed dictionary: normalised word to its pronunciations, in file order.
///
/// A `BTreeMap` rather than a `HashMap`: nothing iterates it today, but `CLAUDE.md` §4 asks for
/// the ordered container by default so that a future pass which does iterate cannot introduce
/// nondeterminism by accident.
fn table() -> &'static BTreeMap<String, Vec<Phones>> {
    static TABLE: OnceLock<BTreeMap<String, Vec<Phones>>> = OnceLock::new();
    TABLE.get_or_init(|| parse_entries(CMUDICT))
}

/// Parses dictionary text into the lookup table.
///
/// Split out from [`table`] so it can be tested on synthetic input: the vendored file happens to
/// contain no duplicate keys, so the ordering behaviour below is unobservable through the real
/// data and would otherwise go unexercised.
///
/// Entries like `read(2)` are alternate pronunciations of `read`. They are appended to that
/// word's list in file order rather than dropped, and [`Phonemiser::pronounce`] takes the first —
/// which makes the choice deterministic, as invariant I1 requires, without discarding the
/// alternates a later pass may need.
fn parse_entries(text: &str) -> BTreeMap<String, Vec<Phones>> {
    let mut table: BTreeMap<String, Vec<Phones>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else {
            continue;
        };
        let Some(phones) = parse_phones(fields) else {
            continue;
        };
        table
            .entry(normalise(base_word(key)))
            .or_default()
            .push(phones);
    }
    table
}

/// Strips an alternate-pronunciation suffix: `read(2)` becomes `read`.
fn base_word(key: &str) -> &str {
    match (key.find('('), key.strip_suffix(')')) {
        (Some(open), Some(trimmed)) => trimmed.get(..open).unwrap_or(key),
        _ => key,
    }
}

fn parse_phones<'a>(fields: impl Iterator<Item = &'a str>) -> Option<Phones> {
    let mut phones = Vec::new();
    for field in fields {
        let (symbol, stress) = split_stress(field);
        phones.push((ARPABET.phoneme(symbol)?, stress));
    }
    (!phones.is_empty()).then_some(phones)
}

/// Splits a `CMUdict` phone into its symbol and its stress digit: `ER1` becomes `("ER", Primary)`.
fn split_stress(field: &str) -> (&str, Stress) {
    match field.as_bytes().last() {
        Some(b'0') => (trim_last(field), Stress::Unstressed),
        Some(b'1') => (trim_last(field), Stress::Primary),
        Some(b'2') => (trim_last(field), Stress::Secondary),
        _ => (field, Stress::Unstressed),
    }
}

/// Drops the final byte. `saturating_sub` so an empty input cannot underflow — the subtraction,
/// not the slicing, is what would panic, and no lint catches that.
fn trim_last(field: &str) -> &str {
    field.get(..field.len().saturating_sub(1)).unwrap_or(field)
}

/// The key a word is stored and looked up under.
///
/// Lowercased, because `CMUdict` is lowercase and a poem is not. Nothing else is stripped: the
/// dictionary genuinely contains `'bout`, `'cause` and `'tis`, so removing edge apostrophes would
/// lose real entries.
fn normalise(word: &str) -> String {
    word.to_lowercase()
}

#[cfg(test)]
mod tests {
    use prosody_core::{Phoneme, Phonemiser, Provenance, Stress, SyllabifyError, Unpronounceable};

    use super::{CmuDict, base_word, parse_entries, trim_last};

    fn symbols(dict: CmuDict, word: &str) -> Vec<&'static str> {
        dict.pronounce(word)
            .unwrap()
            .phonemes()
            .map(Phoneme::symbol)
            .collect()
    }

    #[test]
    fn the_vendored_dictionary_parses() {
        let dict = CmuDict::new();
        assert!(!dict.is_empty());
        assert!(dict.len() > 100_000, "only {} entries", dict.len());
    }

    #[test]
    fn looks_up_a_word() {
        let dict = CmuDict::new();
        assert_eq!(symbols(dict, "burn"), ["B", "ER", "N"]);
        assert_eq!(symbols(dict, "burning"), ["B", "ER", "N", "IH", "NG"]);
        assert_eq!(symbols(dict, "burnt"), ["B", "ER", "N", "T"]);
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let dict = CmuDict::new();
        assert_eq!(symbols(dict, "Burn"), symbols(dict, "burn"));
        assert_eq!(symbols(dict, "BURN"), symbols(dict, "burn"));
    }

    #[test]
    fn syllable_counts_and_stress_are_what_a_reader_would_say() {
        let dict = CmuDict::new();
        let burning = dict.pronounce("burning").unwrap();
        assert_eq!(burning.syllable_count(), 2);
        assert_eq!(
            burning.stress_pattern(),
            [Stress::Primary, Stress::Unstressed]
        );

        let remember = dict.pronounce("remember").unwrap();
        assert_eq!(remember.syllable_count(), 3);
        assert_eq!(
            remember.stress_pattern(),
            [Stress::Unstressed, Stress::Primary, Stress::Unstressed]
        );

        // Four syllables, stress on the second: re-MEM-ber-ing.
        let remembering = dict.pronounce("remembering").unwrap();
        assert_eq!(remembering.syllable_count(), 4);
        assert_eq!(
            remembering.stress_pattern(),
            [
                Stress::Unstressed,
                Stress::Primary,
                Stress::Unstressed,
                Stress::Unstressed
            ]
        );
    }

    #[test]
    fn a_dictionary_lookup_says_so() {
        let dict = CmuDict::new();
        assert_eq!(
            dict.pronounce("burn").unwrap().provenance(),
            Provenance::Dictionary
        );
    }

    #[test]
    fn an_unknown_word_is_absent_rather_than_guessed() {
        let dict = CmuDict::new();
        assert_eq!(
            dict.pronounce("krapplesnaffle"),
            Err(Unpronounceable::NotKnown)
        );
        assert!(!dict.contains("krapplesnaffle"));
        // The Slovenian of SPEC.md §9: out of scope for an English dictionary, and it says so.
        assert_eq!(dict.pronounce("gorči"), Err(Unpronounceable::NotKnown));
    }

    #[test]
    fn a_known_word_with_no_vowel_is_distinguishable_from_an_unknown_one() {
        // "hm", "shh" and friends are real entries with no nucleus. Reporting both cases as a
        // bare `None` would leave the debug view unable to say which happened.
        let dict = CmuDict::new();
        for word in ["hm", "hmm", "mm", "sh", "shh"] {
            assert!(dict.contains(word), "{word} should be an entry");
            assert_eq!(
                dict.pronounce(word),
                Err(Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus)),
                "{word}"
            );
        }
        assert_ne!(
            dict.pronounce("hm"),
            dict.pronounce("krapplesnaffle"),
            "the two failure modes must be distinguishable"
        );
    }

    #[test]
    fn pronounce_takes_the_first_variant_and_the_rest_stay_reachable() {
        let dict = CmuDict::new();
        // CMUdict lists the past tense of "read" first, so that is what a deterministic
        // first-entry rule yields.
        assert_eq!(symbols(dict, "read"), ["R", "EH", "D"]);

        let all: Vec<Vec<&str>> = dict
            .variants("read")
            .iter()
            .map(|p| p.phonemes().map(Phoneme::symbol).collect())
            .collect();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], ["R", "EH", "D"]);
        assert_eq!(
            all[1],
            ["R", "IY", "D"],
            "the present tense stays reachable"
        );

        // The variant key itself is not addressable.
        assert!(!dict.contains("read(2)"));
    }

    #[test]
    fn the_common_sense_of_a_heteronym_may_not_be_the_first() {
        // Documented, deliberate: SPEC.md §9 accepts the ambiguity rather than building a
        // part-of-speech tagger. "wind" resolves to the verb, not the breeze — a rhyme pass that
        // cares can reach the noun through `variants`. See PLAN.md.
        let dict = CmuDict::new();
        assert_eq!(symbols(dict, "wind"), ["W", "AY", "N", "D"]);
        let alternates: Vec<Vec<&str>> = dict
            .variants("wind")
            .iter()
            .map(|p| p.phonemes().map(Phoneme::symbol).collect())
            .collect();
        assert!(
            alternates.contains(&vec!["W", "IH", "N", "D"]),
            "the breeze must remain reachable: {alternates:?}"
        );
    }

    #[test]
    fn variants_of_an_unknown_word_are_empty() {
        assert!(CmuDict::new().variants("krapplesnaffle").is_empty());
    }

    #[test]
    fn elided_forms_with_a_leading_apostrophe_are_real_entries() {
        // Relevant to the tokeniser's apostrophe handling: these are genuine words, so stripping
        // edge apostrophes before lookup would lose them.
        let dict = CmuDict::new();
        assert!(dict.contains("'tis"));
        assert!(dict.contains("'bout"));
        assert_eq!(symbols(dict, "'bout"), ["B", "AW", "T"]);
    }

    #[test]
    fn rhyming_words_share_a_tail() {
        let dict = CmuDict::new();
        let tail = |word: &str| -> Vec<&'static str> {
            dict.pronounce(word)
                .unwrap()
                .rhyme_tail()
                .iter()
                .copied()
                .map(Phoneme::symbol)
                .collect()
        };
        assert_eq!(tail("burning"), tail("turning"));
        assert_eq!(tail("burning"), ["ER", "N", "IH", "NG"]);
        assert_eq!(tail("remember"), tail("december"));
        // Different syllable counts, same rhyme.
        assert_eq!(tail("fire"), tail("desire"));
    }

    #[test]
    fn the_inventory_is_reported() {
        assert_eq!(CmuDict::new().inventory().name, "arpabet");
    }

    #[test]
    fn lookups_are_deterministic_across_calls() {
        let dict = CmuDict::new();
        for word in ["burn", "remember", "the", "fire", "tide"] {
            assert_eq!(dict.pronounce(word), dict.pronounce(word));
        }
    }

    // The parser, on synthetic input. The vendored file has no duplicate keys, so these
    // orderings are unobservable through the real data and would otherwise go untested.

    #[test]
    fn variants_are_kept_in_file_order() {
        let table = parse_entries("read R EH1 D\nread(2) R IY1 D\nread(3) R AH0 D\n");
        let entries = table.get("read").unwrap();
        assert_eq!(entries.len(), 3);
        let first: Vec<&str> = entries[0].iter().map(|(p, _)| p.symbol()).collect();
        let second: Vec<&str> = entries[1].iter().map(|(p, _)| p.symbol()).collect();
        assert_eq!(first, ["R", "EH", "D"]);
        assert_eq!(second, ["R", "IY", "D"]);
    }

    #[test]
    fn a_duplicate_key_appends_rather_than_replacing() {
        // Not present in today's data, but a regenerated dictionary could introduce it. The
        // first line must stay first, so `pronounce` keeps answering the same way.
        let table = parse_entries("burn B ER1 N\nburn B AA1 R N\n");
        let entries = table.get("burn").unwrap();
        assert_eq!(entries.len(), 2);
        let first: Vec<&str> = entries[0].iter().map(|(p, _)| p.symbol()).collect();
        assert_eq!(first, ["B", "ER", "N"]);
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let table = parse_entries("\n  \nburn B ER1 N # a comment\n# whole-line comment\n");
        assert_eq!(table.len(), 1);
        assert_eq!(table.get("burn").unwrap().len(), 1);
    }

    #[test]
    fn entries_with_an_unknown_symbol_are_skipped() {
        let table = parse_entries("burn B ER1 N\nweird B QQ1 N\n");
        assert!(table.contains_key("burn"));
        assert!(!table.contains_key("weird"));
    }

    #[test]
    fn base_word_strips_a_variant_suffix() {
        assert_eq!(base_word("read"), "read");
        assert_eq!(base_word("read(2)"), "read");
        assert_eq!(base_word("read(12)"), "read");
        // Parentheses that are part of the word, not a suffix, are left alone.
        assert_eq!(base_word("(unclosed"), "(unclosed");
        assert_eq!(base_word(""), "");
    }

    #[test]
    fn trim_last_handles_an_empty_string() {
        assert_eq!(trim_last(""), "");
        assert_eq!(trim_last("ER1"), "ER");
    }
}
