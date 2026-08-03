//! Phonetic repetition: word families, alliteration, assonance.
//!
//! `SPEC.md` §4.3's remaining rows, and the one it says earns the project:
//!
//! > "burn / burning / burnt" → same voice, modulated envelope, filter, transposition. Exact
//! > repetition gives the loop; near-repetition gives the variation on the loop. That is most of
//! > electronic music's grammar.
//!
//! # Parallel to line voices, not a refinement of them
//!
//! [`VoiceId`](crate::repetition::VoiceId) says which lines are the same line. A [`WordVoiceId`]
//! says which *words* are the same word varied. They are independent: a line can be part of a
//! verbatim refrain and also contain a word whose family lives in an unrelated line. One identity
//! per line has nowhere to put the second, so this is a separate structure rather than a field.
//!
//! # Families are phonetic, and share a prefix
//!
//! §4.3 suggests `stem_match && phoneme_levenshtein <= k`. Spelling cannot do the stem half:
//! `burn` and `burnt` need an irregular table, while their phonemes share a clean `B ER N`. So
//! the match is on phonemes.
//!
//! Measured against the dictionary, not asserted: 80.6% of real inflection pairs join, and 5.0%
//! of merely-similar pairs do. `prosody-lts/tests/family_accuracy.rs` keeps those honest.
//!
//! The prefix requirement is what keeps this apart from rhyme. `burn` and `turn` are one edit
//! apart — but they *rhyme*, which §4.5 maps to pitch, whereas sharing a voice maps to timbre and
//! modulation. Conflating them would make a rhyme sound like a variation on the same instrument.
//! A family shares its root at the front; a rhyme shares its tail at the back.

use std::collections::BTreeMap;

use crate::parse::{Line, Poem};
use crate::phoneme::{Phoneme, Phonemiser, Provenance};
use crate::span::Span;

/// Which family of related words a word belongs to.
///
/// Numbered in order of first appearance, like every other identity in the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WordVoiceId(usize);

impl WordVoiceId {
    /// The voice number, counting from zero.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// How far a word has been varied from its family's root.
///
/// Phoneme edit distance. `SPEC.md` §4.3 asks for exactly this — "with distance driving
/// modulation depth" — so zero is the root itself, and larger means a deeper modulation of the
/// same voice rather than a different voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Distance(usize);

impl Distance {
    /// No variation: this word *is* the family's root form.
    pub const ROOT: Self = Self(0);

    /// The number of phoneme edits from the root.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }

    /// Whether this is the unvaried form.
    #[must_use]
    pub const fn is_root(self) -> bool {
        self.0 == 0
    }
}

/// One occurrence of a word, placed in its family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordForm {
    span: Span,
    voice: WordVoiceId,
    distance: Distance,
    provenance: Provenance,
}

impl WordForm {
    /// Where the word sits in the source.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Which family it belongs to.
    #[must_use]
    pub const fn voice(&self) -> WordVoiceId {
        self.voice
    }

    /// How far it is varied from the family's root.
    #[must_use]
    pub const fn distance(&self) -> Distance {
        self.distance
    }

    /// Whether the pronunciation behind it was guessed.
    #[must_use]
    pub fn rests_on_a_guess(&self) -> bool {
        self.provenance.is_guess()
    }
}

/// A phonetic figure within a window of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SoundFigureKind {
    /// Shared syllable onsets. `SPEC.md` §4.3: timbral continuity, the same sample bank.
    Alliteration,
    /// Shared syllable nuclei.
    Assonance,
}

impl SoundFigureKind {
    /// The conventional name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Alliteration => "alliteration",
            Self::Assonance => "assonance",
        }
    }
}

/// A run of words in one line sharing an onset or a nucleus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundFigure {
    kind: SoundFigureKind,
    phoneme: Phoneme,
    words: Vec<Span>,
}

impl SoundFigure {
    /// Which figure this is.
    #[must_use]
    pub const fn kind(&self) -> SoundFigureKind {
        self.kind
    }

