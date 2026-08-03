//! Property tests for the invariants in `SPEC.md` §6.
//!
//! These are written against the invariants themselves rather than against any one pass, and
//! they grow as the pipeline does. They currently cover I1 (determinism of seeding and of
//! incremental reparse), I2 (per-unit seeding, and locality of a real edit across stanzas) and
//! I4 (spans resolve, nest and are attributable). I3 arrives with the style layer; see `PLAN.md`.
//!
//! Every property here must be able to fail. A property that holds by virtue of a function
//! signature rather than by virtue of the implementation is not a test, and two such properties
//! were removed from this file after review rather than left in to inflate the count.

// Property-test bodies unwrap and index freely, as test code may. See CLAUDE.md §3.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::{
    Strategy, prop_assert, prop_assert_eq, prop_assert_ne, prop_assume, proptest,
};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{Inventory, Phonemiser, Pronunciation, Unpronounceable};
use prosody_core::repetition::FigureKind;
use prosody_core::sound;
use prosody_core::surprisal::{self, Lexicon, Rarity};
use prosody_core::{
    Domain, Phoneme, Poem, Seed, Span, StanzaIdx, Stress, meter, phoneme, repetition, rhyme,
};

/// A lexicon whose bands vary with spelling, so generated poems contain both common and rare
/// words rather than everything landing in one band.
struct LengthLexicon;

impl Lexicon for LengthLexicon {
    fn rarity(&self, word: &str) -> Rarity {
        Rarity::band(u8::try_from(word.chars().count()).unwrap_or(u8::MAX))
    }
}

/// A phonemiser whose output varies with spelling, so a generated poem contains lines that
/// genuinely rhyme and lines that genuinely do not.
///
/// Giving every syllable the same vowel would make *every* line rhyme with every other, and a
/// classifier that dropped everything into one class would pass the rhyme properties below.
struct EverySyllableStressed;

/// The vowel a letter contributes, so different words end differently.
fn vowel_for(c: char) -> Option<&'static str> {
    Some(match c {
        'a' => "AA",
        'e' => "EH",
        'i' => "IH",
        'o' => "OW",
        'u' => "UW",
        _ => return None,
    })
}

impl Phonemiser for EverySyllableStressed {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let letters: Vec<char> = word.chars().collect();
        let nuclei: Vec<&'static str> = letters.iter().filter_map(|c| vowel_for(*c)).collect();
        if nuclei.is_empty() {
            return Err(Unpronounceable::NotKnown);
        }
        // A coda from the word's last consonant, so endings differ beyond their vowel.
        let coda = letters
            .iter()
            .rev()
            .find(|c| vowel_for(**c).is_none())
            .and_then(|c| match c {
                'b'..='d' => ARPABET.phoneme("B"),
                'f'..='k' => ARPABET.phoneme("K"),
                'l'..='p' => ARPABET.phoneme("N"),
                'q'..='t' => ARPABET.phoneme("S"),
                _ => ARPABET.phoneme("T"),
            });

        let last = nuclei.len() - 1;
        let syllables = nuclei
            .iter()
            .enumerate()
            .map(|(i, symbol)| {
                let stress = if i % 2 == 0 {
                    Stress::Primary
                } else {
                    Stress::Unstressed
                };
                let coda = if i == last {
                    coda.into_iter().collect()
                } else {
                    Vec::new()
                };
                prosody_core::Syllable::new(
                    Vec::new(),
                    ARPABET.phoneme(symbol).unwrap(),
                    coda,
                    stress,
                )
                .unwrap()
            })
            .collect();
        Ok(Pronunciation::from_dictionary(syllables))
    }
}

use rand_core::Rng;

/// A poem built to contain repetition: lines are drawn from a small vocabulary and assembled
/// from a shared opening, a middle, and a shared ending, so refrains, anaphora and epistrophe all
/// occur at usable rates.
///
/// The plain generator produced no anaphora or epistrophe at all — measured at 0 in 5000 samples
/// — so every repetition property held vacuously for two of the three figures. This is the same
/// fixture problem the rhyme properties hit an iteration earlier.
fn repetitive_poem() -> impl Strategy<Value = String> {
    // Each line is a shared opening, a middle, and a shared ending, drawn from small pools. That
    // makes refrains, anaphora and epistrophe all common — and, because openings vary in length,
    // makes *nested* figures common too, which is what the once-per-line-set property needs.
    let opening = proptest::sample::select(vec![
        Vec::new(),
        vec!["and"],
        vec!["and", "the"],
        vec!["and", "the", "tide"],
    ]);
    let middle = proptest::collection::vec(
        proptest::sample::select(vec!["burns", "dark", "fire", "slow"]),
        0..3,
    );
    let ending = proptest::sample::select(vec![Vec::new(), vec!["in"], vec!["came", "in"]]);

    let line = (opening, middle, ending).prop_map(|(open, mid, end)| {
        let mut words: Vec<&str> = open;
        words.extend(mid);
        words.extend(end);
        if words.is_empty() {
            words.push("rain");
        }
        words.join(" ")
    });

    proptest::collection::vec(line, 1..8).prop_map(|lines| {
        let mut poem = String::new();
        for (index, line) in lines.iter().enumerate() {
            poem.push_str(line);
            poem.push('\n');
            if index % 3 == 2 {
                poem.push('\n');
            }
        }
        poem
    })
}

