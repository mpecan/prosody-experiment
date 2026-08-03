//! Guessing how a word sounds from how it is spelled.
//!
//! `SPEC.md` §7.2 asks for a letter-to-sound fallback so a word `CMUdict` has never heard of still
//! makes a sound. Poems are full of them — coinages, proper nouns, dialect, and the
//! "krapplesnaffle" case that the vocable-synthesis lineage in §2 exists to serve.
//!
//! # Its own crate
//!
//! This is a concrete, English-only `Phonemiser`, the same kind of thing as `prosody-cmudict` and
//! kept out of `prosody-core` for the same reason: the core holds the inventory-agnostic model
//! that `SPEC.md` §9 requires, and a Slovenian or eSpeak-NG phonemiser should not inherit a
//! hard-coded English ruleset by depending on it.
//!
//! # This is a guess, and says so
//!
//! Every pronunciation from here is [`LetterToSound`](prosody_core::Provenance::LetterToSound)
//! provenance. English spelling is not a
//! function of English sound, so many of these are wrong; what matters is that a wrong guess is
//! never mistaken for a dictionary fact. `CLAUDE.md` §6 requires the fallback to be visible.
//!
//! # How wrong
//!
//! Measured against 117,000 gradable `CMUdict` entries, not estimated — `tests/accuracy.rs`
//! grades every one and fails the build if the figures regress:
//!
//! | measure | |
//! |---|---|
//! | coverage — words it will attempt | 99.9% |
//! | syllable count exactly right | 83.5% |
//! | primary stress in the right place | 67.7% |
//! | whole word exactly right | 25.4% |
//! | phoneme error rate | 0.24 |
//!
//! Syllable count is the figure that matters most: `SPEC.md` §4.2 maps syllables to subdivisions
//! and feet, so a wrong count changes the rhythm, while a wrong vowel only changes the timbre.
//! Whole-word agreement is the harshest possible measure and is dominated by vowel quality in
//! unstressed syllables, which spelling genuinely does not determine.
//!
//! Stress is the weakest part. Spelling cannot recover it, so the first syllable takes primary
//! stress except for suffix families where that is reliably wrong (`-tion`, `-ic`).

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::{
    Inventory, Phoneme, Phonemiser, Pronunciation, Stress, SyllabifyError, Unpronounceable, phoneme,
};

/// Guesses pronunciations from spelling, in `ARPAbet`.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct LetterToSound;

impl LetterToSound {
    /// Creates the guesser.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Phonemiser for LetterToSound {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    /// Guesses how `word` sounds.
    ///
    /// # Errors
    ///
    /// [`Unpronounceable::NotKnown`] if the word contains a character no rule covers — dropping
    /// it instead would produce a confident wrong answer. `Unsyllabifiable` if the rules run but
    /// yield no vowel, as for "brrr": the same distinction `prosody-cmudict` draws for `hm`, and
    /// the reason [`Unpronounceable`] has two variants.
    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let letters = normalise(word).ok_or(Unpronounceable::NotKnown)?;
        if letters.is_empty() {
            return Err(Unpronounceable::NotKnown);
        }
        let symbols = transcribe(&letters);
        let phones = stressed(&letters, &symbols)
            .ok_or(Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus))?;
        let syllables = phoneme::syllabify(&phones)?;
        Ok(Pronunciation::guessed(syllables))
    }
}

/// Lowercases, folds diacritics, and drops characters that carry no sound of their own.
///
/// Returns `None` if a character survives folding without being a letter the rules know.
/// Apostrophes and hyphens are dropped rather than rejected: they join parts of a word
/// (`burn'd`, `half-light`) rather than adding sound.
fn normalise(word: &str) -> Option<Vec<char>> {
    let mut letters = Vec::with_capacity(word.len());
    for raw in word.chars().flat_map(char::to_lowercase) {
        match raw {
            '\'' | '\u{2019}' | '-' | '\u{2010}' => {}
            'a'..='z' => letters.push(raw),
            other => letters.extend(fold_diacritic(other)?.chars()),
        }
    }
    Some(letters)
}

/// Rewrites an accented Latin letter as the English spelling that sounds closest.
///
/// `SPEC.md` §9 says eSpeak-NG or per-language dictionaries are the real answer for non-English
/// text, and this is not that. It exists so a Slovenian or German word degrades to an audible,
/// visibly-guessed approximation rather than to silence.
///
/// Some of these are more than one letter, which is the point: Slovenian `č` is far closer to
/// English `ch` than to a bare `c`, and German `ß` is `ss`. Folding to a single base letter would
/// throw that away.
fn fold_diacritic(c: char) -> Option<&'static str> {
    let folded = match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' => "c",
        'č' => "ch",
        'ď' | 'đ' => "d",
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'í' | 'ì' | 'î' | 'ï' | 'ĩ' | 'ī' | 'į' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ş' | 'ŝ' | 'ș' => "s",
        'š' => "sh",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'ú' | 'ù' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' => "z",
        'ž' => "zh",
        _ => return None,
    };
    Some(folded)
}