    /// The phoneme the words share.
    #[must_use]
    pub const fn phoneme(&self) -> Phoneme {
        self.phoneme
    }

    /// The words sharing it, in source order.
    #[must_use]
    pub fn words(&self) -> &[Span] {
        &self.words
    }
}

/// A poem's phonetic repetitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundRepetition {
    poem: crate::seed::Fingerprint,
    forms: Vec<WordForm>,
    figures: Vec<SoundFigure>,
    voices: usize,
}

impl SoundRepetition {
    /// Which poem this describes; see [`Poem::fingerprint`](crate::parse::Poem::fingerprint).
    ///
    /// Spans are bare byte offsets, so two poems with the same line lengths have identical ones.
    /// Any pass combining several analyses must check this rather than compare spans.
    #[must_use]
    pub const fn poem(&self) -> crate::seed::Fingerprint {
        self.poem
    }

    /// Every pronounceable word of the poem, placed in a family, in source order.
    #[must_use]
    pub fn forms(&self) -> &[WordForm] {
        &self.forms
    }

    /// Every alliteration and assonance found.
    #[must_use]
    pub fn figures(&self) -> &[SoundFigure] {
        &self.figures
    }

    /// How many word families the poem has.
    #[must_use]
    pub const fn voice_count(&self) -> usize {
        self.voices
    }

    /// The forms belonging to one family, in source order.
    pub fn family(&self, voice: WordVoiceId) -> impl Iterator<Item = &WordForm> {
        self.forms.iter().filter(move |f| f.voice == voice)
    }

    /// The figures of one kind.
    pub fn figures_of(&self, kind: SoundFigureKind) -> impl Iterator<Item = &SoundFigure> {
        self.figures.iter().filter(move |f| f.kind == kind)
    }
}

/// The endings English inflects with, and where each one may attach.
///
/// A prefix rule alone is not enough: `sing` is phonetically a prefix of `sink`, exactly as it is
/// of `sings`. But a list of endings alone is not enough either — measured over the dictionary,
/// "any word plus `T`" joined `for`/`fort`, `car`/`cart` and `star`/`start`, and whole families
/// of fifty unrelated words chained together at 45% precision.
///
/// What fixes it is that English chooses its ending *by the sound before it*. The plural of a
/// voiceless-final word is `S` and of a voiced-final word is `Z`; the past of a voiceless-final
/// word is `T` and of a voiced-final word is `D`. So `for` + `T` is not a word English would
/// build — `R` is voiced, and the past would be `D`. Checking the allomorph is what separates
/// `walked` from `cart`.
///
/// `burnt` survives because English also has a small irregular class taking `T` after a nasal or
/// a lateral: burnt, learnt, dreamt, spelt, spilt, knelt.
static ENDINGS: &[Ending] = &[
    // Plural and third person, by voicing.
    Ending::after_voiceless(&["S"]),
    Ending::after_voiced(&["Z"]),
    Ending::after_sibilant(&["IH", "Z"]),
    Ending::after_sibilant(&["AH", "Z"]),
    // Past and participle, by voicing. `T` also covers the irregular nasal/lateral class.
    Ending::after_voiceless_or_sonorant(&["T"]),
    Ending::after_voiced(&["D"]),
    Ending::after_alveolar_stop(&["IH", "D"]),
    Ending::after_alveolar_stop(&["AH", "D"]),
    // These are unambiguous whatever precedes them.
    Ending::anywhere(&["IH", "NG"]),
    Ending::anywhere(&["IH", "NG", "Z"]),
    Ending::anywhere(&["ER"]),
    Ending::anywhere(&["ER", "Z"]),
    Ending::anywhere(&["IH", "S", "T"]),
    Ending::anywhere(&["AH", "S", "T"]),
    Ending::anywhere(&["L", "IY"]),
    Ending::anywhere(&["N", "AH", "S"]),
];

/// An ending, and the sounds it may follow.
struct Ending {
    phonemes: &'static [&'static str],
    context: Context,
}

