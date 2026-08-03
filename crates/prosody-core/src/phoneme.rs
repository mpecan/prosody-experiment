//! The sound of a word: phonemes, syllables, stress.
//!
//! Everything downstream of here — meter (`SPEC.md` §4.2), rhyme (§4.5), alliteration and
//! morphological near-repetition (§4.3) — reads this module's types rather than any particular
//! dictionary's format.
//!
//! # Why the inventory is a value and not an enum
//!
//! `CMUdict` is English-only, and `SPEC.md` §9 is explicit that a second phonemiser with a
//! *different phoneme inventory* is the expected future, and that retrofitting the abstraction
//! later is grim. So a [`Phoneme`] is not an `ARPAbet` variant; it is an index into an
//! [`Inventory`] that says what the symbols are and which of them are vowels. `ARPAbet` is one
//! inventory, defined in [`arpabet`], and nothing outside that module names an `ARPAbet` symbol.
//!
//! Two phonemes from different inventories are never equal, so a comparison that silently spans
//! inventories yields "different" rather than a plausible-looking match.

use core::fmt;

pub mod arpabet;

/// Whether a phoneme can be a syllable nucleus.
///
/// The distinction the analysis passes actually need: vowels carry stress and count as
/// subdivisions (`SPEC.md` §4.2), consonants form onsets and codas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhonemeClass {
    /// A vowel — can be a syllable nucleus and carries stress.
    Vowel,
    /// A consonant.
    Consonant,
}

/// One symbol of an inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symbol {
    /// The symbol as the dictionary writes it, e.g. `"AA"` or `"NG"`.
    pub text: &'static str,
    /// Whether it can be a syllable nucleus.
    pub class: PhonemeClass,
}

/// A set of phonemes, and the rules for combining them.
///
/// `legal_onset` is the phonotactics needed for syllabification. It belongs to the inventory
/// rather than being a free function because what may begin a syllable is a fact about a
/// language, not about phonemes in general.
///
/// # Names must be unique
///
/// [`Phoneme`] equality and ordering compare `name`, so two inventories sharing a name would make
/// their phonemes compare equal — the cross-inventory false match this module exists to prevent.
/// Ordering cannot instead use pointer identity: addresses vary between runs, and invariant I1
/// requires a sort order that does not. Debug builds assert that equal names imply the same
/// symbol table.
pub struct Inventory {
    /// A stable, unique identifier.
    pub name: &'static str,
    /// The symbols, in a fixed order. A [`Phoneme`] is an index into this.
    pub symbols: &'static [Symbol],
    /// Whether a run of consonants may begin a syllable in this language.
    ///
    /// Takes phonemes rather than symbol strings so that implementations need no projection and
    /// the caller needs no allocation per candidate cluster.
    pub legal_onset: fn(&[Phoneme]) -> bool,
}

impl Inventory {
    /// Looks up a symbol by its written form.
    #[must_use]
    pub fn phoneme(&'static self, text: &str) -> Option<Phoneme> {
        let index = self.symbols.iter().position(|s| s.text == text)?;
        u16::try_from(index).ok().map(|index| Phoneme {
            inventory: self,
            index,
        })
    }
}

impl fmt::Debug for Inventory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inventory")
            .field("name", &self.name)
            .field("symbols", &self.symbols.len())
            .finish_non_exhaustive()
    }
}

/// A single speech sound, within some [`Inventory`].
#[derive(Debug, Clone, Copy)]
pub struct Phoneme {
    inventory: &'static Inventory,
    index: u16,
}

impl Phoneme {
    /// The inventory this phoneme belongs to.
    #[must_use]
    pub const fn inventory(self) -> &'static Inventory {
        self.inventory
    }

    /// The symbol as the dictionary writes it.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        self.entry().map_or("", |s| s.text)
    }

    /// Whether this phoneme can be a syllable nucleus.
    #[must_use]
    pub fn class(self) -> PhonemeClass {
        self.entry().map_or(PhonemeClass::Consonant, |s| s.class)
    }

    /// Whether this phoneme is a vowel.
    #[must_use]
    pub fn is_vowel(self) -> bool {
        self.class() == PhonemeClass::Vowel
    }

    /// The symbol table entry.
    ///
    /// Only [`Inventory::phoneme`] can build a `Phoneme`, and it checks the index first, so the
    /// `None` case is unreachable. The assertion catches a future constructor that forgets to
    /// check, rather than letting it degrade into a silently wrong `""` or `Consonant`.
    fn entry(self) -> Option<&'static Symbol> {
        let entry = self.inventory.symbols.get(usize::from(self.index));
        debug_assert!(
            entry.is_some(),
            "phoneme index outside its inventory's symbol table"
        );
        entry
    }
}