fn is_vowel_letter(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// The long ("name of the letter") value of a vowel, given what precedes it.
///
/// Only `u` needs the context. Long `u` is /juː/ — "cube" is `K Y UW B`, not `K UW B` — but the
/// glide drops after coronals and liquids, so "rule" and "dune" keep a plain `UW`.
fn long_vowel(letters: &[char], index: usize, c: char) -> Option<&'static [&'static str]> {
    if c == 'u' {
        return Some(if yod_drops(letters, index) {
            &["UW"]
        } else {
            &["Y", "UW"]
        });
    }
    Some(match c {
        'a' => &["EY"],
        'e' => &["IY"],
        'i' => &["AY"],
        'o' => &["OW"],
        _ => return None,
    })
}

/// Whether the glide of a long `u` is lost after the preceding consonant.
fn yod_drops(letters: &[char], index: usize) -> bool {
    let Some(previous) = index.checked_sub(1).and_then(|i| letters.get(i)).copied() else {
        return false;
    };
    // "chute" and "sure" lose it; "huge" keeps it, so `h` alone is not enough.
    if previous == 'h' {
        return matches!(
            index.checked_sub(2).and_then(|i| letters.get(i)).copied(),
            Some('c' | 's')
        );
    }
    matches!(previous, 'r' | 'l' | 'j' | 's' | 'd' | 't' | 'n' | 'z')
}

/// The short value of a vowel.
fn short_vowel(c: char) -> Option<&'static [&'static str]> {
    Some(match c {
        'a' => &["AE"],
        'e' => &["EH"],
        'i' => &["IH"],
        'o' => &["AA"],
        'u' => &["AH"],
        _ => return None,
    })
}

/// Multi-letter spellings. Declaration order does not matter: [`match_grapheme`] takes the
/// longest match, not the first, so a rule can be added anywhere without shadowing another.
static GRAPHEMES: &[(&str, &[&str])] = &[
    ("tch", &["CH"]),
    ("dge", &["JH"]),
    ("igh", &["AY"]),
    ("ch", &["CH"]),
    ("sh", &["SH"]),
    ("ph", &["F"]),
    ("wh", &["W"]),
    ("ck", &["K"]),
    ("ng", &["NG"]),
    ("qu", &["K", "W"]),
    ("ee", &["IY"]),
    ("ea", &["IY"]),
    ("oo", &["UW"]),
    ("ou", &["AW"]),
    ("ow", &["OW"]),
    ("oi", &["OY"]),
    ("oy", &["OY"]),
    ("ai", &["EY"]),
    ("ay", &["EY"]),
    ("oa", &["OW"]),
    ("au", &["AO"]),
    ("aw", &["AO"]),
    ("ei", &["EY"]),
    ("ie", &["IY"]),
];

/// Vowel + `r` spellings, which only apply when no vowel follows the `r`.
///
/// "burn" is `B ER N`, but "hero" is `HH IH R OW` and "very" is `V EH R IY` — an `r` that opens
/// the next syllable is a plain consonant, not part of the vowel.
static R_CONTROLLED: &[(&str, &[&str])] = &[
    ("ar", &["AA", "R"]),
    ("or", &["AO", "R"]),
    ("er", &["ER"]),
    ("ir", &["ER"]),
    ("ur", &["ER"]),
];

/// Function words whose initial `th` is voiced.
///
/// English spells /θ/ and /ð/ identically, and which one a word takes is lexical, not
/// positional — so a closed list is the only honest rule. These are the common ones, and they are
/// frequent enough in verse to be worth naming.
static VOICED_TH: &[&str] = &[
    "the",
    "this",
    "that",
    "these",
    "those",
    "then",
    "than",
    "they",
    "them",
    "their",
    "there",
    "though",
    "thus",
    "thee",
    "thy",
    "thine",
    "thence",
    "thither",
    "themselves",
];

