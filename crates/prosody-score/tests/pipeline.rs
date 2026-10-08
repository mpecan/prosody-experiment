//! The metrics and the edit test, through the real lowering passes.
//!
//! The phonemiser here is a toy whose every stress the test chose: one syllable per vowel letter,
//! lower case unstressed and upper case primary, so `da dA` is an iamb and `dA da` a trochee by construction.
//! Any letter it does not know is silent, which gives the edit test a way to make a word sound
//! identical (`dAq`) or not at all (`q`).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use prosody_core::lower::{self, Analyses};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_score::{measure, sensitivity};

struct Toy;

impl Phonemiser for Toy {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let phones: Vec<_> = word
            .chars()
            .filter_map(|letter| {
                let (symbol, stress) = match letter {
                    'a' => ("AH", Stress::Unstressed),
                    'A' => ("AA", Stress::Primary),
                    'o' => ("AO", Stress::Unstressed),
                    'O' => ("OW", Stress::Primary),
                    'e' => ("IH", Stress::Unstressed),
                    'E' => ("EH", Stress::Primary),
                    'I' => ("IY", Stress::Primary),
                    'd' => ("D", Stress::Unstressed),
                    's' => ("S", Stress::Unstressed),
                    'm' => ("M", Stress::Unstressed),
                    't' => ("T", Stress::Unstressed),
                    _ => return None,
                };
                Some((ARPABET.phoneme(symbol).unwrap(), stress))
            })
            .collect();
        if phones.is_empty() {
            return Err(Unpronounceable::NotKnown);
        }
        Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
    }
}

struct Flat;

impl Lexicon for Flat {
    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }
}

fn compile(source: &str) -> Option<Score> {
    let poem = Poem::parse(source.to_owned()).ok()?;
    let meter = meter::scan(&poem, &Toy);
    let rhyme = rhyme::analyse(&poem, &Toy);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, &Toy);
    let surprisal = surprisal::analyse(&poem, &Toy, &Flat);
    let roles = role::assign(&meter, &repetition).ok()?;
    lower::run(Analyses {
        poem: &poem,
        meter: &meter,
        rhyme: &rhyme,
        repetition: &repetition,
        sound: &sound,
        surprisal: &surprisal,
        roles: &roles,
    })
    .ok()
}

fn value(score: &Score, name: &str) -> f64 {
    measure(score)
        .into_iter()
        .find(|metric| metric.name() == name)
        .unwrap()
        .value()
}

#[test]
fn a_line_of_iambs_measures_as_worked_by_hand() {
    // `da dA da dA`: two iambs, one beat each, accents on both offbeats. The first accent is
    // followed by the unaccented second beat — one level stronger — and the second by nothing.
    // One unit over two beats.
    let score = compile("da dA da dA\n").unwrap();
    assert!((value(&score, "syncopation") - 0.5).abs() < 1e-9);
    // One line is one voice, and nothing returns. But each beat's `dA` is pitched (the bass,
    // below), and a pitched event sounds the line's drum *and* the synth: two instruments a beat.
    assert!((value(&score, "layering") - 2.0).abs() < 1e-9);
    assert!(value(&score, "recurrence").abs() < 1e-9);
    // A rhyme class of one is no melodic relation, so no rhyme note (PLAN.md, free verse). But
    // the one line is the foundation, so its two stressed `dA`s carry the bass: 2 of 4 events.
    assert!((value(&score, "pitched") - 0.5).abs() < 1e-9);
}

#[test]
fn trochees_do_not_syncopate() {
    let score = compile("dA da dA da\n").unwrap();
    assert!(value(&score, "syncopation").abs() < 1e-9);
}

#[test]
fn a_refrain_is_a_returning_voice() {
    // §4.3: a repeated line is the same instrument again. Bars 0 and 2 share a voice, and once
    // bar 0 is heard it loops in the refrain lane under bar 1 (§4.6). Every bar also has a
    // pitched `dA` or `mA` (rhyme or bass), so the synth sounds in each. The (bar, instrument)
    // pairs: {0, synth} in slot 0; {1, 0, synth} in slot 1; {0, synth} in slot 2, a refrain
    // stanza. Four of the seven bring something back.
    let score = compile("da dA da dA\nsa mA sa mA\n\nda dA da dA\n").unwrap();
    assert!((value(&score, "recurrence") - 4.0 / 7.0).abs() < 1e-9);
}

#[test]
fn an_edit_to_a_word_that_sounds_the_same_is_inaudible() {
    // `dAq` is pronounced exactly like `dA`, and `daq` like `da`, so every edit leaves the music
    // as it was. A measure that called these audible would be measuring the text, not the sound.
    let result = sensitivity::measure("da dA\n", &["daq", "dAq"], compile).unwrap();
    assert_eq!(result.edits(), 2);
    assert_eq!(result.audible(), 0);
    assert_eq!(result.local(), 2);
    assert_eq!(result.explained(), 2);
}