/// Where an ending may attach.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    /// Any preceding sound.
    Anywhere,
    /// After a voiceless consonant, as `walked` and `cats`.
    Voiceless,
    /// After a voiced sound, as `burned` and `cars`.
    Voiced,
    /// After a sibilant, as `kisses` and `wishes`.
    Sibilant,
    /// After `T` or `D`, as `waited` and `landed`.
    AlveolarStop,
    /// After a voiceless consonant, or the nasals and laterals of the irregular `-t` class.
    VoicelessOrSonorant,
}

impl Ending {
    const fn anywhere(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::Anywhere,
        }
    }
    const fn after_voiceless(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::Voiceless,
        }
    }
    const fn after_voiced(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::Voiced,
        }
    }
    const fn after_sibilant(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::Sibilant,
        }
    }
    const fn after_alveolar_stop(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::AlveolarStop,
        }
    }
    const fn after_voiceless_or_sonorant(phonemes: &'static [&'static str]) -> Self {
        Self {
            phonemes,
            context: Context::VoicelessOrSonorant,
        }
    }

    /// Whether this ending may follow `previous`.
    fn fits(&self, previous: &str) -> bool {
        let voiceless = matches!(
            previous,
            "P" | "T" | "K" | "F" | "TH" | "S" | "SH" | "CH" | "HH"
        );
        let sibilant = matches!(previous, "S" | "Z" | "SH" | "ZH" | "CH" | "JH");
        let alveolar_stop = matches!(previous, "T" | "D");
        let sonorant = matches!(previous, "N" | "M" | "L");
        match self.context {
            Context::Anywhere => true,
            // A sibilant takes the syllabic ending instead, so `S`/`Z` are excluded after one.
            Context::Voiceless => voiceless && !sibilant,
            Context::Voiced => !voiceless && !sibilant && !alveolar_stop,
            Context::Sibilant => sibilant,
            Context::AlveolarStop => alveolar_stop,
            Context::VoicelessOrSonorant => (voiceless || sonorant) && !alveolar_stop,
        }
    }
}

/// The fewest phonemes a word needs before it can join a family at all.
///
/// Short function words share prefixes constantly, and grouping "a" with "at" would put most of
/// English in one voice.
const MIN_LENGTH: usize = 3;

/// Finds a poem's phonetic repetitions.
#[must_use]
pub fn analyse(poem: &Poem, phonemiser: &impl Phonemiser) -> SoundRepetition {
    let mut words: Vec<(Span, Vec<Phoneme>, Provenance)> = Vec::new();
    let mut figures = Vec::new();

    for stanza in poem.stanzas() {
        for line in stanza.lines() {
            let spoken = pronounce_line(line, poem, phonemiser);
            figures.extend(line_figures(&spoken));
            for (span, pronunciation) in spoken {
                words.push((
                    span,
                    pronunciation.phonemes().collect(),
                    pronunciation.provenance(),
                ));
            }
        }
    }

    let (forms, voices) = assign_families(&words);
    SoundRepetition {
        poem: poem.fingerprint(),
        forms,
        figures,
        voices,
    }
}

/// Every pronounceable word of a line, with its sound.
fn pronounce_line(
    line: &Line,
    poem: &Poem,
    phonemiser: &impl Phonemiser,
) -> Vec<(Span, crate::phoneme::Pronunciation)> {
    line.words()
        .filter_map(|token| {
            let span = token.span();
            let text = span.text(poem.source())?;
            phonemiser.pronounce(text).ok().map(|p| (span, p))
        })
        .collect()
}