/// Turns folded letters into `ARPAbet` symbols.
fn transcribe(letters: &[char]) -> Vec<&'static str> {
    let word: String = letters.iter().collect();
    let voiced_th = VOICED_TH.contains(&word.as_str());

    // A consonant + `le` ending is its own syllable and is emitted at the end; the main loop
    // stops before it. Otherwise the `l` and `e` would also be transcribed normally.
    let consonant_le = ends_with_consonant_le(letters);
    let body = if consonant_le {
        letters.len().saturating_sub(2)
    } else {
        letters.len()
    };

    // The stem is the word without an inflectional `d` or `s`, so "hoped" is treated as "hope"
    // plus a suffix and keeps its silent `e`. Every index below is relative to the stem.
    let stem_len = strip_inflection(letters).len().min(body);
    let silent_e_at = has_silent_final_e(letters)
        .then(|| stem_len.checked_sub(1))
        .flatten();
    let magic_e_vowel = silent_e_at.and_then(|_| stem_len.checked_sub(3));
    // "table" lengthens its vowel the way "make" does: v-C-l-e is the same shape as v-C-e.
    let le_vowel = consonant_le.then(|| letters.len().checked_sub(4)).flatten();

    let mut symbols = Vec::with_capacity(letters.len());
    let mut index = 0;
    while index < body {
        if Some(index) == silent_e_at {
            index += 1;
            continue;
        }

        if let Some(consumed) = silent_start(letters, index) {
            index += consumed;
            continue;
        }

        // The vowel before a silent final `e` says its own name: "make", not "mack".
        if (Some(index) == magic_e_vowel || Some(index) == le_vowel)
            && let Some(&letter) = letters.get(index)
            && let Some(phones) = long_vowel(letters, index, letter)
        {
            symbols.extend_from_slice(phones);
            index += 1;
            continue;
        }

        if let Some(phones) = th_sound(letters, index, voiced_th) {
            symbols.extend_from_slice(phones);
            index += 2;
            continue;
        }

        if let Some((length, phones)) = gh_sound(letters, index) {
            symbols.extend_from_slice(phones);
            index += length;
            continue;
        }

        if let Some((length, phones)) = match_r_controlled(letters, index, silent_e_at, body) {
            symbols.extend_from_slice(phones);
            index += length;
            continue;
        }

        if let Some((length, phones)) = match_grapheme(letters, index, silent_e_at, body) {
            symbols.extend_from_slice(phones);
            index += length;
            continue;
        }

        let Some(&letter) = letters.get(index) else {
            break;
        };

        // A doubled consonant is one sound: "runner", not "run-ner". But `cc` and `gg` before a
        // front vowel are two different sounds — "accent" is `AE K S EH N T`, not `AE S EH N T`.
        if !is_vowel_letter(letter)
            && letters.get(index + 1) == Some(&letter)
            && !splits_when_doubled(letters, index, letter)
        {
            index += 1;
            continue;
        }

        if let Some(phones) = final_vowel(letters, index, letter, body) {
            symbols.extend_from_slice(phones);
            index += 1;
            continue;
        }

        if let Some(phones) = single_letter(letters, index, letter) {
            symbols.extend_from_slice(phones);
        }
        index += 1;
    }

    if consonant_le {
        symbols.push("AH");
        symbols.push("L");
    }
    symbols
}

/// Whether a doubled `c` or `g` is two distinct sounds rather than one.
fn splits_when_doubled(letters: &[char], index: usize, letter: char) -> bool {
    matches!(letter, 'c' | 'g') && matches!(letters.get(index + 2), Some('e' | 'i' | 'y'))
}

/// Silent letters that only occur at the start of a word.
///
/// Returns how many letters to skip. "knight" is `N AY T`, "write" is `R AY T`, "psalm" opens on
/// `S`.
fn silent_start(letters: &[char], index: usize) -> Option<usize> {
    if index != 0 {
        return None;
    }
    let pair = (letters.first()?, letters.get(1)?);
    match (pair.0, pair.1) {
        ('k' | 'g', 'n') | ('p', 's' | 'n') | ('w', 'r') => Some(1),
        _ => None,
    }
}

/// `th`, voiced or not.
fn th_sound(letters: &[char], index: usize, voiced_word: bool) -> Option<&'static [&'static str]> {
    if letters.get(index) != Some(&'t') || letters.get(index + 1) != Some(&'h') {
        return None;
    }
    // Voiced in the closed list of function words, and between vowels: "mother", "weather".
    let between_vowels = index > 0
        && letters
            .get(index.wrapping_sub(1))
            .is_some_and(|&c| is_vowel_letter(c))
        && letters.get(index + 2).is_some_and(|&c| is_vowel_letter(c));
    let voiced = (voiced_word && index == 0) || between_vowels;
    Some(if voiced { &["DH"] } else { &["TH"] })
}

/// `gh`: silent after a vowel, a plain `g` at the start of a word.
///
/// "though" and "night" lose it; "ghost" keeps it. "laugh" and "cough" want `F`, which this rule
/// gets wrong — English does not decide this by position, and a closed list is not worth it here.
fn gh_sound(letters: &[char], index: usize) -> Option<(usize, &'static [&'static str])> {
    if letters.get(index) != Some(&'g') || letters.get(index + 1) != Some(&'h') {
        return None;
    }
    if index == 0 {
        return Some((2, &["G"]));
    }
    let after_vowel = index
        .checked_sub(1)
        .and_then(|i| letters.get(i))
        .is_some_and(|&c| is_vowel_letter(c));
    after_vowel.then_some((2, &[]))
}

/// Whether the word ends in consonant + `le`, which forms its own syllable.
fn ends_with_consonant_le(letters: &[char]) -> bool {
    let length = letters.len();
    if length < 3 {
        return false;
    }
    letters.get(length - 2) == Some(&'l')
        && letters.get(length - 1) == Some(&'e')
        && letters
            .get(length - 3)
            .is_some_and(|&c| !is_vowel_letter(c))
}