/// A well-formed poem: stanzas of lines of words, blank-line separated.
fn poem_source() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::collection::vec(proptest::collection::vec("[a-zšč]{1,6}", 1..4), 1..4),
        1..4,
    )
    .prop_map(|stanzas| {
        let rendered: Vec<String> = stanzas
            .iter()
            .map(|lines| {
                lines
                    .iter()
                    .map(|words| words.join(" "))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        format!("{}\n", rendered.join("\n\n"))
    })
}

/// Every stanza's seed, in order.
fn stanza_seeds(poem: &Poem) -> Vec<Seed> {
    (0..poem.stanzas().len())
        .filter_map(|i| poem.stanza_seed(StanzaIdx(i)))
        .collect()
}

/// Every token span in the poem, in source order.
fn token_spans(poem: &Poem) -> Vec<Span> {
    poem.stanzas()
        .iter()
        .flat_map(prosody_core::Stanza::lines)
        .flat_map(prosody_core::Line::tokens)
        .copied()
        .map(prosody_core::Token::span)
        .collect()
}

/// Replacement texts an edit might use.
///
/// The point of the awkward ones is coverage the earlier version of this file lacked: an empty
/// string deletes, `a\nb` inserts a line break, `\n\n` splits a stanza, and `gorči` is
/// multi-byte. Without those, every generated edit stayed inside one line, and the row/column
/// arithmetic in `Poem::apply` was never required to be right — a deliberately broken
/// `point_of` passed the whole property suite.
fn replacement() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        "", "x", "quenched", "gorči", "a\nb", "\n\n", "  ", "ó",
    ])
}

/// The largest byte offset the span strategies draw. Chosen to sit somewhat beyond the longest
/// source `poem_text()` can produce, so that in-bounds, boundary and out-of-bounds spans all
/// occur at usable rates.
const MAX_OFFSET: usize = 200;

/// Text shaped roughly like a stanza: words, spaces and newlines, plus multi-byte characters at
/// two, three and four bytes wide, so the non-English case from `SPEC.md` §9 is exercised rather
/// than assumed away.
fn poem_text() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Zčšžæø€漢🔥' \n]{0,40}").unwrap()
}

fn domain() -> impl Strategy<Value = Domain> {
    proptest::sample::select(vec![Domain::Stanza, Domain::Line])
}

/// Sub-stream labels must be `'static`, so the strategy selects from a fixed set.
fn label() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        "detune",
        "sample-bank",
        "ornament",
        "velocity",
        "filter",
    ])
}

/// Seeds a whole string as one unit.
fn seed_of(domain: Domain, text: &str) -> Seed {
    Seed::for_unit(domain, Span::new(0, text.len()), text).unwrap()
}

fn draw(seed: &Seed, count: usize) -> Vec<u32> {
    let mut rng = seed.rng();
    (0..count).map(|_| rng.next_u32()).collect()
}

/// An arbitrary phone sequence, drawn from the whole `ARPAbet` inventory.
///
/// Arbitrary sequences reach shapes no real word has — clusters English never uses, runs of
/// vowels, consonant-only words — which is the point: the dictionary corpus only exercises
/// sequences that happen to occur in English.
fn phone_sequence() -> impl Strategy<Value = Vec<(Phoneme, Stress)>> {
    let symbols: Vec<&'static str> = ARPABET.symbols.iter().map(|s| s.text).collect();
    proptest::collection::vec(
        (
            proptest::sample::select(symbols),
            proptest::sample::select(vec![Stress::Unstressed, Stress::Secondary, Stress::Primary]),
        ),
        0..12,
    )
    .prop_map(|drawn| {
        drawn
            .into_iter()
            .map(|(symbol, stress)| {
                let phoneme = ARPABET.phoneme(symbol).unwrap();
                let stress = if phoneme.is_vowel() {
                    stress
                } else {
                    Stress::Unstressed
                };
                (phoneme, stress)
            })
            .collect()
    })
}