/// Equality spans an inventory: the same index in two different inventories means two different
/// sounds, and must not compare equal.
impl PartialEq for Phoneme {
    fn eq(&self, other: &Self) -> bool {
        debug_assert!(
            self.inventory.name != other.inventory.name
                || core::ptr::eq(self.inventory.symbols, other.inventory.symbols),
            "two distinct inventories share a name"
        );
        self.index == other.index && self.inventory.name == other.inventory.name
    }
}

impl Eq for Phoneme {}

impl core::hash::Hash for Phoneme {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.inventory.name.hash(state);
        self.index.hash(state);
    }
}

/// Ordering is by inventory name then index, giving the total order `CLAUDE.md` §4 requires
/// before anything derived from phonemes reaches an emitter.
impl Ord for Phoneme {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.inventory
            .name
            .cmp(other.inventory.name)
            .then(self.index.cmp(&other.index))
    }
}

impl PartialOrd for Phoneme {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Phoneme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.symbol())
    }
}

/// How strongly a syllable is stressed.
///
/// Lexical stress, as a dictionary records it — not phrasal stress. `SPEC.md` §9 accepts the
/// resulting error rate ("content" the noun against "content" the adjective) rather than building
/// a part-of-speech tagger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stress {
    /// No stress.
    Unstressed,
    /// Secondary stress.
    Secondary,
    /// Primary stress.
    Primary,
}

impl Stress {
    /// Whether this syllable is stressed at all.
    ///
    /// Foot detection (`SPEC.md` §4.2) mostly wants the binary distinction; velocity mapping
    /// wants the three-way one.
    #[must_use]
    pub fn is_stressed(self) -> bool {
        self != Self::Unstressed
    }
}

/// One syllable: an optional onset, a vowel nucleus, an optional coda, and a stress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syllable {
    onset: Vec<Phoneme>,
    nucleus: Phoneme,
    coda: Vec<Phoneme>,
    stress: Stress,
}

impl Syllable {
    /// Builds a syllable.
    ///
    /// Returns `None` if `nucleus` is not a vowel, or if any onset or coda phoneme is — a
    /// syllable with a vowel in its coda is not a syllable, it is two.
    #[must_use]
    pub fn new(
        onset: Vec<Phoneme>,
        nucleus: Phoneme,
        coda: Vec<Phoneme>,
        stress: Stress,
    ) -> Option<Self> {
        let has_vowel = |ps: &[Phoneme]| ps.iter().any(|p| p.is_vowel());
        if !nucleus.is_vowel() || has_vowel(&onset) || has_vowel(&coda) {
            return None;
        }
        Some(Self {
            onset,
            nucleus,
            coda,
            stress,
        })
    }

    /// The consonants before the nucleus.
    ///
    /// Alliteration is an onset match (`SPEC.md` §4.3).
    #[must_use]
    pub fn onset(&self) -> &[Phoneme] {
        &self.onset
    }

    /// The vowel.
    ///
    /// Assonance is a nucleus match (`SPEC.md` §4.3).
    #[must_use]
    pub const fn nucleus(&self) -> Phoneme {
        self.nucleus
    }

    /// The consonants after the nucleus.
    #[must_use]
    pub fn coda(&self) -> &[Phoneme] {
        &self.coda
    }

    /// How strongly this syllable is stressed.
    #[must_use]
    pub const fn stress(&self) -> Stress {
        self.stress
    }

    /// The nucleus and coda together.
    ///
    /// This is the unit rhyme is scored on (`SPEC.md` §4.5).
    #[must_use]
    pub fn rime(&self) -> Vec<Phoneme> {
        let mut rime = Vec::with_capacity(1 + self.coda.len());
        rime.push(self.nucleus);
        rime.extend_from_slice(&self.coda);
        rime
    }