/// Groups words into families by shared phonetic root.
///
/// A word joins the family whose root it shares most of, and founds one otherwise. The first form
/// of a family is its root, so distance is measured from the word as the poem first said it —
/// which makes "burn" the root of "burning" in a poem that says "burn" first, and the reverse in
/// one that does not. `SPEC.md` §4.5 already assigns rhyme classes by order of first appearance;
/// this is the same rule.
fn assign_families(words: &[(Span, Vec<Phoneme>, Provenance)]) -> (Vec<WordForm>, usize) {
    // Every distinct sound in each family, not only its root. "burnt" and "burning" are each a
    // family with "burn" but not with each other, so a poem that said "burnt" first would
    // otherwise split one family in two. Distinct, because refrain-heavy verse repeats the same
    // handful of words hundreds of times and comparing against every occurrence is quadratic.
    let mut families: Vec<Vec<Vec<Phoneme>>> = Vec::new();
    let mut placed: Vec<usize> = Vec::with_capacity(words.len());

    for (_, phonemes, _) in words {
        let mut best: Option<(usize, usize)> = None;
        for (index, members) in families.iter().enumerate() {
            let Some(nearest) = members
                .iter()
                .filter_map(|member| family_distance(phonemes, member))
                .min()
            else {
                continue;
            };
            if best.is_none_or(|(_, seen)| nearest < seen) {
                best = Some((index, nearest));
            }
        }
        let voice = if let Some((index, _)) = best {
            if let Some(members) = families.get_mut(index)
                && !members.contains(phonemes)
            {
                members.push(phonemes.clone());
            }
            index
        } else {
            families.push(vec![phonemes.clone()]);
            families.len() - 1
        };
        placed.push(voice);
    }

    // Depth is measured from each family's *shortest* form — the base the others are built on.
    // Measuring from whichever form the poem said first made "burning burn burnt" report the same
    // depth for "burn" and "burnt", collapsing "no suffix" and "a shallow suffix" into one
    // modulation, purely because of word order. `SPEC.md` §4.3 uses this number as depth, so it
    // has to describe the words rather than their order.
    let roots: Vec<Vec<Phoneme>> = families
        .iter()
        .map(|members| {
            members
                .iter()
                .min_by(|a, b| a.len().cmp(&b.len()).then_with(|| compare(a, b)))
                .cloned()
                .unwrap_or_default()
        })
        .collect();

    let forms = words
        .iter()
        .zip(placed)
        .map(|((span, phonemes, provenance), voice)| WordForm {
            span: *span,
            voice: WordVoiceId(voice),
            distance: Distance(
                roots
                    .get(voice)
                    .map_or(0, |root| edit_distance(phonemes, root)),
            ),
            provenance: *provenance,
        })
        .collect();
    let count = families.len();
    (forms, count)
}

/// A total order over phoneme sequences, so the choice of root cannot depend on iteration order.
fn compare(a: &[Phoneme], b: &[Phoneme]) -> core::cmp::Ordering {
    a.iter().cmp(b.iter())
}

/// The edit distance between two words if they are one family, or `None` if they are not.
///
/// English inflection *adds* material: "burn" becomes "burnt", "burning", "burns". So one word's
/// phonemes must be a **prefix** of the other's, and what remains must be an ending English would
/// actually attach to that sound — see [`ENDINGS`].
///
/// Both halves are needed. Containment without the ending check groups `sing` with `sink`; the
/// ending list without the *context* check groups `for` with `fort` and `car` with `cart`.
///
/// Two consequences worth naming. It separates variation from rhyme: `burn` and `turn` are one
/// edit apart but share a tail rather than a root, and §4.5 already maps that to pitch. And it
/// misses irregulars — `sing`/`sang`, `go`/`went` — which no prefix rule can reach.
#[must_use]
fn family_distance(a: &[Phoneme], b: &[Phoneme]) -> Option<usize> {
    // The same word is always the same voice, however short. The length gate below is about
    // *near* matches: without this, a repeated "the" founded a new family each time, and an exact
    // repetition — the one case §4.3 is unambiguous about — would have been missed.
    if a == b {
        return Some(0);
    }
    let (shorter, longer) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if shorter.len() < MIN_LENGTH {
        return None;
    }
    if longer.get(..shorter.len()) != Some(shorter) {
        return None;
    }
    let ending: Vec<&str> = longer
        .get(shorter.len()..)?
        .iter()
        .copied()
        .map(Phoneme::symbol)
        .collect();
    let previous = shorter.last().copied().map(Phoneme::symbol)?;
    let fits = ENDINGS
        .iter()
        .any(|candidate| candidate.phonemes == ending.as_slice() && candidate.fits(previous));
    fits.then(|| edit_distance(a, b))
}