/// A vowel that ends a word.
///
/// "hero" and "piano" end on a full `OW`, not a reduced schwa; "he" and "she" say `IY`. Without
/// this the last vowel of every such word came out short and was then reduced away.
fn final_vowel(
    letters: &[char],
    index: usize,
    letter: char,
    body: usize,
) -> Option<&'static [&'static str]> {
    if index + 1 != body {
        return None;
    }
    if letters == ['t', 'h', 'e'] {
        return Some(&["AH"]);
    }
    Some(match letter {
        'o' => &["OW"],
        'i' => &["IY"],
        'u' => &["UW"],
        // A final `e` only sounds in a very short word: "he", "she", "be".
        'e' if letters.len() <= 3 => &["IY"],
        // A final `a` is a schwa, which the short vowel plus reduction already gives.
        _ => return None,
    })
}

/// Whether the word ends in an `e` that is silent and lengthens the preceding vowel.
///
/// Looks through an inflectional suffix first: "hoped" is "hope" plus `d`, so its `e` is silent
/// even though it is not the last letter. Without this every past tense and plural is wrong —
/// "hoped" would come out `HH AA P EH D`.
fn has_silent_final_e(letters: &[char]) -> bool {
    let effective = strip_inflection(letters);
    let Some((&last, rest)) = effective.split_last() else {
        return false;
    };
    if last != 'e' || rest.len() < 2 {
        return false;
    }
    let Some(&before) = rest.last() else {
        return false;
    };
    // A vowel before the `e` makes a digraph instead. A consonant + `le` keeps its own syllable
    // ("table"), but "smile" and "rule" have a vowel before the `l` and are ordinary silent-e.
    let consonant_before_l = before == 'l'
        && rest
            .len()
            .checked_sub(2)
            .and_then(|i| rest.get(i))
            .is_some_and(|&c| !is_vowel_letter(c));
    if is_vowel_letter(before) || consonant_before_l {
        return false;
    }
    // `y` counts as a vowel only away from the start of the word, where it is a consonant.
    // Counting a leading `y` let the rule silence a word's only nucleus: "ybe" lost its `e`.
    rest.iter()
        .enumerate()
        .any(|(i, &c)| is_vowel_letter(c) || (c == 'y' && i > 0))
}

/// The word with a trailing inflectional `d` or `s` removed, if that leaves a plausible stem.
fn strip_inflection(letters: &[char]) -> &[char] {
    let length = letters.len();
    if length < 4 {
        return letters;
    }
    let Some(&last) = letters.last() else {
        return letters;
    };
    if !matches!(last, 'd' | 's') {
        return letters;
    }
    // Only when an `e` sits immediately before it, which is what makes the stem's own `e` final.
    if letters.get(length - 2) == Some(&'e') {
        letters.get(..length - 1).unwrap_or(letters)
    } else {
        letters
    }
}

/// The longest multi-letter spelling starting at `index`, if any.
///
/// Longest match, not first match: the table's declaration order is then irrelevant, so adding a
/// rule cannot silently shadow another. An earlier version scanned in order and a shorter rule
/// declared first would win.
fn match_grapheme(
    letters: &[char],
    index: usize,
    silent_e_at: Option<usize>,
    body: usize,
) -> Option<(usize, &'static [&'static str])> {
    longest_of(GRAPHEMES, letters, index, silent_e_at, body)
}

/// A vowel + `r` spelling, but only when the `r` closes the syllable.
fn match_r_controlled(
    letters: &[char],
    index: usize,
    silent_e_at: Option<usize>,
    body: usize,
) -> Option<(usize, &'static [&'static str])> {
    let (length, phones) = longest_of(R_CONTROLLED, letters, index, silent_e_at, body)?;
    let follows = letters.get(index + length).copied();
    match follows {
        // A vowel after the `r` means the `r` belongs to the next syllable: "hero", "very".
        Some(next) if is_vowel_letter(next) || next == 'y' => None,
        _ => Some((length, phones)),
    }
}

fn longest_of(
    table: &'static [(&'static str, &'static [&'static str])],
    letters: &[char],
    index: usize,
    silent_e_at: Option<usize>,
    body: usize,
) -> Option<(usize, &'static [&'static str])> {
    let mut best: Option<(usize, &'static [&'static str])> = None;
    for &(spelling, phones) in table {
        let length = spelling.chars().count();
        let end = index + length;
        if end > body {
            continue;
        }
        // A silent `e` is not available to a spelling that does not itself end in one — otherwise
        // "judge" could never match `dge`, which is the only place that rule applies.
        if silent_e_at.is_some_and(|at| index <= at && at < end) && !spelling.ends_with('e') {
            continue;
        }
        let matches = letters
            .get(index..end)
            .is_some_and(|run| run.iter().zip(spelling.chars()).all(|(&a, b)| a == b));
        if matches && best.is_none_or(|(best_length, _)| length > best_length) {
            best = Some((length, phones));
        }
    }
    best
}

