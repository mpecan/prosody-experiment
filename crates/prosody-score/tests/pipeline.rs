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
use prosody_score::{Profile, measure, measure_as, sensitivity};

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
    // `da dA da dA`: two iambs, one beat each, accents on both offbeats — played twice, since
    // the outro repeats the last stanza (§4.6). Per bar, the first accent is followed by the
    // unaccented second beat, one level stronger: one unit. The second accent of bar 0 is followed
    // by bar 1's unaccented downbeat, two levels stronger; that of bar 1 by nothing. 1 + 2 + 1 + 0
    // units over four beats: 1.0 — the metrics module's `iambs(2)`, worked the same way.
    let score = compile("da dA da dA\n").unwrap();
    assert!((value(&score, "syncopation") - 1.0).abs() < 1e-9);
    // One line is one voice. Each beat's `dA` is pitched (the bass, below), and a pitched event
    // sounds the line's drum *and* the synth: two instruments a beat. In the outro both return:
    // 2 of the 4 (bar, instrument) pairs.
    assert!((value(&score, "layering") - 2.0).abs() < 1e-9);
    assert!((value(&score, "recurrence") - 0.5).abs() < 1e-9);
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
    // stanza; {0, synth} again in slot 3, the outro, its lane silent under its own voice. Six of
    // the nine bring something back.
    let score = compile("da dA da dA\nsa mA sa mA\n\nda dA da dA\n").unwrap();
    assert!((value(&score, "recurrence") - 6.0 / 9.0).abs() < 1e-9);
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

/// The same poem under the bass and harmony variants given.
fn compile_with(source: &str, bass: &str, harmony: &str) -> Score {
    let mappings = prosody_core::Mappings::all()
        .with(prosody_core::Mapping::Bass.choose_named(bass).unwrap())
        .with(
            prosody_core::Mapping::Harmony
                .choose_named(harmony)
                .unwrap(),
        );
    let poem = Poem::parse(source.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Toy);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &Toy),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &Toy),
            surprisal: &surprisal::analyse(&poem, &Toy, &Flat),
            roles: &roles,
        },
        &mappings,
    )
    .unwrap()
}

fn metric(score: &Score, name: &str) -> f64 {
    measure(score)
        .into_iter()
        .find(|metric| metric.name() == name)
        .unwrap()
        .value()
}

#[test]
fn a_walking_bass_is_pitched_but_its_chords_root_adds_no_entropy() {
    // AABB of two-iamb lines, ending O, O, E, E: O is degree 0, E degree 1. Every line is
    // `xa xA xa xR`, four syllables. AABB, not ABAB: each open rhyme is answered in the very next
    // bar, so nothing is held under another line, and every placement is its bar's foundation line
    // — the one whose walk sounds (`Score::sounds_bass`, one walk a bar).
    let source = "da dA da dO\nma mA ma mO\nta tA ta tE\nsa sA sa sE\n";
    // Hand-worked shares of pitched events, the same in every line and so in the whole piece:
    // the height bass pitches the stressed `xA` and the melody the rhyme word — 2 of 4. W2 pitches
    // each foot's first syllable, both `xa`, and the melody the rhyme word — 3 of 4.
    let height = compile(source).unwrap();
    let feet = compile_with(source, "feet", "off");
    assert!((metric(&height, "pitched") - 0.5).abs() < 1e-12);
    assert!((metric(&feet, "pitched") - 0.75).abs() < 1e-12);
    // Under H1 the E lines' bars walk the same steps from degree 1 instead of 0. The root is the
    // harmony's, which pitch entropy leaves out (the step-5 review fix), so the entropy is the same
    // with the chords as without them.
    let rhymed = compile_with(source, "feet", "rhyme");
    assert!(rhymed.chords().next().is_some());
    assert!((metric(&feet, "pitch_entropy") - metric(&rhymed, "pitch_entropy")).abs() < 1e-12);
}

#[test]
fn a_line_held_under_another_keeps_its_melody_but_not_its_walk() {
    // ABAB of two-iamb lines, ending O, E, O, E. The arrangement, by hand: the stanza is stated in
    // slots 0-3, the A line held under slot 1 (its answer comes in slot 2) and the B line under
    // slot 2 (answered in slot 3), and the loop plays one more pass, slots 4-7, with every rhyme
    // already answered — 8 whole lines and 2 held ones, 40 syllables.
    //
    // The height bass sounds under a held line too, as it always has: each line pitches its
    // stressed `xA` and its rhyme word, 2 of 4, so 20 of 40. W2 pitches each foot's first `xa` and
    // the rhyme word, 3 of 4 — but one walk a bar (`Score::sounds_bass`): a held line keeps its
    // melody note and drops its walk, so 8 × 3 + 2 × 1 = 26 of 40, not the 30 it was when every
    // line walked at once.
    let source = "da dA da dO\nma mA ma mE\nta tA ta tO\nsa sA sa sE\n";
    let height = compile(source).unwrap();
    let feet = compile_with(source, "feet", "off");
    let slots = feet.arrangement().slots();
    assert_eq!(slots.len(), 8);
    assert_eq!(
        slots
            .iter()
            .map(|slot| slot.placements().len())
            .sum::<usize>(),
        10
    );
    assert!((metric(&height, "pitched") - 0.5).abs() < 1e-12);
    assert!((metric(&feet, "pitched") - 26.0 / 40.0).abs() < 1e-12);
}

#[test]
fn an_anapaest_held_under_an_iamb_crosses_its_beat() {
    // ABAB, the first A line opening on an anapaest: `da da dA | da dO` against the others'
    // `xa xA | xa xE`, two beats each. By hand, as in the test above: slots 0-3 state the stanza,
    // the A line held under slot 1 and the B line under slot 2, and the outro plays slots 4-7
    // with nothing held — 8 slots of 2 beats, 16 beats.
    //
    // Only slot 1 has two lines with different feet in one beat: its host's first foot divides
    // beat 0 in two, the held A line's anapaest in three. One crossed beat of 16. Slot 2's two
    // lines are both iambs. Nothing else in the piece crosses.
    let source = "da da dA da dO\nma mA ma mE\nta tA ta tO\nsa sA sa sE\n";
    let score = compile(source).unwrap();
    let cross = measure_as(&score, Profile::Jazz)
        .into_iter()
        .find(|metric| metric.name() == "cross_rhythm")
        .unwrap();
    assert!(
        (cross.value() - 1.0 / 16.0).abs() < 1e-12,
        "{}",
        cross.value()
    );
    // Measured by the jazz profile only; the machine profile and the generic one are unchanged.
    for profile in [Profile::Interest, Profile::Machine] {
        assert!(
            measure_as(&score, profile)
                .iter()
                .all(|metric| metric.name() != "cross_rhythm")
        );
    }
}