    /// Every phoneme, in order.
    pub fn phonemes(&self) -> impl Iterator<Item = Phoneme> + '_ {
        self.onset
            .iter()
            .copied()
            .chain(core::iter::once(self.nucleus))
            .chain(self.coda.iter().copied())
    }
}

/// Where a pronunciation came from.
///
/// `CLAUDE.md` §6 requires fallbacks to be visible: a guessed pronunciation must be
/// distinguishable from a looked-up one everywhere it travels, including in the debug view, or
/// the system quietly becomes the "text-shaped seed" failure of `SPEC.md` §1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provenance {
    /// Looked up in a pronunciation dictionary.
    Dictionary,
    /// Guessed from spelling because the word was not in the dictionary.
    LetterToSound,
}

impl Provenance {
    /// Whether this pronunciation was guessed rather than looked up.
    ///
    /// The predicate the debug view (`SPEC.md` §8) renders, and the one `CLAUDE.md` §6's
    /// visible-fallback rule turns on.
    #[must_use]
    pub fn is_guess(self) -> bool {
        self == Self::LetterToSound
    }
}

/// How a word sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pronunciation {
    syllables: Vec<Syllable>,
    provenance: Provenance,
}

impl Pronunciation {
    /// A pronunciation that was looked up.
    ///
    /// Named constructors rather than one taking a [`Provenance`] argument: a single constructor
    /// accepting either value makes `new(guessed, Provenance::Dictionary)` compile, and the whole
    /// point of tracking provenance is that such a claim should be hard to write by accident.
    #[must_use]
    pub const fn from_dictionary(syllables: Vec<Syllable>) -> Self {
        Self {
            syllables,
            provenance: Provenance::Dictionary,
        }
    }

    /// A pronunciation that was guessed from spelling.
    #[must_use]
    pub const fn guessed(syllables: Vec<Syllable>) -> Self {
        Self {
            syllables,
            provenance: Provenance::LetterToSound,
        }
    }

    /// The syllables, in order.
    #[must_use]
    pub fn syllables(&self) -> &[Syllable] {
        &self.syllables
    }

    /// Whether this was looked up or guessed.
    #[must_use]
    pub const fn provenance(&self) -> Provenance {
        self.provenance
    }

    /// How many syllables the word has — the subdivision count of `SPEC.md` §4.2.
    #[must_use]
    pub fn syllable_count(&self) -> usize {
        self.syllables.len()
    }

    /// The stress of each syllable, in order.
    #[must_use]
    pub fn stress_pattern(&self) -> Vec<Stress> {
        self.syllables.iter().map(Syllable::stress).collect()
    }

    /// Every phoneme, in order.
    pub fn phonemes(&self) -> impl Iterator<Item = Phoneme> + '_ {
        self.syllables.iter().flat_map(Syllable::phonemes)
    }

    /// The rhyming tail: everything from the last stressed syllable's nucleus onward.
    ///
    /// `SPEC.md` §4.5 wants rhyme scored continuously on nucleus and coda, and English rhyme is
    /// reckoned from the last stressed vowel — so "burning" rhymes with "turning" on
    /// `ER N IH NG`, not merely on the final `IH NG`.
    ///
    /// "Stressed" means primary *or* secondary. Restricting to primary would break real rhymes:
    /// "cavalcade" carries primary stress on its first syllable and only secondary on the rhyming
    /// final one, yet it rhymes with "brigade".
    ///
    /// Falls back to the last syllable when nothing is stressed, as for single-syllable function
    /// words.
    #[must_use]
    pub fn rhyme_tail(&self) -> Vec<Phoneme> {
        let from = self
            .syllables
            .iter()
            .rposition(|s| s.stress().is_stressed())
            .or_else(|| self.syllables.len().checked_sub(1));
        let Some(from) = from else {
            return Vec::new();
        };
        let mut tail = Vec::new();
        for (index, syllable) in self.syllables.iter().enumerate().skip(from) {
            if index == from {
                tail.extend(syllable.rime());
            } else {
                tail.extend(syllable.phonemes());
            }
        }
        tail
    }
}