/// The sound of a single letter, given what surrounds it.
#[allow(clippy::match_same_arms)] // one arm per letter reads as the table it is; merging loses that
fn single_letter(letters: &[char], index: usize, letter: char) -> Option<&'static [&'static str]> {
    let next = letters.get(index + 1).copied();
    let soft = matches!(next, Some('e' | 'i' | 'y'));
    Some(match letter {
        'a' | 'e' | 'i' | 'o' | 'u' => short_vowel(letter)?,
        'b' => &["B"],
        'c' if soft => &["S"],
        'c' => &["K"],
        'd' => &["D"],
        'f' => &["F"],
        'g' if soft => &["JH"],
        'g' => &["G"],
        'h' => &["HH"],
        'j' => &["JH"],
        'k' => &["K"],
        'l' => &["L"],
        'm' => &["M"],
        'n' => &["N"],
        'p' => &["P"],
        'q' => &["K"],
        'r' => &["R"],
        's' => &["S"],
        't' => &["T"],
        'v' => &["V"],
        'w' => &["W"],
        'x' if index == 0 => &["Z"],
        'x' => &["K", "S"],
        'y' => y_sound(letters, index),
        'z' => &["Z"],
        _ => return None,
    })
}

/// `y` is a consonant at the start of a word and a vowel elsewhere.
fn y_sound(letters: &[char], index: usize) -> &'static [&'static str] {
    if index == 0 {
        return &["Y"];
    }
    if index + 1 != letters.len() {
        return &["IH"];
    }
    // Word-final: "my" says AY, "happy" says IY. Length is a decent proxy for which.
    if letters.len() <= 3 { &["AY"] } else { &["IY"] }
}

/// Suffixes whose stress lands on the syllable before them rather than on the first.
///
/// Measured against the dictionary: first-syllable stress is right for 6% of `-tion` words and
/// 27% of `-ic` words, against 94% and 93% for the penultimate syllable.
fn takes_penultimate_stress(letters: &[char]) -> bool {
    let word: String = letters.iter().collect();
    [
        "tion", "sion", "cion", "ical", "ic", "ity", "ify", "ious", "eous",
    ]
    .iter()
    .any(|suffix| word.ends_with(suffix))
}

/// Attaches guessed stress, then reduces the unstressed vowels.
///
/// Returns `None` if nothing in `symbols` can be a nucleus.
fn stressed(letters: &[char], symbols: &[&'static str]) -> Option<Vec<(Phoneme, Stress)>> {
    let nuclei: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, s)| is_vowel_symbol(s))
        .map(|(i, _)| i)
        .collect();
    if nuclei.is_empty() {
        return None;
    }

    let stressed_syllable = if takes_penultimate_stress(letters) && nuclei.len() >= 2 {
        nuclei.len() - 2
    } else {
        0
    };
    let stressed_at = nuclei.get(stressed_syllable).copied();

    let mut phones = Vec::with_capacity(symbols.len());
    for (position, &symbol) in symbols.iter().enumerate() {
        let is_nucleus = is_vowel_symbol(symbol);
        let stress = if is_nucleus && Some(position) == stressed_at {
            Stress::Primary
        } else {
            Stress::Unstressed
        };
        // English reduces unstressed vowels towards schwa. Without this, every polysyllable keeps
        // full spelling-derived vowels and reads as over-articulated: "abandon" as `AE B AE N D
        // AA N` rather than `AH B AE N D AH N`.
        let symbol = if is_nucleus && stress == Stress::Unstressed {
            reduce(symbol)
        } else {
            symbol
        };
        phones.push((ARPABET.phoneme(symbol)?, stress));
    }
    Some(phones)
}