#[test]
fn silencing_a_word_reaches_only_what_sounds_under_it() {
    // `q` has no pronunciation, so each edit silences one syllable of its bar — heard there. The
    // stanzas share no vowel, so nothing rhymes across them.
    //
    // But stanza 1 (foundation) loops under stanza 2 (flourish), fitted to stanza 2's bar. Silencing
    // either stressed `mO` turns `so mO so mO` from two iambs into one foot, so the slot shrinks
    // from two beats to one and stanza 1's looped copy is squeezed into it: heard outside stanza 2,
    // so not local — and explained, because stanza 1 sounds under the edited line. Every edit to
    // stanza 1, and to stanza 2's unstressed `so`s, keeps every slot's length: local.
    let source = "da dA da dA\n\nso mO so mO\n";
    let result = sensitivity::measure(source, &["q"], compile).unwrap();
    assert_eq!(result.edits(), 8);
    assert_eq!(result.audible(), 8);
    assert_eq!(result.local(), 6);
    assert_eq!(result.explained(), 8);
    assert_eq!(result.failed(), 0);
}

#[test]
fn a_rhyme_across_stanzas_carries_an_edit_with_it() {
    // `dA` and `mA` rhyme (§4.5), so both are pitched. Silencing either leaves the other a rhyme
    // class of one, which is unpitched — a change in the *other* stanza. The two unstressed words
    // carry no rhyme and stay local — though silencing `da` does renumber stanza 2's word-family
    // ids, which are counted poem-wide. No backend plays those ids, so that is not heard and not a
    // leak. And one rhyme edit is local after all: one of the two lines is the foundation, so
    // when its partner falls silent its `A` keeps a note — the bass's open vowel, degree 0, the
    // same degree the rhyme gave it. Three of four are local — and all four are *explained*,
    // because the bars the rhyme edits reach are the bars that rhymed with them.
    let result = sensitivity::measure("da dA\n\nsa mA\n", &["q"], compile).unwrap();
    assert_eq!(result.edits(), 4);
    assert_eq!(result.audible(), 4);
    assert_eq!(result.local(), 3);
    assert_eq!(result.explained(), 4);
}

#[test]
fn breaking_a_refrain_is_heard_where_the_refrain_returns() {
    // Bars: 0 `da dA` · 1 `so mO` ‖ 2 `se mI` · 3 `da dA`. Bars 0 and 3 are one voice (§4.3's
    // refrain) and rhyme with each other; `mO` and `mI` rhyme with nothing. The probe is `tE`,
    // a stressed monosyllable whose vowel no other word has.
    //
    // - Bar 0 or bar 3, either word: the refrain splits, so the *other* copy's bar loses the
    //   partner it shared an instrument with. Heard across the stanza break — not local. Four.
    // - Bar 1 or bar 2, either word: no relation crosses the break. Voices after it may be
    //   renumbered, but no backend plays the number. Local. Four.
    //
    // Audibility: `so`/`se` → `tE` stresses an unstressed syllable, which is heard. `mO`/`mI` →
    // `tE` swaps a stressed, unrhymed monosyllable for another, so only the vowel differs — and
    // every line here is foundation, so the vowel reaches the bass by its height. `mI` (IY, close,
    // degree 4) → `tE` (EH, mid, degree 2) is heard. `mO` (OW, mid) → `tE` (mid) is not: the bass
    // hears height, and nothing else here carries a vowel under a flat lexicon. Seven.
    let source = "da dA\nso mO\n\nse mI\nda dA\n";
    let result = sensitivity::measure(source, &["tE"], compile).unwrap();
    assert_eq!(result.edits(), 8);
    assert_eq!(result.audible(), 7);
    assert_eq!(result.local(), 4);
    // The four that reached across the break reached the refrain's other copy — its own voice.
    assert_eq!(result.explained(), 8);
}

#[test]
fn a_degree_renumbering_is_an_unexplained_leak() {
    // `PLAN.md`: "a scale degree is numbered poem-wide, so adding a rhyme early transposes an
    // untouched stanza later." Stanza 1 rhymes `dA`/`mA` and takes degree 0; stanza 2 rhymes
    // `mI`/`tI` and takes degree 1. Break stanza 1's rhyme and stanza 2's class becomes the
    // first, so it moves to degree 0 — a stanza that shares nothing with the edit, transposed.
    //
    // Edits to `dA` or `mA` do that: not local, and not explained. The other six — the unstressed
    // words anywhere, and stanza 2's rhyme words, which come after stanza 1 and so cannot renumber
    // it — stay local.
    let source = "da dA\nso mA\n\nse mI\nte tI\n";
    let result = sensitivity::measure(source, &["tE"], compile).unwrap();
    assert_eq!(result.edits(), 8);
    assert_eq!(result.local(), 6);
    assert_eq!(result.explained(), 6);
}

#[test]
fn the_edit_test_is_deterministic() {
    let source = "da dA da dA\nsa mA da dA\n\ndA da mA sa\n";
    let first = sensitivity::measure(source, &["tAm", "q", "sa"], compile);
    let second = sensitivity::measure(source, &["tAm", "q", "sa"], compile);
    assert!(first.is_some());
    assert_eq!(first, second);
}

#[test]
fn no_probes_means_no_measurement() {
    assert_eq!(sensitivity::measure("da dA\n", &[], compile), None);
}
