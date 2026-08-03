//! The `ARPAbet` phoneme inventory, as `CMUdict` uses it.
//!
//! This module is the only place in the system that names an `ARPAbet` symbol. Everything else goes
//! through [`Inventory`] and [`Phoneme`], so that adding `eSpeak-NG` or a
//! per-language dictionary with a different inventory (`SPEC.md` §9) touches nothing outside a
//! sibling module.

use super::{Inventory, Phoneme, PhonemeClass, Symbol};

/// Shorthand for a vowel entry.
const fn vowel(text: &'static str) -> Symbol {
    Symbol {
        text,
        class: PhonemeClass::Vowel,
    }
}

/// Shorthand for a consonant entry.
const fn consonant(text: &'static str) -> Symbol {
    Symbol {
        text,
        class: PhonemeClass::Consonant,
    }
}

/// The 39 `ARPAbet` phonemes.
///
/// `ER` is counted as a vowel: it is r-coloured and does carry stress in `CMUdict`, so treating it
/// as a consonant would lose a syllable from every word like "burn" or "her".
static SYMBOLS: &[Symbol] = &[
    // Vowels.
    vowel("AA"),
    vowel("AE"),
    vowel("AH"),
    vowel("AO"),
    vowel("AW"),
    vowel("AY"),
    vowel("EH"),
    vowel("ER"),
    vowel("EY"),
    vowel("IH"),
    vowel("IY"),
    vowel("OW"),
    vowel("OY"),
    vowel("UH"),
    vowel("UW"),
    // Consonants.
    consonant("B"),
    consonant("CH"),
    consonant("D"),
    consonant("DH"),
    consonant("F"),
    consonant("G"),
    consonant("HH"),
    consonant("JH"),
    consonant("K"),
    consonant("L"),
    consonant("M"),
    consonant("N"),
    consonant("NG"),
    consonant("P"),
    consonant("R"),
    consonant("S"),
    consonant("SH"),
    consonant("T"),
    consonant("TH"),
    consonant("V"),
    consonant("W"),
    consonant("Y"),
    consonant("Z"),
    consonant("ZH"),
];

/// The `ARPAbet` inventory.
pub static ARPABET: Inventory = Inventory {
    name: "arpabet",
    symbols: SYMBOLS,
    legal_onset,
};

/// Whether `cluster` may begin an English syllable.
///
/// The phonotactics that syllabification needs. Deliberately a table of what English actually
/// allows rather than "any consonants": without it, maximal onset would put the whole of `K S T R`
/// at the front of the second syllable of "extra", and every derived syllable count and stress
/// position after that would be wrong.
fn legal_onset(cluster: &[Phoneme]) -> bool {
    match cluster {
        [] => true,
        // `NG` never begins an English syllable; `ZH` only does in loanwords.
        [single] => !matches!(single.symbol(), "NG" | "ZH"),
        [first, second] => is_legal_pair(first.symbol(), second.symbol()),
        // Three-consonant onsets in English are all /s/ + voiceless stop + liquid or glide.
        [s, stop, liquid] => {
            s.symbol() == "S"
                && matches!(stop.symbol(), "P" | "T" | "K")
                && matches!(liquid.symbol(), "L" | "R" | "W" | "Y")
        }
        _ => false,
    }
}