/// Why a phone sequence could not be grouped into syllables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SyllabifyError {
    /// No vowel, so there is no nucleus to build a syllable on.
    ///
    /// Real dictionary entries do this — `hm`, `shh`, `mm`. They are words; they are just not
    /// words with syllables.
    #[error("no vowel to build a syllable on")]
    NoNucleus,
    /// The phonemes come from more than one inventory, so no single phonotactics applies.
    #[error("phonemes come from more than one inventory")]
    MixedInventories,
}

/// Why a word could not be pronounced.
///
/// Distinguishing these is what lets the debug view (`SPEC.md` §8) tell a writer *which* thing
/// happened. Collapsing them into a bare `None` makes "I have never heard of this word" and
/// "this word has no syllables" identical, and `CLAUDE.md` §6 requires the difference to be
/// visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Unpronounceable {
    /// The phonemiser has no entry for this word.
    #[error("no entry for this word")]
    NotKnown,
    /// There is an entry, but it does not group into syllables.
    #[error("entry cannot be syllabified: {0}")]
    Unsyllabifiable(#[from] SyllabifyError),
}

/// Turning a written word into sound.
///
/// The boundary `SPEC.md` §9 asks for. `CMUdict` is the first implementation; eSpeak-NG or a
/// per-language dictionary is the expected second, with a *different phoneme inventory* — which
/// is why [`Phonemiser::inventory`] exists and why callers must not assume `ARPAbet`.
pub trait Phonemiser {
    /// The inventory this phonemiser produces phonemes from.
    fn inventory(&self) -> &'static Inventory;

    /// How `word` sounds.
    ///
    /// `word` is a written token as it appears in the poem; implementations do their own
    /// normalisation. Must be deterministic: the same word always yields the same pronunciation,
    /// including when the source offers several variants (invariant I1).
    ///
    /// # Errors
    ///
    /// [`Unpronounceable`], distinguishing a word the phonemiser does not know from one it knows
    /// but cannot syllabify.
    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable>;
}

/// A phonemiser that consults `primary` and falls back to `fallback`.
///
/// The composition `SPEC.md` §7.2 asks for: a dictionary, then a guess. It falls back **only**
/// when the primary has no entry at all. A word the primary knows but cannot syllabify — `hm`,
/// `shh` — genuinely has no syllable, and guessing one would invent sound the poem does not
/// contain.
#[derive(Debug, Clone, Copy)]
pub struct Fallback<P, Q> {
    primary: P,
    fallback: Q,
}

impl<P: Phonemiser, Q: Phonemiser> Fallback<P, Q> {
    /// Composes two phonemisers, or `None` if they do not share an inventory.
    ///
    /// Mixing inventories would produce pronunciations that cannot be compared with one another,
    /// so rhyme and alliteration would silently stop matching across the boundary.
    #[must_use]
    pub fn new(primary: P, fallback: Q) -> Option<Self> {
        (primary.inventory().name == fallback.inventory().name)
            .then_some(Self { primary, fallback })
    }
}

impl<P: Phonemiser, Q: Phonemiser> Phonemiser for Fallback<P, Q> {
    fn inventory(&self) -> &'static Inventory {
        self.primary.inventory()
    }

    /// # Errors
    ///
    /// The primary's error when it knows the word, the fallback's when neither does.
    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        match self.primary.pronounce(word) {
            Err(Unpronounceable::NotKnown) => self.fallback.pronounce(word),
            other => other,
        }
    }
}