/// Levenshtein distance over phonemes.
fn edit_distance(a: &[Phoneme], b: &[Phoneme]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0_usize; b.len() + 1];
    for (i, left) in a.iter().enumerate() {
        if let Some(slot) = current.first_mut() {
            *slot = i + 1;
        }
        for (j, right) in b.iter().enumerate() {
            let cost = usize::from(left != right);
            let diagonal = previous.get(j).copied().unwrap_or(0) + cost;
            let up = previous.get(j + 1).copied().unwrap_or(0) + 1;
            let left_cell = current.get(j).copied().unwrap_or(0) + 1;
            if let Some(slot) = current.get_mut(j + 1) {
                *slot = diagonal.min(up).min(left_cell);
            }
        }
        previous.clone_from(&current);
    }
    previous.last().copied().unwrap_or(0)
}

/// Alliteration and assonance within one line.
///
/// The window is the line. Rhyme is line-final and across lines, so keeping these inside a line
/// means the two passes cannot report the same pair — a question raised when rhyme landed.
fn line_figures(spoken: &[(Span, crate::phoneme::Pronunciation)]) -> Vec<SoundFigure> {
    let mut onsets: BTreeMap<Phoneme, Vec<Span>> = BTreeMap::new();
    let mut nuclei: BTreeMap<Phoneme, Vec<Span>> = BTreeMap::new();

    for (span, pronunciation) in spoken {
        // The *stressed* syllable, not the first. "collide" and "believe" alliterate on their
        // stressed `L`; taking the first syllable instead reported a spurious assonance on the
        // unstressed schwa they both open with, and missed the alliteration entirely. The rhyme
        // pass already reckons from the stressed syllable for the same reason.
        let syllables = pronunciation.syllables();
        let Some(carrying) = syllables
            .iter()
            .find(|syllable| syllable.stress().is_stressed())
            .or_else(|| syllables.first())
        else {
            continue;
        };
        if let Some(onset) = carrying.onset().first() {
            onsets.entry(*onset).or_default().push(*span);
        }
        nuclei.entry(carrying.nucleus()).or_default().push(*span);
    }

    let mut figures = Vec::new();
    for (phoneme, words) in onsets {
        if words.len() > 1 {
            figures.push(SoundFigure {
                kind: SoundFigureKind::Alliteration,
                phoneme,
                words,
            });
        }
    }
    for (phoneme, words) in nuclei {
        if words.len() > 1 {
            figures.push(SoundFigure {
                kind: SoundFigureKind::Assonance,
                phoneme,
                words,
            });
        }
    }
    figures
}