proptest! {
    /// Surprisal is bounded, and its two halves stay within the whole.
    ///
    /// `SPEC.md` §4.4 turns this number into filter cutoff and attack time, so a value outside
    /// `0..=1` would drive a parameter off its range rather than to its edge.
    #[test]
    fn surprisal_is_bounded(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let scored = surprisal::analyse(&poem, &EverySyllableStressed, &LengthLexicon);
        let one = surprisal::Surprisal::NONE;
        for word in scored.words() {
            prop_assert!(word.surprisal() >= one);
            prop_assert!(word.novelty() >= one);
            prop_assert!(word.rarity().get() < Rarity::BANDS);
            // Both halves are ratios of at most one, so their weighted mean is too.
            prop_assert!(word.surprisal().as_f64() <= 1.0 + f64::EPSILON);
            prop_assert!(word.novelty().as_f64() <= 1.0 + f64::EPSILON);
        }
    }

    /// I4 — every score names the word it came from.
    #[test]
    fn i4_every_score_traces_to_a_word(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let scored = surprisal::analyse(&poem, &EverySyllableStressed, &LengthLexicon);
        let mut seen = std::collections::BTreeSet::new();
        for word in scored.words() {
            prop_assert!(word.span().text(poem.source()).is_some());
            prop_assert!(seen.insert(word.span()), "a word scored twice");
        }
    }

    /// Rarity depends on the word alone, never on its neighbours — the half of §4.4 that is
    /// supposed to be context-free.
    #[test]
    fn rarity_ignores_context(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let scored = surprisal::analyse(&poem, &EverySyllableStressed, &LengthLexicon);
        for word in scored.words() {
            let text = word.span().text(poem.source()).unwrap();
            prop_assert_eq!(word.rarity(), LengthLexicon.rarity(text));
        }
    }

    /// I1 — scoring is a pure function of the poem, the phonemiser and the lexicon.
    #[test]
    fn i1_surprisal_is_reproducible(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(
            surprisal::analyse(&poem, &EverySyllableStressed, &LengthLexicon),
            surprisal::analyse(&poem, &EverySyllableStressed, &LengthLexicon)
        );
    }

    /// A word family means what it says: every member shares its root's opening, and no two
    /// families hold words that should have been one.
    ///
    /// `SPEC.md` §4.3 turns a shared voice into a shared instrument and the distance into
    /// modulation depth, so a family holding unrelated words would play them as variations of
    /// each other.
    #[test]
    fn word_families_share_a_root(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let phonemiser = EverySyllableStressed;
        let rep = sound::analyse(&poem, &phonemiser);

        let phonemes_of = |form: &sound::WordForm| -> Vec<prosody_core::Phoneme> {
            let text = form.span().text(poem.source()).unwrap();
            phonemiser.pronounce(text).unwrap().phonemes().collect()
        };

        for voice in 0..rep.voice_count() {
            let members: Vec<&sound::WordForm> = rep
                .forms()
                .iter()
                .filter(|f| f.voice().get() == voice)
                .collect();
            let Some(root) = members.first() else { continue };
            let root_sound = phonemes_of(root);
            prop_assert!(root.distance().is_root(), "a family's first form is its root");

            for member in &members {
                let sound = phonemes_of(member);
                // Every member shares the root's opening, up to the shorter of the two.
                let shared = root_sound
                    .iter()
                    .zip(sound.iter())
                    .take_while(|(a, b)| a == b)
                    .count();
                let shorter = root_sound.len().min(sound.len());
                prop_assert!(
                    shared >= shorter.saturating_sub(1) || shared * 4 >= shorter * 3,
                    "a family member that does not share the root's opening"
                );
                // Distance is zero exactly when the sound is identical.
                prop_assert_eq!(member.distance().is_root(), sound == root_sound);
            }
        }
    }

    /// I4 — every word form and every phonetic figure names real source text.
    #[test]
    fn i4_every_sound_form_traces_to_a_word(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = sound::analyse(&poem, &EverySyllableStressed);
        for form in rep.forms() {
            prop_assert!(form.span().text(poem.source()).is_some());
            prop_assert!(form.voice().get() < rep.voice_count());
        }
        for figure in rep.figures() {
            prop_assert!(figure.words().len() >= 2, "a figure needs two words");
            let mut seen = std::collections::BTreeSet::new();
            for word in figure.words() {
                prop_assert!(word.text(poem.source()).is_some());
                prop_assert!(seen.insert(*word), "a word listed twice in one figure");
            }
        }
    }

    /// I1 — phonetic analysis is a pure function of the poem and the phonemiser.
    #[test]
    fn i1_sound_analysis_is_reproducible(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(
            sound::analyse(&poem, &EverySyllableStressed),
            sound::analyse(&poem, &EverySyllableStressed)
        );
    }

    /// Voices mean what they say: lines share a voice exactly when they are the same line.
    ///
    /// `SPEC.md` §4.3's refrain row turns a shared voice into a literal loop, so a voice shared
    /// by two different lines would loop something the poem never repeated.
    #[test]
    fn a_voice_is_shared_exactly_by_identical_lines(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = repetition::analyse(&poem);

        let normalise = |span: Span| -> Option<String> {
            Some(span.text(poem.source())?.to_lowercase())
        };
        let placed: Vec<(usize, Option<String>)> = rep
            .stanzas()
            .iter()
            .flat_map(prosody_core::repetition::StanzaRepetition::lines)
            .map(|line| (line.voice().get(), normalise(line.line())))
            .collect();

        for (i, (voice, text)) in placed.iter().enumerate() {
            for (other, other_text) in placed.iter().skip(i + 1) {
                if voice == other {
                    prop_assert_eq!(text, other_text, "one voice, two different lines");
                } else {
                    prop_assert_ne!(text, other_text, "one line, two different voices");
                }
            }
        }
    }

    /// I4 — every figure occurrence resolves to real source text, and sits inside some line.
    #[test]
    fn i4_every_figure_occurrence_resolves(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = repetition::analyse(&poem);
        let lines: Vec<Span> = rep
            .stanzas()
            .iter()
            .flat_map(prosody_core::repetition::StanzaRepetition::lines)
            .map(repetition::LineRepetition::line)
            .collect();

        for figure in rep.figures() {
            prop_assert!(figure.occurrences().len() >= 2, "a figure repeats");
            let mut seen = std::collections::BTreeSet::new();
            for occurrence in figure.occurrences() {
                let hull = occurrence.hull();
                prop_assert!(hull.is_some(), "an occurrence with no words");
                let hull = hull.unwrap();
                prop_assert!(
                    lines.iter().any(|line| line.contains(hull)),
                    "an occurrence outside every line"
                );
                for word in occurrence.words() {
                    prop_assert!(word.text(poem.source()).is_some());
                }
                prop_assert!(seen.insert(hull), "duplicate occurrence");
            }
        }
    }

    /// Every occurrence of a figure really is the same words.
    #[test]
    fn figure_occurrences_share_their_text(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = repetition::analyse(&poem);
        for figure in rep.figures() {
            // Word by word, so punctuation between the words cannot hide a difference.
            let texts: Vec<Vec<String>> = figure
                .occurrences()
                .iter()
                .map(|o| {
                    o.words()
                        .iter()
                        .filter_map(|w| w.text(poem.source()))
                        .map(str::to_lowercase)
                        .collect()
                })
                .collect();
            prop_assert_eq!(texts.len(), figure.occurrences().len());
            for text in &texts {
                prop_assert_eq!(text.len(), figure.words(), "wrong word count");
                prop_assert_eq!(text, &texts[0], "occurrences differ");
            }
        }
    }

    /// No two figures of the same kind cover the same set of lines.
    ///
    /// This is what stops one ostinato being reported at every shorter n — three lines sharing
    /// "and the tide" also share "and the" and "and", and reporting all three would triple a
    /// single figure. Nested figures over *different* line-sets are fine and expected.
    #[test]
    fn a_figure_is_reported_once_per_set_of_lines(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = repetition::analyse(&poem);
        let lines: Vec<Span> = rep
            .stanzas()
            .iter()
            .flat_map(prosody_core::repetition::StanzaRepetition::lines)
            .map(prosody_core::repetition::LineRepetition::line)
            .collect();

        let mut seen: std::collections::BTreeSet<(FigureKind, Vec<Span>)> =
            std::collections::BTreeSet::new();
        for figure in rep.figures() {
            let mut covered: Vec<Span> = figure
                .occurrences()
                .iter()
                .filter_map(|o| {
                    let hull = o.hull()?;
                    lines.iter().copied().find(|line| line.contains(hull))
                })
                .collect();
            covered.sort_unstable();
            covered.dedup();
            prop_assert!(
                seen.insert((figure.kind(), covered)),
                "the same lines reported twice under one kind"
            );
        }
    }

    /// A group of identical lines is a refrain, and never also an anaphora or epistrophe.
    ///
    /// §4.3 gives them different musical results — a literal loop against a downbeat-anchored
    /// figure — so reporting both would turn one repetition into two.
    #[test]
    fn identical_lines_are_a_refrain_and_nothing_else(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        let rep = repetition::analyse(&poem);
        let line_text = |span: Span| span.text(poem.source()).map(str::to_lowercase);

        for figure in rep.figures() {
            if figure.kind() == FigureKind::Refrain {
                continue;
            }
            let lines: Vec<Option<String>> = rep
                .stanzas()
                .iter()
                .flat_map(prosody_core::repetition::StanzaRepetition::lines)
                .filter(|line| {
                    figure
                        .occurrences()
                        .iter()
                        .filter_map(prosody_core::repetition::Occurrence::hull)
                        .any(|hull| line.line().contains(hull))
                })
                .map(|line| line_text(line.line()))
                .collect();
            prop_assert!(
                lines.windows(2).any(|pair| pair.first() != pair.last()),
                "identical lines reported as {:?}",
                figure.kind()
            );
        }
    }

    /// I1 — repetition analysis is a pure function of the poem.
    #[test]
    fn i1_repetition_analysis_is_reproducible(source in repetitive_poem()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(repetition::analyse(&poem), repetition::analyse(&poem));
    }

    /// Class membership means what it says: lines sharing a class actually rhyme, and a line
    /// alone in its class rhymes with nothing.
    ///
    /// The partition property below is about *shape*, and is satisfied by obviously-wrong
    /// implementations — one class for everything, or a class per line, are both partitions.
    /// This is the one that pins content, and it needs a phonemiser whose words do not all
    /// rhyme with each other.
    #[test]
    fn rhyme_classes_mean_what_they_claim(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let phonemiser = EverySyllableStressed;
        let rhymes = rhyme::analyse(&poem, &phonemiser);

        // Re-derive each rhyming pronunciation from the source, so this does not simply reuse
        // whatever the module recorded.
        let mut placed: Vec<(usize, prosody_core::Pronunciation)> = Vec::new();
        for stanza in rhymes.stanzas() {
            for line in stanza.lines() {
                let (Some(class), Some(word)) = (line.class(), line.word()) else {
                    prop_assert_eq!(line.score(), None, "a line with no word has no score");
                    continue;
                };
                let text = word.text(poem.source()).unwrap();
                placed.push((class.get(), phonemiser.pronounce(text).unwrap()));
            }
        }

        for (index, (class, pronunciation)) in placed.iter().enumerate() {
            let mates: Vec<&prosody_core::Pronunciation> = placed
                .iter()
                .enumerate()
                .filter(|(other, (their, _))| *other != index && their == class)
                .map(|(_, (_, p))| p)
                .collect();

            if let Some(best) = mates.iter().map(|m| rhyme::score(pronunciation, m)).max() {
                prop_assert!(
                    best >= rhyme::RHYME_THRESHOLD,
                    "lines share a class without rhyming"
                );
            } else {
                // Alone in its class, so nothing placed *before* it rhymed with it — otherwise
                // it would have joined that class. A *later* line may still rhyme with it and
                // yet belong elsewhere, having found a class it matches better; that is the
                // greedy rule working, not a violation.
                for (_, earlier) in placed.iter().take(index) {
                    prop_assert!(
                        rhyme::score(pronunciation, earlier) < rhyme::RHYME_THRESHOLD,
                        "founded a class despite rhyming with an earlier line"
                    );
                }
            }
        }
    }

    /// Rhyme classes are a partition: every line has at most one class, and the classes a poem
    /// uses are numbered contiguously from zero — which is what lets `SPEC.md` §4.5 hand each one
    /// the next scale degree.
    #[test]
    fn rhyme_classes_form_a_partition(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let rhymes = rhyme::analyse(&poem, &EverySyllableStressed);

        let mut seen = std::collections::BTreeSet::new();
        for stanza in rhymes.stanzas() {
            for line in stanza.lines() {
                if let Some(class) = line.class() {
                    prop_assert!(class.get() < rhymes.class_count());
                    seen.insert(class.get());
                }
            }
        }
        prop_assert_eq!(seen.len(), rhymes.class_count());
        for (expected, actual) in seen.iter().enumerate() {
            prop_assert_eq!(expected, *actual);
        }
    }

    /// I4 — a rhyme names the word it rests on, and that word lies inside its line.
    #[test]
    fn i4_every_rhyme_traces_to_a_word(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let rhymes = rhyme::analyse(&poem, &EverySyllableStressed);
        for stanza in rhymes.stanzas() {
            for line in stanza.lines() {
                if let Some(word) = line.word() {
                    prop_assert!(word.text(poem.source()).is_some());
                    prop_assert!(line.line().contains(word));
                    prop_assert!(line.class().is_some(), "a rhyming word implies a class");
                } else {
                    prop_assert!(line.class().is_none(), "no word means no class");
                    prop_assert!(line.tail().is_empty());
                }
            }
        }
    }

    /// I1 — rhyme analysis is a pure function of the poem and the phonemiser.
    #[test]
    fn i1_rhyme_analysis_is_reproducible(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(
            rhyme::analyse(&poem, &EverySyllableStressed),
            rhyme::analyse(&poem, &EverySyllableStressed)
        );
    }

    /// Scoring is symmetric and reflexive — the two halves of an equivalence relation a
    /// score-based rhyme test can honestly promise. Transitivity it cannot, which is why classes
    /// are built greedily rather than by closure.
    #[test]
    fn rhyme_scoring_is_symmetric_and_reflexive(a in "[a-z]{1,8}", b in "[a-z]{1,8}") {
        let phonemiser = EverySyllableStressed;
        let (Ok(x), Ok(y)) = (phonemiser.pronounce(&a), phonemiser.pronounce(&b)) else {
            return Ok(());
        };
        prop_assert_eq!(rhyme::score(&x, &y), rhyme::score(&y, &x));
        prop_assert!(rhyme::score(&x, &x).is_perfect());
    }

    /// I4 — every syllable of a line belongs to exactly one foot, and no foot reaches past the
    /// line. A segmentation that dropped or double-counted a syllable would change the rhythm
    /// with nothing to trace it to.
    #[test]
    fn i4_feet_partition_a_line_exactly(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let scanned = meter::scan(&poem, &EverySyllableStressed);
        for stanza in scanned.stanzas() {
            for line in stanza.lines() {
                let mut next = 0;
                for foot in line.feet() {
                    prop_assert_eq!(foot.beats().start, next, "feet must be contiguous");
                    prop_assert!(foot.beats().end > foot.beats().start, "a foot has syllables");
                    prop_assert_eq!(
                        foot.beats().len(),
                        foot.kind().syllables(),
                        "a foot's width must match its kind"
                    );
                    prop_assert!(line.span().contains(foot.span()));
                    next = foot.beats().end;
                }
                prop_assert_eq!(next, line.beats().len(), "feet must cover every syllable");
                prop_assert_eq!(line.syllable_count().get(), line.beats().len());
                prop_assert_eq!(line.foot_count().get(), line.feet().len());
            }
        }
    }

    /// I4 — every syllable names the word it came from, and that word lies inside its line.
    #[test]
    fn i4_every_syllable_traces_to_a_word(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let scanned = meter::scan(&poem, &EverySyllableStressed);
        for stanza in scanned.stanzas() {
            for line in stanza.lines() {
                for beat in line.beats() {
                    prop_assert!(beat.word.text(poem.source()).is_some());
                    prop_assert!(line.span().contains(beat.word));
                }
                for span in line.unpronounced() {
                    prop_assert!(line.span().contains(*span));
                }
            }
        }
    }

    /// I1 — scanning is a pure function of the poem and the phonemiser.
    #[test]
    fn i1_scanning_is_reproducible(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(
            meter::scan(&poem, &EverySyllableStressed),
            meter::scan(&poem, &EverySyllableStressed)
        );
    }

    /// Variance is a total order, so role assignment can rank stanzas without a tie-break that
    /// depends on float rounding — and the order tracks the *value*, not the representation.
    ///
    /// The axioms alone are not enough: an `Ord` that compared numerators and ignored denominators
    /// is still a valid total order, and passed an earlier version of this test while defeating
    /// the whole point of the exact arithmetic. The duplication case below is what catches it.
    #[test]
    fn variance_is_a_total_order(
        a in proptest::collection::vec(0_usize..20, 0..8),
        b in proptest::collection::vec(0_usize..20, 0..8),
    ) {
        let (x, y) = (meter::Variance::of(&a), meter::Variance::of(&b));
        prop_assert_eq!(x.cmp(&y), y.cmp(&x).reverse());
        prop_assert_eq!(x.cmp(&x), core::cmp::Ordering::Equal);

        // Equality must agree with ordering, or `Ord`'s contract is broken.
        prop_assert_eq!(x == y, x.cmp(&y) == core::cmp::Ordering::Equal);

        // Repeating a set leaves its spread unchanged but scales the stored fraction, so this
        // fails for any comparison that reads the numerator without the denominator.
        let doubled: Vec<usize> = a.iter().chain(a.iter()).copied().collect();
        prop_assert_eq!(meter::Variance::of(&doubled), x);
        prop_assert_eq!(meter::Variance::of(&doubled).cmp(&y), x.cmp(&y));

        // A set of identical values has no spread at all, whatever the value is.
        if a.iter().all(|v| Some(v) == a.first()) {
            prop_assert!(x.is_zero());
        }
    }

    /// Relative variance is invariant under scaling every line's length, which raw variance is
    /// not — the property `SPEC.md` §4.1 needs when stanzas differ in line length.
    #[test]
    fn relative_variance_ignores_scale(
        counts in proptest::collection::vec(1_usize..12, 1..6),
        factor in 2_usize..5,
    ) {
        let scaled: Vec<usize> = counts.iter().map(|c| c * factor).collect();
        prop_assert_eq!(
            meter::Variance::relative(&scaled),
            meter::Variance::relative(&counts)
        );
    }

    /// I4, for sound rather than text — syllabification attributes every phoneme exactly once,
    /// in order. A pass that lost or reordered a phoneme would make every syllable count, stress
    /// vector and rhyme derived from it untraceable to what the poet wrote.
    #[test]
    fn i4_syllabification_conserves_every_phoneme(phones in phone_sequence()) {
        let expected: Vec<Phoneme> = phones.iter().map(|(p, _)| *p).collect();
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            // The only refusal for a single-inventory sequence is having no vowel.
            prop_assert!(!expected.iter().any(|p| p.is_vowel()));
            return Ok(());
        };

        let got: Vec<Phoneme> = syllables.iter().flat_map(prosody_core::Syllable::phonemes).collect();
        prop_assert_eq!(got, expected.clone());
        prop_assert_eq!(
            syllables.len(),
            expected.iter().filter(|p| p.is_vowel()).count()
        );
    }

    /// Every interior onset is legal *and* maximal, so the boundary is pinned rather than merely
    /// permitted. Stated over arbitrary sequences, not just the ones English happens to use.
    #[test]
    fn syllable_boundaries_are_legal_and_maximal(phones in phone_sequence()) {
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            return Ok(());
        };
        for index in 1..syllables.len() {
            let onset = syllables[index].onset();
            prop_assert!((ARPABET.legal_onset)(onset));

            if let Some(&borrowed) = syllables[index - 1].coda().last() {
                let mut longer = vec![borrowed];
                longer.extend_from_slice(onset);
                prop_assert!(
                    !(ARPABET.legal_onset)(&longer),
                    "onset is not maximal"
                );
            }
        }
    }

    /// Every syllable holds exactly one vowel, in its nucleus.
    #[test]
    fn syllables_hold_one_vowel_each(phones in phone_sequence()) {
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            return Ok(());
        };
        for syllable in &syllables {
            prop_assert!(syllable.nucleus().is_vowel());
            prop_assert!(syllable.onset().iter().all(|p| !p.is_vowel()));
            prop_assert!(syllable.coda().iter().all(|p| !p.is_vowel()));
        }
    }

    /// I1 — syllabification is a pure function of its input.
    #[test]
    fn i1_syllabification_is_reproducible(phones in phone_sequence()) {
        prop_assert_eq!(phoneme::syllabify(&phones), phoneme::syllabify(&phones));
    }

    /// I1 — derivation is a pure function of its inputs, so repeated runs agree, and the
    /// generator built from a seed replays the same stream.
    #[test]
    fn i1_seed_derivation_is_reproducible(domain in domain(), content in poem_text()) {
        let first = seed_of(domain, &content);
        let second = seed_of(domain, &content);
        prop_assert_eq!(first, second);
        prop_assert_eq!(draw(&first, 32), draw(&second, 32));
    }

    /// I2 — a unit's seed depends on the unit's own bytes and on nothing about where those bytes
    /// sit. The same stanza text embedded at different offsets in different poems must seed
    /// identically; if it did not, an edit anywhere before a stanza would move that stanza's
    /// voice, which is precisely the locality failure I2 exists to prevent.
    ///
    /// This is the property that the `Span` parameter of `Seed::for_unit` makes testable: the
    /// call site has to name its bounds, so a test can vary everything outside them.
    #[test]
    fn i2_a_unit_seeds_the_same_wherever_it_sits(
        domain in domain(),
        unit in poem_text(),
        before in poem_text(),
        after in poem_text(),
    ) {
        let alone = seed_of(domain, &unit);

        let embedded = format!("{before}{unit}{after}");
        let span = Span::new(before.len(), before.len() + unit.len());
        // The surrounding text may end mid-character; only a span on real boundaries resolves.
        prop_assume!(span.text(&embedded) == Some(unit.as_str()));

        prop_assert_eq!(Seed::for_unit(domain, span, &embedded).unwrap(), alone);
    }

    /// I2 — widening a span past the unit changes the seed. This is the mistake the old `&str`
    /// API could not distinguish from correct use: passing the whole poem where a stanza was
    /// meant stays deterministic, so only a locality property like this one catches it.
    #[test]
    fn i2_widening_a_span_changes_the_seed(
        unit in poem_text(),
        extra in poem_text(),
    ) {
        prop_assume!(!extra.is_empty());
        let source = format!("{unit}{extra}");
        let narrow = Span::new(0, unit.len());
        let wide = Span::new(0, source.len());
        prop_assume!(narrow.text(&source).is_some());

        prop_assert_ne!(
            Seed::for_unit(Domain::Stanza, narrow, &source).unwrap(),
            Seed::for_unit(Domain::Stanza, wide, &source).unwrap()
        );
    }

    /// I2 — distinct unit content gives distinct seeds.
    #[test]
    fn i2_distinct_content_gives_distinct_seeds(
        domain in domain(),
        a in poem_text(),
        b in poem_text(),
    ) {
        // `prop_assume!` rather than an `if`, so proptest tracks how often the case is vacuous
        // instead of silently asserting nothing.
        prop_assume!(a != b);
        prop_assert_ne!(seed_of(domain, &a), seed_of(domain, &b));
    }

    /// I2 — the two domains never collide for the same text.
    #[test]
    fn i2_domains_stay_separated(content in poem_text()) {
        prop_assert_ne!(seed_of(Domain::Stanza, &content), seed_of(Domain::Line, &content));
    }

    /// I2 — named sub-streams within a unit are independent, and asking for one does not disturb
    /// another.
    #[test]
    fn i2_child_seeds_are_independent(
        content in poem_text(),
        left in label(),
        right in label(),
    ) {
        let parent = seed_of(Domain::Stanza, &content);
        prop_assert_ne!(parent.child(left), parent);

        if left == right {
            prop_assert_eq!(parent.child(left), parent.child(right));
        } else {
            prop_assert_ne!(parent.child(left), parent.child(right));
            prop_assert_ne!(draw(&parent.child(left), 8), draw(&parent.child(right), 8));
        }
    }

    /// I4 — a span resolves to exactly the bytes it claims, or declines for a reason.
    ///
    /// Both halves matter. Without the `None` arm, an implementation that never resolved
    /// anything would pass; without the offset check, one that returned a correctly-sized slice
    /// from the wrong position would pass. Offsets are drawn near the source length so that
    /// in-bounds, boundary and out-of-bounds cases all actually occur — drawing them from the
    /// whole `usize` range would make the resolving case vanishingly rare.
    #[test]
    fn i4_spans_resolve_to_their_claimed_bytes_or_decline(
        source in poem_text(),
        start in 0_usize..=MAX_OFFSET,
        end in 0_usize..=MAX_OFFSET,
    ) {
        let span = Span::new(start, end);
        match span.text(&source) {
            Some(text) => {
                prop_assert_eq!(text.len(), span.len());
                // The slice must begin at the offset the span claims, not merely occur somewhere
                // in the source.
                let offset = text.as_ptr().addr() - source.as_ptr().addr();
                prop_assert_eq!(offset, span.start());
            }
            None => {
                // Declining is only allowed for a span that genuinely cannot be sliced.
                prop_assert!(
                    span.end() > source.len()
                        || !source.is_char_boundary(span.start())
                        || !source.is_char_boundary(span.end()),
                    "declined a span that was in bounds and on character boundaries"
                );
            }
        }
    }

    /// I4 — a node's span covers its children's spans, which is what makes attribution
    /// transitive: if an event traces to a token, it traces to the line containing that token.
    #[test]
    fn i4_join_covers_both_operands(
        a_start in 0_usize..200,
        a_len in 0_usize..50,
        b_start in 0_usize..200,
        b_len in 0_usize..50,
    ) {
        let a = Span::new(a_start, a_start + a_len);
        let b = Span::new(b_start, b_start + b_len);
        let joined = a.join(b);

        prop_assert!(joined.contains(a));
        prop_assert!(joined.contains(b));
        prop_assert_eq!(joined, b.join(a));
        prop_assert!(joined.len() >= a.len().max(b.len()));
    }

    /// I2, the property this whole design exists for — editing one stanza must not disturb any
    /// other stanza's voice.
    ///
    /// A randomly chosen token in a randomly chosen line of stanza *i* is replaced. Every stanza
    /// *j ≠ i* keeps a byte-identical seed, even though the edit shifts their byte offsets: seeds
    /// follow text, not position. Stanza *i*'s own seed must change, or the edit would be
    /// inaudible and the test vacuous.
    #[test]
    fn i2_an_edit_moves_only_its_own_stanza(
        source in poem_source(),
        pick in 0_usize..4096,
        text in replacement(),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        let stanza_count = poem.stanzas().len();
        let target = pick % stanza_count;

        let lines = poem.stanzas()[target].lines();
        let line = (pick / 7) % lines.len();
        let tokens = lines[line].tokens();
        let token = (pick / 13) % tokens.len();
        let span = tokens[token].span();
        prop_assume!(span.text(poem.source()) != Some(text));

        let before = stanza_seeds(&poem);
        poem.apply(span, text).unwrap();

        // Edits that add or remove a stanza break change the stanza list itself; those are
        // covered by the reparse property below, where index-wise comparison still makes sense.
        prop_assume!(poem.stanzas().len() == stanza_count);

        let after = stanza_seeds(&poem);
        prop_assert_eq!(after.len(), before.len());
        for index in 0..before.len() {
            if index == target {
                prop_assert_ne!(before[index], after[index], "edited stanza kept its seed");
            } else {
                prop_assert_eq!(before[index], after[index], "stanza {} moved", index);
            }
        }
    }

    /// I2 — appending or removing material *after* a stanza must not change that stanza's seed.
    ///
    /// This is the case a raw tree-sitter node range gets wrong: the trailing newline of a
    /// stanza's last line belongs to the stanza when it is terminal and to the stanza break once
    /// something follows, so the extent — and the seed — would move without the text moving.
    #[test]
    fn i2_appending_a_stanza_leaves_earlier_stanzas_alone(
        source in poem_source(),
        addition in poem_source(),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        let before = stanza_seeds(&poem);
        let before_spans: Vec<Span> = poem.stanzas().iter().map(prosody_core::Stanza::span).collect();

        let end = Span::new(poem.source().len(), poem.source().len());
        poem.apply(end, &format!("\n{addition}")).unwrap();

        let after = stanza_seeds(&poem);
        prop_assert!(after.len() >= before.len());
        for index in 0..before.len() {
            prop_assert_eq!(before_spans[index], poem.stanzas()[index].span());
            prop_assert_eq!(before[index], after[index]);
        }
    }

    /// I4 — every node the parser produces resolves to real source text, and the tree nests:
    /// tokens inside lines inside stanzas.
    #[test]
    fn i4_every_parsed_node_resolves_and_nests(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        for stanza in poem.stanzas() {
            prop_assert!(stanza.span().text(poem.source()).is_some());
            for line in stanza.lines() {
                prop_assert!(line.span().text(poem.source()).is_some());
                prop_assert!(stanza.span().contains(line.span()));
                for token in line.tokens() {
                    prop_assert!(token.span().text(poem.source()).is_some());
                    prop_assert!(line.span().contains(token.span()));
                    prop_assert!(!token.span().is_empty());
                }
            }
        }
    }

    /// I4 — tokenisation loses nothing. Every non-whitespace byte of a line is covered by exactly
    /// one token.
    ///
    /// Nesting and resolution say only that the tokens which exist are well-formed; they would
    /// happily accept a tokeniser that dropped half its output. This says the text is *fully*
    /// attributed, which is what invariant I4 actually claims.
    #[test]
    fn i4_tokens_cover_every_non_whitespace_byte(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        for stanza in poem.stanzas() {
            for line in stanza.lines() {
                let mut covered = vec![false; line.span().len()];
                for token in line.tokens() {
                    for offset in token.span().range() {
                        let local = offset - line.span().start();
                        prop_assert!(!covered[local], "byte {} covered twice", offset);
                        covered[local] = true;
                    }
                }
                let text = line.span().text(poem.source()).unwrap();
                for (local, byte) in text.bytes().enumerate() {
                    if !byte.is_ascii_whitespace() {
                        prop_assert!(covered[local], "byte {local} of {text:?} has no token");
                    }
                }
            }
        }
    }

    /// I1 — an incremental reparse lands in exactly the same place as parsing the edited text
    /// from scratch, for every shape of edit including ones that insert newlines and change the
    /// stanza count. If these disagreed, output would depend on a poem's edit history rather
    /// than on the poem.
    #[test]
    fn i1_incremental_reparse_matches_a_fresh_parse(
        source in poem_source(),
        pick in 0_usize..4096,
        text in replacement(),
    ) {
        let mut incremental = Poem::parse(source).unwrap();
        let spans = token_spans(&incremental);
        let span = spans[pick % spans.len()];

        incremental.apply(span, text).unwrap();

        let fresh = Poem::parse(incremental.source().to_owned()).unwrap();
        prop_assert_eq!(incremental.stanzas(), fresh.stanzas());
        prop_assert_eq!(stanza_seeds(&incremental), stanza_seeds(&fresh));
        prop_assert_eq!(incremental.diagnostics(), fresh.diagnostics());
    }

    /// I1 — a chain of incremental edits is equivalent to parsing the final text once. A single
    /// edit can hide a stale-tree bug that only shows up on the edit after it.
    #[test]
    fn i1_chained_edits_match_a_fresh_parse(
        source in poem_source(),
        picks in proptest::collection::vec((0_usize..4096, replacement()), 1..5),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        for (pick, text) in picks {
            let spans = token_spans(&poem);
            if spans.is_empty() {
                break;
            }
            poem.apply(spans[pick % spans.len()], text).unwrap();
        }
        let fresh = Poem::parse(poem.source().to_owned()).unwrap();
        prop_assert_eq!(poem.stanzas(), fresh.stanzas());
        prop_assert_eq!(stanza_seeds(&poem), stanza_seeds(&fresh));
    }

    /// I4 — containment is reflexive and transitive, so a token inside a line inside a stanza is
    /// attributable to that stanza.
    #[test]
    fn i4_containment_is_transitive(
        outer_start in 0_usize..100,
        outer_len in 0_usize..100,
        inner_offset in 0_usize..100,
        inner_len in 0_usize..100,
        leaf_offset in 0_usize..100,
        leaf_len in 0_usize..100,
    ) {
        let outer = Span::new(outer_start, outer_start + outer_len);
        prop_assert!(outer.contains(outer));

        let inner = Span::new(outer_start + inner_offset, outer_start + inner_offset + inner_len);
        let leaf = Span::new(inner.start() + leaf_offset, inner.start() + leaf_offset + leaf_len);

        if outer.contains(inner) && inner.contains(leaf) {
            prop_assert!(outer.contains(leaf));
        }
    }
}