/// Groups a flat phone sequence into syllables by the maximal onset principle.
///
/// Each vowel is a nucleus. Consonants between two vowels go to the following syllable's onset as
/// far as the inventory's phonotactics allow, and the rest close the preceding syllable — so
/// "extra" is `EH K` + `S T R AH`, not `EH K S T R` + `AH`. Consonants before the first vowel
/// always form the first onset, and those after the last vowel always close the last syllable.
///
/// # A known limitation
///
/// Maximal onset ignores morpheme boundaries, so "bathroom" (`B AE TH R UW M`) syllabifies as
/// `B AE` + `TH R UW M` — "ba-throom" — because `TH R` is a legal onset, as in "three". Syllable
/// counts and rhyme tails are unaffected, but onset-based alliteration (`SPEC.md` §4.3) would
/// credit that syllable with a `TH R` onset. This cannot be fixed by adjusting the phonotactics,
/// since `TH R` genuinely is legal; it needs morphology. See `PLAN.md`.
///
/// # Errors
///
/// [`SyllabifyError`] if there is no vowel, or if the phonemes span several inventories.
pub fn syllabify(phones: &[(Phoneme, Stress)]) -> Result<Vec<Syllable>, SyllabifyError> {
    let symbols: Vec<Phoneme> = phones.iter().map(|(p, _)| *p).collect();
    let Some(first) = symbols.first() else {
        return Err(SyllabifyError::NoNucleus);
    };
    let inventory = first.inventory();
    if symbols.iter().any(|p| p.inventory().name != inventory.name) {
        return Err(SyllabifyError::MixedInventories);
    }

    let nuclei: Vec<usize> = symbols
        .iter()
        .enumerate()
        .filter(|(_, p)| p.is_vowel())
        .map(|(i, _)| i)
        .collect();
    if nuclei.is_empty() {
        return Err(SyllabifyError::NoNucleus);
    }

    let mut syllables = Vec::with_capacity(nuclei.len());
    for (position, &nucleus) in nuclei.iter().enumerate() {
        let previous = position.checked_sub(1).and_then(|i| nuclei.get(i));
        let onset_start = match previous {
            // Whatever precedes the first vowel must open the word: there is no earlier syllable
            // to give it to, so legality does not enter into it.
            None => 0,
            Some(&previous) => longest_legal_onset(inventory, &symbols, previous + 1, nucleus),
        };
        let coda_end = match nuclei.get(position + 1) {
            Some(&next) => longest_legal_onset(inventory, &symbols, nucleus + 1, next),
            None => symbols.len(),
        };

        let onset = symbols.get(onset_start..nucleus);
        let coda = symbols.get(nucleus + 1..coda_end);
        let centre = phones.get(nucleus);
        let (Some(onset), Some(coda), Some(&(phoneme, stress))) = (onset, coda, centre) else {
            // Unreachable: every index above derives from `nuclei`, which indexes `symbols`.
            debug_assert!(false, "syllable bounds fell outside the phone sequence");
            return Err(SyllabifyError::NoNucleus);
        };
        let Some(syllable) = Syllable::new(onset.to_vec(), phoneme, coda.to_vec(), stress) else {
            debug_assert!(false, "syllable built with a vowel outside its nucleus");
            return Err(SyllabifyError::NoNucleus);
        };
        syllables.push(syllable);
    }

    Ok(syllables)
}