#[cfg(test)]
mod tests {
    use super::{Distance, SoundFigureKind, SoundRepetition, analyse, family_distance};
    use crate::parse::Poem;
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phoneme, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };

    const W: Stress = Stress::Unstressed;
    const S: Stress = Stress::Primary;

    struct Fake;

    /// Words chosen so families, rhymes and near-misses are all present.
    static WORDS: &[(&str, &[(&str, Stress)])] = &[
        ("burn", &[("B", W), ("ER", S), ("N", W)]),
        ("burnt", &[("B", W), ("ER", S), ("N", W), ("T", W)]),
        (
            "burning",
            &[("B", W), ("ER", S), ("N", W), ("IH", W), ("NG", W)],
        ),
        ("burns", &[("B", W), ("ER", S), ("N", W), ("Z", W)]),
        // Rhymes with burn, shares no root.
        ("turn", &[("T", W), ("ER", S), ("N", W)]),
        // Shares two phonemes with burn, but is a different word.
        ("bird", &[("B", W), ("ER", S), ("D", W)]),
        ("fire", &[("F", W), ("AY", S), ("ER", W)]),
        ("fired", &[("F", W), ("AY", S), ("ER", W), ("D", W)]),
        ("tide", &[("T", W), ("AY", S), ("D", W)]),
        ("dark", &[("D", W), ("AA", S), ("R", W), ("K", W)]),
        ("deep", &[("D", W), ("IY", S), ("P", W)]),
        ("sing", &[("S", W), ("IH", S), ("NG", W)]),
        ("sings", &[("S", W), ("IH", S), ("NG", W), ("Z", W)]),
        ("sink", &[("S", W), ("IH", S), ("NG", W), ("K", W)]),
        ("for", &[("F", W), ("AO", S), ("R", W)]),
        ("fort", &[("F", W), ("AO", S), ("R", W), ("T", W)]),
        ("car", &[("K", W), ("AA", S), ("R", W)]),
        ("cart", &[("K", W), ("AA", S), ("R", W), ("T", W)]),
        ("walk", &[("W", W), ("AO", S), ("K", W)]),
        ("walked", &[("W", W), ("AO", S), ("K", W), ("T", W)]),
        // Stress on the second syllable, so the figure must not read the first.
        (
            "collide",
            &[("K", W), ("AH", W), ("L", W), ("AY", S), ("D", W)],
        ),
        (
            "believe",
            &[("B", W), ("AH", W), ("L", W), ("IY", S), ("V", W)],
        ),
        ("the", &[("DH", W), ("AH", W)]),
        ("a", &[("AH", W)]),
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

    fn sound(source: &str) -> SoundRepetition {
        analyse(&Poem::parse(source).unwrap(), &Fake)
    }

    fn phonemes(word: &str) -> Vec<Phoneme> {
        Fake.pronounce(word).unwrap().phonemes().collect()
    }

    fn related(a: &str, b: &str) -> Option<usize> {
        family_distance(&phonemes(a), &phonemes(b))
    }

    #[test]
    fn the_row_that_earns_the_project() {
        // SPEC.md §4.3: "burn / burning / burnt → same voice, modulated envelope, filter,
        // transposition."
        let rep = sound("burn burning burnt\n");
        let forms = rep.forms();
        assert_eq!(forms.len(), 3);
        assert_eq!(forms[0].voice(), forms[1].voice());
        assert_eq!(forms[0].voice(), forms[2].voice());
        assert_eq!(rep.voice_count(), 1, "one voice, three forms");

        // The first form is the root; the others are varied.
        assert!(forms[0].distance().is_root());
        assert!(!forms[1].distance().is_root());
        assert!(!forms[2].distance().is_root());
    }

    #[test]
    fn a_family_joins_through_a_shared_member() {
        // "burnt" and "burning" do not relate directly, but both relate to "burn". Whichever the
        // poem says first, all three end up in one voice.
        for source in [
            "burn burnt burning\n",
            "burnt burn burning\n",
            "burning burn burnt\n",
        ] {
            let rep = sound(source);
            assert_eq!(rep.voice_count(), 1, "{source:?} -> {:?}", rep.forms());
        }
    }

    #[test]
    fn distance_orders_the_variations() {
        // "burnt" adds one phoneme, "burning" adds two — so burnt is the shallower modulation.
        let rep = sound("burn burnt burning\n");
        let forms = rep.forms();
        assert_eq!(forms[1].distance(), Distance(1));
        assert_eq!(forms[2].distance(), Distance(2));
        assert!(forms[1].distance() < forms[2].distance());
    }

    #[test]
    fn a_rhyme_is_not_a_variation() {
        // "burn" and "turn" are one edit apart, but they share a tail, not a root. §4.5 maps that
        // to pitch; putting them in one voice would map it to timbre instead.
        assert_eq!(related("burn", "turn"), None);
        let rep = sound("burn turn\n");
        assert_ne!(rep.forms()[0].voice(), rep.forms()[1].voice());
        assert_eq!(rep.voice_count(), 2);
    }

    #[test]
    fn a_containment_is_not_enough_without_a_real_ending() {
        // "sing" really is a phonetic prefix of "sink", exactly as it is of "sings". What tells
        // them apart is that `Z` is an English ending after a nasal and `K` is not. Guarded here
        // rather than only in another crate's real-word test, where it used to be the sole check.
        assert_eq!(related("sing", "sings"), Some(1));
        assert_eq!(related("sing", "sink"), None);
    }

    #[test]
    fn an_ending_must_suit_the_sound_before_it() {
        // English picks its ending by the sound before it: `T` after a voiceless consonant or a
        // nasal, `D` after a voiced one. "for" ends in a voiced `R`, so "fort" is not "for" plus
        // an ending English would build — and without this check a measured 45% of families were
        // accidents like for/fort, car/cart and star/start.
        assert_eq!(related("for", "fort"), None);
        assert_eq!(related("car", "cart"), None);
        // ...while the regular and irregular past forms both survive.
        assert_eq!(related("walk", "walked"), Some(1));
        assert_eq!(related("burn", "burnt"), Some(1));
    }

    #[test]
    fn a_shared_opening_is_not_enough() {
        // "burn" and "bird" share `B ER`, which is two thirds of the shorter — not enough.
        assert_eq!(related("burn", "bird"), None);
    }

    #[test]
    fn a_family_admits_several_inflections() {
        assert_eq!(related("burn", "burnt"), Some(1));
        assert_eq!(related("burn", "burns"), Some(1));
        assert_eq!(related("burn", "burning"), Some(2));
        // Neither is a prefix of the other, so they are not *directly* related — they join
        // through "burn", which is why families are matched against every member rather than
        // only the root.
        assert_eq!(related("burnt", "burning"), None);
        assert_eq!(related("fire", "fired"), Some(1));
    }

    #[test]
    fn short_words_do_not_form_families() {
        // Function words share prefixes constantly; grouping them would put most of English in
        // one voice.
        assert_eq!(related("the", "a"), None);
        // ...but a repeated short word is still the same word.
        assert_eq!(related("the", "the"), Some(0));
        let rep = sound("the a the\n");
        assert_eq!(
            rep.voice_count(),
            2,
            "the two `the`s are one voice; `a` is another"
        );
    }

    #[test]
    fn a_repeated_word_is_its_own_family_at_distance_zero() {
        let rep = sound("burn and burn\n");
        let forms = rep.forms();
        // "and" is not in the table, so only the two "burn"s are pronounceable.
        assert_eq!(forms.len(), 2);
        assert_eq!(forms[0].voice(), forms[1].voice());
        assert!(
            forms[1].distance().is_root(),
            "an exact repeat is not varied"
        );
    }

    #[test]
    fn families_cross_lines_and_stanzas() {
        let source = "the burn\n\nthe burning\n";
        let rep = sound(source);
        let forms = rep.forms();
        assert_eq!(forms.len(), 4, "every word here is pronounceable");
        let text = |i: usize| forms[i].span().text(source);
        assert_eq!((text(1), text(3)), (Some("burn"), Some("burning")));
        assert_eq!(
            forms[1].voice(),
            forms[3].voice(),
            "one family across stanzas"
        );
        assert!(forms[3].distance().get() > 0);
        // "the" repeats, so it is its own family at distance zero.
        assert_eq!(forms[0].voice(), forms[2].voice());
    }

    #[test]
    fn every_form_traces_to_its_own_word() {
        let source = "burn burning burnt\n";
        let rep = sound(source);
        let texts: Vec<&str> = rep
            .forms()
            .iter()
            .filter_map(|f| f.span().text(source))
            .collect();
        assert_eq!(texts, ["burn", "burning", "burnt"]);
    }

    #[test]
    fn alliteration_is_a_shared_onset_within_a_line() {
        let source = "burn burning dark\n";
        let rep = sound(source);
        let figures: Vec<_> = rep.figures_of(SoundFigureKind::Alliteration).collect();
        assert_eq!(figures.len(), 1);
        assert_eq!(figures[0].phoneme().symbol(), "B");
        let words: Vec<&str> = figures[0]
            .words()
            .iter()
            .filter_map(|s| s.text(source))
            .collect();
        assert_eq!(words, ["burn", "burning"]);
    }

    #[test]
    fn assonance_is_a_shared_nucleus_within_a_line() {
        let source = "fire tide dark\n";
        let rep = sound(source);
        let figures: Vec<_> = rep.figures_of(SoundFigureKind::Assonance).collect();
        assert_eq!(figures.len(), 1);
        assert_eq!(figures[0].phoneme().symbol(), "AY");
        let words: Vec<&str> = figures[0]
            .words()
            .iter()
            .filter_map(|s| s.text(source))
            .collect();
        assert_eq!(words, ["fire", "tide"]);
    }

    #[test]
    fn a_phonetic_figure_does_not_span_lines() {
        // Rhyme is line-final and across lines; keeping these inside a line means the two passes
        // never report the same pair.
        let rep = sound("burn dark\ndeep burning\n");
        for figure in rep.figures() {
            assert_eq!(figure.words().len(), 2);
        }
        // "burn" and "burning" alliterate, but they are on different lines, so no figure.
        assert!(
            rep.figures_of(SoundFigureKind::Alliteration)
                .all(|f| f.phoneme().symbol() != "B")
        );
    }

    #[test]
    fn a_figure_reads_the_stressed_syllable() {
        // "collide" and "believe" both open on an unstressed schwa and carry `L` in the stressed
        // syllable. Reading the first syllable reported assonance on the schwa and missed the
        // alliteration.
        let source = "collide believe\n";
        let rep = sound(source);
        let alliteration: Vec<_> = rep.figures_of(SoundFigureKind::Alliteration).collect();
        assert_eq!(alliteration.len(), 1, "{alliteration:?}");
        assert_eq!(alliteration[0].phoneme().symbol(), "L");
        assert_eq!(
            rep.figures_of(SoundFigureKind::Assonance).count(),
            0,
            "the shared schwa is unstressed and is not assonance"
        );
    }

    #[test]
    fn the_root_is_the_shortest_form_whatever_the_poem_says_first() {
        // Measuring depth from whichever form came first made "burn" and "burnt" report the same
        // depth when the poem opened on "burning" — collapsing no-suffix and shallow-suffix into
        // one modulation because of word order.
        let forward = sound("burn burnt burning\n");
        let reverse = sound("burning burnt burn\n");
        let depths = |rep: &SoundRepetition, word: &str, source: &str| {
            rep.forms()
                .iter()
                .find(|f| f.span().text(source) == Some(word))
                .map(|f| f.distance().get())
        };
        assert_eq!(depths(&forward, "burn", "burn burnt burning\n"), Some(0));
        assert_eq!(depths(&reverse, "burn", "burning burnt burn\n"), Some(0));
        assert_eq!(depths(&forward, "burnt", "burn burnt burning\n"), Some(1));
        assert_eq!(depths(&reverse, "burnt", "burning burnt burn\n"), Some(1));
    }

    #[test]
    fn alliteration_needs_two_words() {
        let rep = sound("burn tide\n");
        assert_eq!(rep.figures_of(SoundFigureKind::Alliteration).count(), 0);
    }

    #[test]
    fn an_unpronounceable_word_has_no_form() {
        let rep = sound("burn krapplesnaffle\n");
        assert_eq!(rep.forms().len(), 1);
    }

    #[test]
    fn a_family_can_be_read_back() {
        let rep = sound("burn tide burning\n");
        let voice = rep.forms()[0].voice();
        assert_eq!(rep.family(voice).count(), 2);
    }

    #[test]
    fn analysis_is_deterministic() {
        let source = "burn burning burnt\n\nthe fire fired\n";
        assert_eq!(sound(source), sound(source));
    }
}