/// An unstressed vowel's reduced value.
///
/// Only the plain monophthongs reduce. Diphthongs keep their shape, and `IH`, `IY`, `UW` and `ER`
/// are already what unstressed syllables commonly hold in `CMUdict`.
fn reduce(symbol: &'static str) -> &'static str {
    match symbol {
        "AE" | "EH" | "AA" | "AO" | "UH" => "AH",
        other => other,
    }
}

fn is_vowel_symbol(symbol: &str) -> bool {
    ARPABET.phoneme(symbol).is_some_and(Phoneme::is_vowel)
}

#[cfg(test)]
mod tests {
    use prosody_core::{
        Fallback, Phoneme, Phonemiser, Pronunciation, Provenance, Stress, SyllabifyError,
        Unpronounceable, phoneme,
    };

    use super::{LetterToSound, transcribe};

    fn say(word: &str) -> Vec<&'static str> {
        LetterToSound::new()
            .pronounce(word)
            .unwrap()
            .phonemes()
            .map(Phoneme::symbol)
            .collect()
    }

    #[test]
    fn everything_it_produces_is_marked_a_guess() {
        let word = LetterToSound::new().pronounce("krapplesnaffle").unwrap();
        assert_eq!(word.provenance(), Provenance::LetterToSound);
        assert!(word.provenance().is_guess());
        assert!(!Provenance::Dictionary.is_guess());
    }

    #[test]
    fn simple_consonant_vowel_consonant() {
        assert_eq!(say("cat"), ["K", "AE", "T"]);
        assert_eq!(say("bed"), ["B", "EH", "D"]);
        assert_eq!(say("sit"), ["S", "IH", "T"]);
        assert_eq!(say("hot"), ["HH", "AA", "T"]);
        assert_eq!(say("bug"), ["B", "AH", "G"]);
    }

    #[test]
    fn a_final_e_is_silent_and_lengthens_the_vowel() {
        assert_eq!(say("make"), ["M", "EY", "K"]);
        assert_eq!(say("bite"), ["B", "AY", "T"]);
        assert_eq!(say("note"), ["N", "OW", "T"]);
        assert_eq!(say("cube"), ["K", "Y", "UW", "B"]);
    }

    #[test]
    fn the_glide_of_a_long_u_drops_after_coronals_and_liquids() {
        // "cube" keeps it; these do not. Untested in the previous iteration, and deleting the
        // whole rule passed the suite.
        assert_eq!(say("rule"), ["R", "UW", "L"]);
        assert_eq!(say("dune"), ["D", "UW", "N"]);
        assert_eq!(say("tune"), ["T", "UW", "N"]);
        assert_eq!(say("chute"), ["CH", "UW", "T"]);
        // ...but `h` alone does not drop it: "huge" is not "hoodge".
        assert_eq!(say("huge"), ["HH", "Y", "UW", "JH"]);
    }

    #[test]
    fn an_inflected_form_keeps_its_stems_silent_e() {
        // The suffix-blind version got every past tense and plural wrong: "hoped" as
        // `HH AA P EH D`.
        assert_eq!(say("hoped"), ["HH", "OW", "P", "D"]);
        assert_eq!(say("smiled"), ["S", "M", "AY", "L", "D"]);
        assert_eq!(say("makes"), ["M", "EY", "K", "S"]);
        assert_eq!(say("hopes"), ["HH", "OW", "P", "S"]);
    }

    #[test]
    fn digraphs_are_one_sound() {
        assert_eq!(say("ship"), ["SH", "IH", "P"]);
        assert_eq!(say("chat"), ["CH", "AE", "T"]);
        assert_eq!(say("thin"), ["TH", "IH", "N"]);
        assert_eq!(say("phone"), ["F", "OW", "N"]);
        assert_eq!(say("king"), ["K", "IH", "NG"]);
        assert_eq!(say("duck"), ["D", "AH", "K"]);
        assert_eq!(say("queen"), ["K", "W", "IY", "N"]);
        assert_eq!(say("night"), ["N", "AY", "T"]);
    }

    #[test]
    fn dge_is_a_single_affricate() {
        // The rule existed but could never fire: the silent-final-e limit excluded any spelling
        // that itself ends in `e`, so "judge" produced a spurious `D`.
        assert_eq!(say("judge"), ["JH", "AH", "JH"]);
        assert_eq!(say("edge"), ["EH", "JH"]);
        assert_eq!(say("bridge"), ["B", "R", "IH", "JH"]);
    }

    #[test]
    fn th_is_voiced_in_function_words_and_between_vowels() {
        assert_eq!(say("the"), ["DH", "AH"]);
        assert_eq!(say("this"), ["DH", "IH", "S"]);
        assert_eq!(say("mother"), ["M", "AA", "DH", "ER"]);
        // Still unvoiced elsewhere.
        assert_eq!(say("thin"), ["TH", "IH", "N"]);
        assert_eq!(say("path"), ["P", "AE", "TH"]);
    }

    #[test]
    fn gh_is_silent_after_a_vowel_and_hard_at_the_start() {
        // The `ou` here should be `OW`, which spelling cannot tell us — but the `gh` must go.
        let though = say("though");
        assert!(!though.contains(&"G"), "{though:?}");
        assert!(!though.contains(&"HH"), "{though:?}");
        let ghost = say("ghost");
        assert_eq!(ghost.first(), Some(&"G"), "{ghost:?}");
        assert!(!ghost.contains(&"HH"), "{ghost:?}");
    }

    #[test]
    fn silent_letters_open_a_word() {
        assert_eq!(say("knight"), ["N", "AY", "T"]);
        assert_eq!(say("write"), ["R", "AY", "T"]);
        assert_eq!(say("gnome"), ["N", "OW", "M"]);
    }

    #[test]
    fn r_controlled_vowels_need_the_r_to_close_the_syllable() {
        assert_eq!(say("burn"), ["B", "ER", "N"]);
        assert_eq!(say("bird"), ["B", "ER", "D"]);
        assert_eq!(say("car"), ["K", "AA", "R"]);
        assert_eq!(say("for"), ["F", "AO", "R"]);
        // A vowel after the `r` means it opens the next syllable instead.
        assert_eq!(say("hero"), ["HH", "EH", "R", "OW"]);
        assert_eq!(say("very"), ["V", "EH", "R", "IY"]);
    }

    #[test]
    fn a_consonant_plus_le_is_its_own_syllable() {
        assert_eq!(say("table"), ["T", "EY", "B", "AH", "L"]);
        assert_eq!(say("apple"), ["AE", "P", "AH", "L"]);
        assert_eq!(say("little"), ["L", "IH", "T", "AH", "L"]);
    }

    #[test]
    fn c_and_g_soften_before_front_vowels() {
        assert_eq!(say("cat"), ["K", "AE", "T"]);
        assert_eq!(say("city"), ["S", "IH", "T", "IY"]);
        assert_eq!(say("got"), ["G", "AA", "T"]);
        assert_eq!(say("gin"), ["JH", "IH", "N"]);
    }

    #[test]
    fn a_doubled_c_before_a_front_vowel_is_two_sounds() {
        // "accent" is not "assent": the two `c`s differ, so this is not a geminate.
        assert_eq!(say("accent"), ["AE", "K", "S", "AH", "N", "T"]);
        // ...but an ordinary double still collapses.
        assert_eq!(say("account"), ["AE", "K", "AW", "N", "T"]);
    }

    #[test]
    fn y_is_a_consonant_at_the_start_a_vowel_in_the_middle_and_at_the_end() {
        assert_eq!(say("yes"), ["Y", "EH", "S"]);
        assert_eq!(say("my"), ["M", "AY"]);
        assert_eq!(say("happy"), ["HH", "AE", "P", "IY"]);
        // The mid-word branch was untested; mutating it passed the whole suite.
        assert_eq!(say("gym"), ["JH", "IH", "M"]);
        assert_eq!(say("myth"), ["M", "IH", "TH"]);
    }

    #[test]
    fn a_short_word_ending_in_a_vowel_says_its_name() {
        assert_eq!(say("he"), ["HH", "IY"]);
        assert_eq!(say("she"), ["SH", "IY"]);
        assert_eq!(say("go"), ["G", "OW"]);
        assert_eq!(say("so"), ["S", "OW"]);
    }

    #[test]
    fn doubled_consonants_are_one_sound() {
        assert_eq!(say("runner"), ["R", "AH", "N", "ER"]);
        assert_eq!(say("bitter"), ["B", "IH", "T", "ER"]);
    }

    #[test]
    fn unstressed_vowels_reduce_towards_schwa() {
        // Without reduction, every polysyllable keeps its full spelling vowels and reads as
        // over-articulated. This was the single largest source of error before.
        // Stress lands on the first syllable here, which is the documented guess and is wrong
        // for this word — but the later vowels do reduce, which is the point.
        assert_eq!(say("abandon"), ["AE", "B", "AH", "N", "D", "AH", "N"]);
        // The final `a` reduces; the first vowel keeps its spelling value, which is the known
        // limit of deriving quality from spelling alone.
        assert_eq!(say("sofa"), ["S", "AA", "F", "AH"]);
    }

    #[test]
    fn apostrophes_and_hyphens_join_rather_than_sound() {
        assert_eq!(say("burn'd"), say("burnd"));
        assert_eq!(say("half-light"), say("halflight"));
        assert_eq!(say("burn\u{2019}d"), say("burnd"));
    }

    #[test]
    fn the_invented_word_that_motivates_this_module() {
        // SPEC.md §2: typing "krapplesnaffle" and having it become sound.
        let word = LetterToSound::new().pronounce("krapplesnaffle").unwrap();
        assert_eq!(word.provenance(), Provenance::LetterToSound);
        assert!(word.syllable_count() >= 3);
    }

    #[test]
    fn stress_falls_on_the_first_syllable_by_default() {
        let word = LetterToSound::new().pronounce("krapplesnaffle").unwrap();
        let pattern = word.stress_pattern();
        assert_eq!(pattern.first(), Some(&Stress::Primary));
        assert!(pattern.iter().skip(1).all(|s| *s == Stress::Unstressed));
    }

    #[test]
    fn some_suffixes_pull_the_stress_to_the_penultimate_syllable() {
        // Measured: first-syllable stress is right for 6% of `-tion` words, penultimate for 94%.
        let longer = LetterToSound::new().pronounce("celebration").unwrap();
        let pattern = longer.stress_pattern();
        assert_ne!(
            pattern.iter().position(|s| *s == Stress::Primary),
            Some(0),
            "a -tion word must not take first-syllable stress: {pattern:?}"
        );
    }

    #[test]
    fn diacritics_fold_towards_the_nearest_english_spelling() {
        // SPEC.md §9's Slovenian case: not correct Slovenian, but audible and visibly guessed
        // rather than silent. `č` folds to `ch`, not to a bare `c`, because that is far closer.
        assert_eq!(say("gorči"), say("gorchi"));
        assert_eq!(say("šum"), say("shum"));
        assert_eq!(say("über"), say("uber"));
        assert_eq!(say("café"), say("cafe"));
        // German ß was missing entirely, so the word returned silence.
        assert_eq!(say("straße"), say("strasse"));
    }

    #[test]
    fn a_word_with_no_letter_at_all_is_unknown() {
        let lts = LetterToSound::new();
        assert_eq!(lts.pronounce(""), Err(Unpronounceable::NotKnown));
        assert_eq!(lts.pronounce("'"), Err(Unpronounceable::NotKnown));
        // Non-Latin scripts have no rules here, and a wrong guess is worse than none.
        assert_eq!(lts.pronounce("漢字"), Err(Unpronounceable::NotKnown));
        assert_eq!(lts.pronounce("🔥"), Err(Unpronounceable::NotKnown));
    }

    #[test]
    fn letters_that_yield_no_vowel_are_unsyllabifiable_not_unknown() {
        // The distinction M2a introduced: the guesser has rules for every letter of "brrr", it
        // just produces no nucleus — exactly `CMUdict`'s situation with "hm".
        assert_eq!(
            LetterToSound::new().pronounce("brrr"),
            Err(Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus))
        );
    }

    #[test]
    fn guessing_is_deterministic() {
        let lts = LetterToSound::new();
        for word in ["krapplesnaffle", "gorči", "burn", "Ozymandias"] {
            assert_eq!(lts.pronounce(word), lts.pronounce(word));
        }
    }

    #[test]
    fn case_does_not_matter() {
        assert_eq!(say("Burn"), say("burn"));
        assert_eq!(say("BURN"), say("burn"));
    }

    #[test]
    fn grapheme_matching_takes_the_longest_rule_not_the_first() {
        // Declaration order used to decide, so a shorter rule listed earlier silently shadowed a
        // longer one. `tch` must beat `ch`; `dge` must beat the single letters.
        assert_eq!(say("catch"), ["K", "AE", "CH"]);
        assert_eq!(say("match"), ["M", "AE", "CH"]);
        assert_eq!(transcribe(&"igh".chars().collect::<Vec<_>>()), ["AY"]);
    }

    /// A phonemiser reporting a different inventory, so the refusal path can be tested.
    #[derive(Debug, Clone, Copy)]
    struct Foreign;

    static OTHER_INVENTORY: prosody_core::Inventory = prosody_core::Inventory {
        name: "test-foreign",
        symbols: &[prosody_core::phoneme::Symbol {
            text: "AA",
            class: prosody_core::PhonemeClass::Vowel,
        }],
        legal_onset: |_| true,
    };

    impl Phonemiser for Foreign {
        fn inventory(&self) -> &'static prosody_core::Inventory {
            &OTHER_INVENTORY
        }

        fn pronounce(&self, _: &str) -> Result<Pronunciation, Unpronounceable> {
            Err(Unpronounceable::NotKnown)
        }
    }

    /// A phonemiser that knows exactly one word, for testing composition.
    #[derive(Debug, Clone, Copy)]
    struct OneWord;

    impl Phonemiser for OneWord {
        fn inventory(&self) -> &'static prosody_core::Inventory {
            &super::ARPABET
        }

        fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
            match word {
                "burn" => {
                    let phones = super::stressed(&['b'], &["B", "ER", "N"]).unwrap();
                    Ok(Pronunciation::from_dictionary(
                        phoneme::syllabify(&phones).unwrap(),
                    ))
                }
                "hm" => Err(Unpronounceable::Unsyllabifiable(SyllabifyError::NoNucleus)),
                _ => Err(Unpronounceable::NotKnown),
            }
        }
    }

    #[test]
    fn fallback_prefers_the_primary() {
        let chain = Fallback::new(OneWord, LetterToSound::new()).unwrap();
        assert_eq!(
            chain.pronounce("burn").unwrap().provenance(),
            Provenance::Dictionary
        );
    }

    #[test]
    fn fallback_guesses_only_for_unknown_words() {
        let chain = Fallback::new(OneWord, LetterToSound::new()).unwrap();
        assert_eq!(
            chain.pronounce("krapplesnaffle").unwrap().provenance(),
            Provenance::LetterToSound
        );
        // A known word with no syllable must NOT be guessed at.
        assert!(matches!(
            chain.pronounce("hm"),
            Err(Unpronounceable::Unsyllabifiable(_))
        ));
    }

    #[test]
    fn fallback_refuses_to_mix_inventories() {
        // The previous version of this test only asserted the compatible case, so deleting the
        // check entirely passed the suite.
        assert!(Fallback::new(OneWord, LetterToSound::new()).is_some());
        assert!(Fallback::new(Foreign, LetterToSound::new()).is_none());
        assert!(Fallback::new(LetterToSound::new(), Foreign).is_none());
    }

    #[test]
    fn the_chain_reports_the_primarys_inventory() {
        let chain = Fallback::new(OneWord, LetterToSound::new()).unwrap();
        assert_eq!(chain.inventory().name, "arpabet");
    }
}