/// The earliest index in `from..to` at which the consonants form a legal onset.
///
/// Maximal onset: try the longest run first and shorten until the inventory accepts it. The empty
/// cluster is legal in every inventory, so this terminates at `to` at worst.
fn longest_legal_onset(
    inventory: &'static Inventory,
    symbols: &[Phoneme],
    from: usize,
    to: usize,
) -> usize {
    for start in from..to {
        if let Some(run) = symbols.get(start..to)
            && (inventory.legal_onset)(run)
        {
            return start;
        }
    }
    to
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::arpabet::ARPABET;
    use super::{
        Inventory, Phoneme, PhonemeClass, Pronunciation, Provenance, Stress, SyllabifyError,
        Syllable, Symbol, syllabify,
    };

    /// A second inventory, for checking that phonemes never compare across inventories.
    static OTHER: Inventory = Inventory {
        name: "test-other",
        symbols: &[
            Symbol {
                text: "AA",
                class: PhonemeClass::Vowel,
            },
            Symbol {
                text: "B",
                class: PhonemeClass::Consonant,
            },
        ],
        legal_onset: |_| true,
    };

    /// Builds a phone sequence from `ARPAbet` symbols, with stress on the vowels given.
    fn phones(symbols: &[&str], stresses: &[Stress]) -> Vec<(Phoneme, Stress)> {
        let mut next = stresses.iter().copied();
        symbols
            .iter()
            .map(|s| {
                let phoneme = ARPABET.phoneme(s).unwrap();
                let stress = if phoneme.is_vowel() {
                    next.next().unwrap_or(Stress::Unstressed)
                } else {
                    Stress::Unstressed
                };
                (phoneme, stress)
            })
            .collect()
    }

    /// Renders syllables as `onset|nucleus|coda`, so an expected split reads like one.
    fn shape(syllables: &[Syllable]) -> Vec<String> {
        syllables
            .iter()
            .map(|s| {
                let join = |ps: &[Phoneme]| {
                    ps.iter()
                        .copied()
                        .map(Phoneme::symbol)
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                format!(
                    "{}|{}|{}",
                    join(s.onset()),
                    s.nucleus().symbol(),
                    join(s.coda())
                )
            })
            .collect()
    }

    fn split(symbols: &[&str], stresses: &[Stress]) -> Vec<String> {
        shape(&syllabify(&phones(symbols, stresses)).unwrap())
    }

    fn tail_of(word: &Pronunciation) -> Vec<&'static str> {
        word.rhyme_tail()
            .iter()
            .copied()
            .map(Phoneme::symbol)
            .collect()
    }

    fn hash_of(phoneme: Phoneme) -> u64 {
        let mut hasher = DefaultHasher::new();
        phoneme.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn phonemes_know_their_class() {
        assert!(ARPABET.phoneme("AA").unwrap().is_vowel());
        assert!(ARPABET.phoneme("ER").unwrap().is_vowel());
        assert!(!ARPABET.phoneme("NG").unwrap().is_vowel());
        assert_eq!(
            ARPABET.phoneme("B").unwrap().class(),
            PhonemeClass::Consonant
        );
        assert_eq!(ARPABET.phoneme("QQ"), None);
        assert_eq!(ARPABET.phoneme(""), None);
        assert_eq!(ARPABET.phoneme("aa"), None, "lookup is case-sensitive");
    }

    #[test]
    fn phonemes_from_different_inventories_are_never_equal() {
        let ours = ARPABET.phoneme("AA").unwrap();
        let theirs = OTHER.phoneme("AA").unwrap();
        assert_eq!(ours.symbol(), theirs.symbol());
        assert_ne!(ours, theirs);
    }

    #[test]
    fn ordering_separates_inventories_before_indices() {
        let ours = ARPABET.phoneme("AA").unwrap();
        let theirs = OTHER.phoneme("AA").unwrap();
        // Same index, different inventory: must not compare equal, and must order consistently.
        assert_ne!(ours.cmp(&theirs), core::cmp::Ordering::Equal);
        assert_eq!(ours.cmp(&theirs), "arpabet".cmp("test-other"));
        assert_eq!(theirs.cmp(&ours), "test-other".cmp("arpabet"));

        let aa = ARPABET.phoneme("AA").unwrap();
        let ae = ARPABET.phoneme("AE").unwrap();
        assert!(aa < ae);
        assert_eq!(aa.cmp(&aa), core::cmp::Ordering::Equal);
    }

    #[test]
    fn sorting_phonemes_is_a_total_order() {
        let mut all: Vec<Phoneme> = ARPABET
            .symbols
            .iter()
            .map(|s| ARPABET.phoneme(s.text).unwrap())
            .collect();
        all.reverse();
        all.sort_unstable();
        let symbols: Vec<&str> = all.iter().copied().map(Phoneme::symbol).collect();
        let expected: Vec<&str> = ARPABET.symbols.iter().map(|s| s.text).collect();
        assert_eq!(symbols, expected);
    }

    #[test]
    fn hashing_agrees_with_equality() {
        let aa = ARPABET.phoneme("AA").unwrap();
        let same = ARPABET.phoneme("AA").unwrap();
        let other_inventory = OTHER.phoneme("AA").unwrap();
        let different = ARPABET.phoneme("AE").unwrap();

        assert_eq!(hash_of(aa), hash_of(same));
        assert_ne!(hash_of(aa), hash_of(other_inventory));
        assert_ne!(hash_of(aa), hash_of(different));
    }

    // Hand-worked syllabifications. Each expectation is the standard English syllable break,
    // written from the phonotactics rather than read off the implementation. These are the tests
    // that catch a wrong phonotactics table — a corpus-wide legality check cannot, because it
    // validates the splitter against the very table under suspicion.

    #[test]
    fn a_single_syllable_word() {
        // burn = B ER1 N
        assert_eq!(split(&["B", "ER", "N"], &[Stress::Primary]), ["B|ER|N"]);
    }

    #[test]
    fn maximal_onset_moves_consonants_to_the_following_syllable() {
        // extra = EH1 K S T R AH0. "S T R" is a legal onset, "K S T R" is not.
        assert_eq!(
            split(
                &["EH", "K", "S", "T", "R", "AH"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["|EH|K", "S T R|AH|"]
        );
    }

    #[test]
    fn an_illegal_cluster_is_split() {
        // napkin = N AE1 P K IH0 N. "P K" cannot open a syllable.
        assert_eq!(
            split(
                &["N", "AE", "P", "K", "IH", "N"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["N|AE|P", "K|IH|N"]
        );
    }

    #[test]
    fn a_legal_pair_moves_whole() {
        // secret = S IY1 K R AH0 T. "K R" is legal, so both consonants move.
        assert_eq!(
            split(
                &["S", "IY", "K", "R", "AH", "T"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["S|IY|", "K R|AH|T"]
        );
    }

    #[test]
    fn an_illegal_pair_leaves_one_consonant_behind() {
        // average = AE1 V R AH0 JH. "V R" is not a legal English onset.
        assert_eq!(
            split(
                &["AE", "V", "R", "AH", "JH"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["|AE|V", "R|AH|JH"]
        );
    }

    #[test]
    fn a_single_intervocalic_consonant_opens_the_next_syllable() {
        // city = S IH1 T IY0
        assert_eq!(
            split(
                &["S", "IH", "T", "IY"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["S|IH|", "T|IY|"]
        );
    }

    #[test]
    fn a_three_consonant_run_splits_where_english_allows() {
        // monster = M AA1 N S T ER0. "N S T" is not a legal onset; "S T" is.
        assert_eq!(
            split(
                &["M", "AA", "N", "S", "T", "ER"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["M|AA|N", "S T|ER|"]
        );
    }

    #[test]
    fn consonants_before_the_first_vowel_all_open_the_word() {
        // strong = S T R AO1 NG
        assert_eq!(
            split(&["S", "T", "R", "AO", "NG"], &[Stress::Primary]),
            ["S T R|AO|NG"]
        );
    }

    #[test]
    fn a_word_initial_cluster_illegal_elsewhere_is_still_kept_whole() {
        // Loanword onsets the table rejects must not be broken at the start of a word: there is
        // no preceding syllable to hand them to.
        assert_eq!(
            split(&["D", "HH", "AA", "N", "T"], &[Stress::Primary]),
            ["D HH|AA|N T"]
        );
    }

    #[test]
    fn adjacent_vowels_are_separate_syllables() {
        // chaos = K EY1 AA0 S
        assert_eq!(
            split(
                &["K", "EY", "AA", "S"],
                &[Stress::Primary, Stress::Unstressed]
            ),
            ["K|EY|", "|AA|S"]
        );
    }

    #[test]
    fn morpheme_boundaries_are_ignored_by_design() {
        // bathroom = B AE1 TH R UW2 M. "TH R" is a legal onset (as in "three"), so maximal onset
        // takes it across the compound boundary: "ba-throom", not "bath-room". Recorded as a test
        // so the limitation is visible rather than discovered later as a surprise. See PLAN.md.
        assert_eq!(
            split(
                &["B", "AE", "TH", "R", "UW", "M"],
                &[Stress::Primary, Stress::Secondary]
            ),
            ["B|AE|", "TH R|UW|M"]
        );
    }

    #[test]
    fn a_word_with_no_vowel_has_no_syllables() {
        // Real dictionary entries: "shh", "hm".
        assert_eq!(
            syllabify(&phones(&["SH"], &[])),
            Err(SyllabifyError::NoNucleus)
        );
        assert_eq!(syllabify(&[]), Err(SyllabifyError::NoNucleus));
    }

    #[test]
    fn phonemes_from_several_inventories_are_refused() {
        let mixed = vec![
            (ARPABET.phoneme("B").unwrap(), Stress::Unstressed),
            (OTHER.phoneme("AA").unwrap(), Stress::Primary),
        ];
        assert_eq!(syllabify(&mixed), Err(SyllabifyError::MixedInventories));
    }

    #[test]
    fn a_syllable_needs_a_vowel_nucleus() {
        let b = ARPABET.phoneme("B").unwrap();
        let er = ARPABET.phoneme("ER").unwrap();
        assert!(Syllable::new(vec![], b, vec![], Stress::Primary).is_none());
        assert!(Syllable::new(vec![er], er, vec![], Stress::Primary).is_none());
        assert!(Syllable::new(vec![], er, vec![er], Stress::Primary).is_none());
        assert!(Syllable::new(vec![er], b, vec![er], Stress::Primary).is_none());
        assert!(Syllable::new(vec![b], er, vec![b], Stress::Primary).is_some());
    }

    #[test]
    fn rhyme_tail_starts_at_the_last_stressed_vowel() {
        // burning = B ER1 N IH0 NG -> ER N IH NG, not IH NG.
        let word = Pronunciation::from_dictionary(
            syllabify(&phones(
                &["B", "ER", "N", "IH", "NG"],
                &[Stress::Primary, Stress::Unstressed],
            ))
            .unwrap(),
        );
        assert_eq!(tail_of(&word), ["ER", "N", "IH", "NG"]);
    }

    #[test]
    fn rhyme_tail_counts_secondary_stress() {
        // cavalcade = K AE1 V AH0 L K EY2 D. The rhyming syllable carries only secondary stress;
        // restricting to primary would take the tail from the first syllable and lose the rhyme
        // with "brigade".
        let word = Pronunciation::from_dictionary(
            syllabify(&phones(
                &["K", "AE", "V", "AH", "L", "K", "EY", "D"],
                &[Stress::Primary, Stress::Unstressed, Stress::Secondary],
            ))
            .unwrap(),
        );
        assert_eq!(tail_of(&word), ["EY", "D"]);
    }

    #[test]
    fn rhyme_tail_of_an_unstressed_word_is_its_last_rime() {
        // the = DH AH0
        let word = Pronunciation::from_dictionary(
            syllabify(&phones(&["DH", "AH"], &[Stress::Unstressed])).unwrap(),
        );
        assert_eq!(tail_of(&word), ["AH"]);
    }

    #[test]
    fn an_empty_pronunciation_has_an_empty_tail() {
        let word = Pronunciation::from_dictionary(Vec::new());
        assert_eq!(word.syllable_count(), 0);
        assert!(word.rhyme_tail().is_empty());
        assert!(word.stress_pattern().is_empty());
        assert_eq!(word.phonemes().count(), 0);
    }

    #[test]
    fn provenance_follows_the_constructor_used() {
        let syllables = syllabify(&phones(&["B", "ER", "N"], &[Stress::Primary])).unwrap();
        assert_eq!(
            Pronunciation::from_dictionary(syllables.clone()).provenance(),
            Provenance::Dictionary
        );
        assert_eq!(
            Pronunciation::guessed(syllables.clone()).provenance(),
            Provenance::LetterToSound
        );
        // Same sound, different origin: these must not be equal, or a guess could stand in for a
        // lookup without anything noticing.
        assert_ne!(
            Pronunciation::from_dictionary(syllables.clone()),
            Pronunciation::guessed(syllables)
        );
    }

    #[test]
    fn stress_pattern_and_counts() {
        let word = Pronunciation::from_dictionary(
            syllabify(&phones(
                &["B", "ER", "N", "IH", "NG"],
                &[Stress::Primary, Stress::Unstressed],
            ))
            .unwrap(),
        );
        assert_eq!(word.syllable_count(), 2);
        assert_eq!(word.stress_pattern(), [Stress::Primary, Stress::Unstressed]);
        let all: Vec<_> = word.phonemes().map(Phoneme::symbol).collect();
        assert_eq!(all, ["B", "ER", "N", "IH", "NG"]);
    }

    #[test]
    fn stress_is_ordered_by_strength() {
        assert!(Stress::Primary > Stress::Secondary);
        assert!(Stress::Secondary > Stress::Unstressed);
        assert!(Stress::Primary.is_stressed());
        assert!(Stress::Secondary.is_stressed());
        assert!(!Stress::Unstressed.is_stressed());
    }
}