#[allow(clippy::match_same_arms)] // the arms are distinct phonotactic classes; merging them would lose why each is legal
fn is_legal_pair(first: &str, second: &str) -> bool {
    match (first, second) {
        // Stop or voiceless fricative followed by a liquid: "play", "brick", "clay", "grow",
        // "flee", "free".
        ("P" | "B" | "K" | "G" | "F", "L" | "R") => true,
        // Coronal stop or dental fricative followed by /r/: "tree", "dream", "three".
        ("T" | "D" | "TH", "R") => true,
        // "shrimp", "shred".
        ("SH", "R") => true,
        // Consonant followed by /w/: "twin", "dwell", "quick", "penguin", "swim", "thwart",
        // "which".
        ("T" | "D" | "K" | "G" | "S" | "TH" | "HH", "W") => true,
        // Consonant followed by /j/: "cute", "few", "music", "huge". Attested in CMUdict for the
        // coronals too, in loanwords and proper nouns ("actuator", "australia", "abkhazia").
        (
            "P" | "B" | "T" | "D" | "K" | "G" | "F" | "V" | "M" | "N" | "HH" | "S" | "Z" | "TH"
            | "L",
            "Y",
        ) => true,
        // /s/ + a voiceless stop, nasal, liquid or /f/: "spin", "stop", "sky", "smoke", "snow",
        // "sleep", "sphere".
        ("S", "P" | "T" | "K" | "M" | "N" | "L" | "F") => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{ARPABET, legal_onset};
    use crate::phoneme::PhonemeClass;

    #[test]
    fn the_inventory_has_thirty_nine_distinct_symbols() {
        assert_eq!(ARPABET.symbols.len(), 39);
        let mut seen: Vec<&str> = ARPABET.symbols.iter().map(|s| s.text).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), count, "duplicate symbol in the inventory");
    }

    #[test]
    fn fifteen_vowels_and_twenty_four_consonants() {
        let vowels = ARPABET
            .symbols
            .iter()
            .filter(|s| s.class == PhonemeClass::Vowel)
            .count();
        assert_eq!(vowels, 15);
        assert_eq!(ARPABET.symbols.len() - vowels, 24);
    }

    /// Turns symbols into phonemes so the table can be probed directly.
    fn cluster(symbols: &[&str]) -> Vec<super::Phoneme> {
        symbols
            .iter()
            .map(|s| ARPABET.phoneme(s).unwrap())
            .collect()
    }

    fn legal(symbols: &[&str]) -> bool {
        legal_onset(&cluster(symbols))
    }

    // Every *symbol* listed in a match arm gets an example, not merely every arm. Testing one
    // representative per arm let a deleted symbol (`F` from the stop+liquid group, breaking
    // "flee" and "free") pass the entire suite.

    #[test]
    fn stop_or_fricative_plus_liquid_onsets() {
        for (first, second, word) in [
            ("P", "L", "play"),
            ("P", "R", "pray"),
            ("B", "L", "blue"),
            ("B", "R", "bring"),
            ("K", "L", "clay"),
            ("K", "R", "crow"),
            ("G", "L", "glow"),
            ("G", "R", "grow"),
            ("F", "L", "flee"),
            ("F", "R", "free"),
            ("T", "R", "tree"),
            ("D", "R", "dream"),
            ("TH", "R", "three"),
            ("SH", "R", "shrimp"),
        ] {
            assert!(legal(&[first, second]), "{first} {second} as in {word:?}");
        }
    }

    #[test]
    fn consonant_plus_glide_onsets() {
        for (first, second, word) in [
            ("T", "W", "twin"),
            ("D", "W", "dwell"),
            ("K", "W", "quick"),
            ("G", "W", "penguin"),
            ("S", "W", "swim"),
            ("TH", "W", "thwart"),
            ("HH", "W", "which"),
            ("P", "Y", "puny"),
            ("B", "Y", "beauty"),
            ("T", "Y", "actuator"),
            ("D", "Y", "dune"),
            ("K", "Y", "cute"),
            ("G", "Y", "figure"),
            ("F", "Y", "few"),
            ("V", "Y", "view"),
            ("M", "Y", "music"),
            ("N", "Y", "news"),
            ("HH", "Y", "huge"),
            ("S", "Y", "consume"),
            ("Z", "Y", "azure"),
            ("TH", "Y", "enthusiast"),
            ("L", "Y", "australia"),
        ] {
            assert!(legal(&[first, second]), "{first} {second} as in {word:?}");
        }
    }

    #[test]
    fn s_plus_consonant_onsets() {
        for (second, word) in [
            ("P", "spin"),
            ("T", "stop"),
            ("K", "sky"),
            ("M", "smoke"),
            ("N", "snow"),
            ("L", "sleep"),
            ("F", "sphere"),
        ] {
            assert!(legal(&["S", second]), "S {second} as in {word:?}");
        }
    }

    #[test]
    fn three_consonant_onsets() {
        for (stop, liquid, word) in [
            ("P", "L", "splash"),
            ("P", "R", "spring"),
            ("T", "R", "string"),
            ("K", "L", "sclerosis"),
            ("K", "R", "scream"),
            ("K", "W", "squint"),
            ("P", "Y", "spurious"),
        ] {
            assert!(legal(&["S", stop, liquid]), "S {stop} {liquid} in {word:?}");
        }
    }

    #[test]
    fn every_single_consonant_may_open_a_syllable_except_ng_and_zh() {
        for symbol in ARPABET
            .symbols
            .iter()
            .filter(|s| s.class != PhonemeClass::Vowel)
        {
            let expected = !matches!(symbol.text, "NG" | "ZH");
            assert_eq!(legal(&[symbol.text]), expected, "single {}", symbol.text);
        }
    }

    #[test]
    fn illegal_onsets_english_forbids() {
        for (symbols, why) in [
            (&["K", "S", "T", "R"][..], "four consonants"),
            (&["P", "K"], "two stops"),
            (&["N", "G"], "nasal then stop"),
            (&["R", "T"], "liquid then stop"),
            (&["L", "B"], "liquid then stop"),
            (&["V", "R"], "voiced fricative then liquid"),
            (&["S", "R"], "/s/ then /r/ is not English"),
            (&["T", "L"], "coronal stop then /l/"),
            (&["D", "L"], "coronal stop then /l/"),
            (&["S", "T", "M"], "three consonants ending in a nasal"),
            (&["S", "B", "R"], "three consonants with a voiced stop"),
            (&["D", "HH"], "only legal word-initially, in loanwords"),
        ] {
            assert!(!legal(symbols), "{symbols:?} should be illegal: {why}");
        }
    }

    #[test]
    fn the_empty_onset_is_always_legal() {
        assert!(legal(&[]));
    }
}
