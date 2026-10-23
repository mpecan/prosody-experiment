//! The bass (`SPEC.md` §4.6, `prosody_core::bass`), through the real passes, with hand-worked
//! walks for each variant.
//!
//! The phonemiser is a toy: `a` is an unstressed syllable (AH), and each capital a primary-stressed
//! one with its own vowel — `A` AA (open), `O` OW and `E` EH (mid), `U` UW and `I` IY (close) —
//! other letters silent. So a vowel's height, which is all the bass reads, can be written straight
//! into the poem: open is the root (step 0), mid the third (2), close the fifth (4).

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{
    BassReason, Chromatic, Direction, Layer, Poem, Velocity, meter, repetition, rhyme, role, sound,
    surprisal,
};

struct Toy;

impl Phonemiser for Toy {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let phones: Vec<_> = word
            .chars()
            .filter_map(|letter| match letter {
                'a' => Some((ARPABET.phoneme("AH").unwrap(), Stress::Unstressed)),
                'A' => Some((ARPABET.phoneme("AA").unwrap(), Stress::Primary)),
                'O' => Some((ARPABET.phoneme("OW").unwrap(), Stress::Primary)),
                'E' => Some((ARPABET.phoneme("EH").unwrap(), Stress::Primary)),
                'U' => Some((ARPABET.phoneme("UW").unwrap(), Stress::Primary)),
                'I' => Some((ARPABET.phoneme("IY").unwrap(), Stress::Primary)),
                _ => None,
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
    // No closed class: every word keeps its dictionary stress under either variant.
    fn is_function_word(&self, _: &str) -> bool {
        false
    }

    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }
}

fn compile(source: &str, bass: Variant, harmony: Variant) -> Score {
    let mappings = Mappings::all()
        .with(Mapping::Bass.choose(bass).unwrap())
        .with(Mapping::Harmony.choose(harmony).unwrap());
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

/// One bar's bass, a note a row: its step above its root, `-` for a semitone below it, and the
/// syllable it sounds on, as `beat.subdivision`. A syllable without a note is `·`.
fn walk(score: &Score, bar: usize) -> Vec<String> {
    score
        .events()
        .iter()
        .filter(|event| event.bar().get() == bar)
        .map(|event| {
            event.bass().map_or_else(
                || "·".to_owned(),
                |note| {
                    let below = match note.chromatic() {
                        Chromatic::Diatonic => "",
                        Chromatic::SemitoneBelow => "-",
                    };
                    format!("{}{below}", note.step().get())
                },
            )
        })
        .collect()
}

/// Every line a test uses is in a stanza of lines of one length, so foundation (§4.1).
fn assert_foundation(score: &Score) {
    for voice in score.voices() {
        assert_eq!(voice.layer(), Layer::Foundation);
    }
}

#[test]
fn height_is_the_default_and_leaves_rhyme_words_to_the_melody() {
    assert_eq!(Mappings::all().get(Mapping::Bass), Variant::Height);
    // `dA da da dU` twice: the stressed `dA` (open) is the root; `dU` is the rhyme word, so it
    // sounds §4.5's note and no bass, as the height bass always has.
    let score = compile("dA da da dU\nmA ma ma dU\n", Variant::Height, Variant::Off);
    assert_foundation(&score);
    assert_eq!(walk(&score, 0), vec!["0", "·", "·", "·"]);
}

#[test]
fn syllables_walk_from_one_stressed_note_to_the_next() {
    // W1, hand-worked. `dA da da da dU`: stressed AA (open, 0) then three unstressed syllables then
    // stressed UW (close, 4). The two passing tones step up from 0 towards 4 — 1, 2 — and the last
    // unstressed syllable is the chromatic approach, a semitone below 4. Every syllable has a note.
    let score = compile(
        "dA da da da dU\nmA ma ma ma dU\n",
        Variant::Syllables,
        Variant::Off,
    );
    assert_foundation(&score);
    assert_eq!(walk(&score, 0), vec!["0", "1", "2", "4-", "4"]);
    let reasons: Vec<BassReason> = score
        .events()
        .iter()
        .filter(|e| e.bar().get() == 0)
        .map(|e| e.bass().unwrap().why())
        .collect();
    let aa = ARPABET.phoneme("AA").unwrap();
    let uw = ARPABET.phoneme("UW").unwrap();
    let (from, toward) = (reasons[0], reasons[4]);
    assert_eq!(from, BassReason::Height(aa));
    assert_eq!(toward, BassReason::Height(uw));
    assert!(matches!(reasons[1], BassReason::Passing { .. }));
    assert!(matches!(reasons[3], BassReason::Approach { onto } if onto.get() == 4));
    // The rhyme word keeps §4.5's note *and* sounds its bass note: W1 walks every syllable.
    let last = score.events().iter().rfind(|e| e.bar().get() == 0).unwrap();
    assert!(last.pitch().is_some() && last.bass().is_some());
}

#[test]
fn syllables_lead_in_and_tail_off_around_the_stressed_ones() {
    // `da da dO da da`: two unstressed before the stressed OW (mid, 2), two after. Before it the
    // walk climbs in: 1 a step below, then the approach a semitone below 2. After it, it steps down
    // from 2: 1, 0.
    let score = compile(
        "da da dO da da\nma ma mO ma ma\n",
        Variant::Syllables,
        Variant::Off,
    );
    assert_eq!(walk(&score, 0), vec!["1", "2-", "2", "1", "0"]);
}

#[test]
fn syllables_between_two_equal_notes_take_the_upper_neighbour() {
    // `dO da da dE`: OW and EH are both mid, 2. Between them the passing tone is the upper
    // neighbour, 3, and the approach a semitone below 2.
    let score = compile(
        "dO da da dE\nmO ma ma mE\n",
        Variant::Syllables,
        Variant::Off,
    );
    assert_eq!(walk(&score, 0), vec!["2", "3", "2-", "2"]);
}

#[test]
fn syllables_walk_down_as_well_as_up() {
    // `dU da da da dA`: from the fifth (4) down to the root (0): 3, 2, then the approach a semitone
    // below 0 — chromatic approach is always from below.
    let score = compile(
        "dU da da da dA\nmU ma ma ma mA\n",
        Variant::Syllables,
        Variant::Off,
    );
    assert_eq!(walk(&score, 0), vec!["4", "3", "2", "0-", "0"]);
}

#[test]
fn a_line_with_no_stressed_syllable_has_no_walk() {
    // Nothing to walk between: no anchor, no notes.
    let score = compile("da da\nma ma\n", Variant::Syllables, Variant::Off);
    assert_eq!(walk(&score, 0), vec!["·", "·"]);
}

/// `da dA da dO da dO da dU`: four iambs, so four feet of two syllables, whose stressed vowels are
/// open, mid, mid, close.
const IAMBS: &str = "da dA da dO da dO da dU\nma mA ma mO ma mO ma mU\n";

#[test]
fn the_fixture_scans_as_iambs() {
    let score = compile(IAMBS, Variant::Feet, Variant::Off);
    let positions: Vec<(usize, usize)> = score
        .events()
        .iter()
        .filter(|e| e.bar().get() == 0)
        .map(|e| (e.beat().get(), e.subdivision().index()))
        .collect();
    assert_eq!(
        positions,
        vec![
            (0, 0),
            (0, 1),
            (1, 0),
            (1, 1),
            (2, 0),
            (2, 1),
            (3, 0),
            (3, 1)
        ]
    );
}

#[test]
fn feet_walk_a_note_a_foot_on_the_beat_with_the_harmony_off() {
    // W2, hand-worked, no chords: the scale's root, third and fifth.
    // Foot 0, AA open: the root, 0. Foot 1, OW mid: the third, 2. Foot 2, OW again: 2 would repeat
    // the note the walk is on, so it steps — the next foot's UW is higher than OW, so up, to 3.
    // Foot 3, UW close: the fifth, 4. One note a foot, on its first syllable (the unstressed `da`).
    let score = compile(IAMBS, Variant::Feet, Variant::Off);
    assert_foundation(&score);
    assert_eq!(
        walk(&score, 0),
        vec!["0", "·", "2", "·", "3", "·", "4", "·"]
    );
    let source = IAMBS;
    let notes: Vec<_> = score
        .events()
        .iter()
        .filter(|e| e.bar().get() == 0)
        .filter_map(|e| e.bass())
        .collect();
    // Each note traces to its whole foot, and is struck with the foot's stressed syllable's accent
    // rather than the unstressed syllable it rides on.
    let spans: Vec<&str> = notes
        .iter()
        .map(|note| note.span().text(source).unwrap())
        .collect();
    assert_eq!(spans, vec!["da dA", "da dO", "da dO", "da dU"]);
    assert!(
        notes
            .iter()
            .all(|note| note.velocity() == Velocity::PRIMARY)
    );
    assert!(notes.iter().all(|note| note.root().get() == 0));
    assert!(matches!(
        notes[2].why(),
        BassReason::Step {
            direction: Direction::Up,
            chord: None,
            ..
        }
    ));
}

#[test]
fn feet_step_down_when_the_next_vowel_is_not_higher() {
    // OW (2), OW again — the next foot's AA is lower, so down, to 1 — then AA (0).
    let score = compile(
        "da dO da dO da dA\nma mO ma mO ma mA\n",
        Variant::Feet,
        Variant::Off,
    );
    assert_eq!(walk(&score, 0), vec!["2", "·", "1", "·", "0", "·"]);
    // The last foot repeating with no next foot steps down too: AA (0), AA again → −1, below the
    // root.
    let score = compile("da dA da dA\nma mA ma mA\n", Variant::Feet, Variant::Off);
    assert_eq!(walk(&score, 0), vec!["0", "·", "-1", "·"]);
}

/// ABAB in four-iamb lines: U, E, U, E. Degrees by first appearance: U 0, E 1.
const ABAB: &str = "da dA da dO da dO da dU\n\
                    ma mA ma mO ma mO ma mE\n\
                    ta tA ta tO ta tO ta tU\n\
                    sa sA sa sO sa sO sa sE\n";

#[test]
fn feet_walk_on_the_bars_chord_under_harmony_rhyme() {
    // H1: each line's chord is I on its class's degree — roots 0, 1, 0, 1. The steps are counted
    // from each bar's own chord's root, so the walk outlines the chord and moves with it.
    // The U lines' vowels are open, mid, mid, close: 0, 2, then a step up (UW is higher) to 3,
    // then 4. The E lines end on EH, mid: 0, 2, then a step *down* (EH is not higher than OW) to
    // 1, then EH's third, 2.
    let score = compile(ABAB, Variant::Feet, Variant::Rhyme);
    assert_foundation(&score);
    for bar in [0, 2] {
        assert_eq!(
            walk(&score, bar),
            vec!["0", "·", "2", "·", "3", "·", "4", "·"]
        );
    }
    for bar in [1, 3] {
        assert_eq!(
            walk(&score, bar),
            vec!["0", "·", "2", "·", "1", "·", "2", "·"]
        );
    }
    let roots: Vec<usize> = (0..4)
        .map(|bar| {
            score
                .events()
                .iter()
                .find(|e| e.bar().get() == bar && e.bass().is_some())
                .unwrap()
                .bass()
                .unwrap()
                .root()
                .get()
        })
        .collect();
    assert_eq!(roots, vec![0, 1, 0, 1]);
    let first = score.events()[0].bass().unwrap();
    assert!(matches!(
        first.why(),
        BassReason::ChordTone { chord: Some(chord), .. } if chord.root().get() == 0
    ));
}

#[test]
fn feet_walk_on_the_cadence() {
    // H2 over ABAB: bar 0 opens U (V of 0, root 4), bar 1 opens E (V of 1, root 5), bar 2 closes
    // U (I, root 0), bar 3 is the turnaround (V of the stanza's first chord, 4 + 4 = 8).
    let score = compile(ABAB, Variant::Feet, Variant::Cadence);
    let roots: Vec<usize> = (0..4)
        .map(|bar| {
            score
                .events()
                .iter()
                .find(|e| e.bar().get() == bar && e.bass().is_some())
                .unwrap()
                .bass()
                .unwrap()
                .root()
                .get()
        })
        .collect();
    assert_eq!(roots, vec![4, 5, 0, 8]);
}

#[test]
fn feet_without_a_chord_fall_back_to_the_scale() {
    // Under H1 a line that rhymes with nothing has no chord: its bar walks from degree 0.
    // `ma mE` rhymes with nothing here; the two U lines rhyme.
    let source = "da dA da dU\nma mA ma mE\nta tA ta tU\n";
    let score = compile(source, Variant::Feet, Variant::Rhyme);
    assert!(score.harmony_of(1).unwrap().chord().is_none());
    let note = score
        .events()
        .iter()
        .find(|e| e.bar().get() == 1 && e.bass().is_some())
        .unwrap()
        .bass()
        .unwrap();
    assert_eq!(note.root().get(), 0);
    assert!(matches!(
        note.why(),
        BassReason::ChordTone { chord: None, .. }
    ));
}

#[test]
fn a_flourish_line_has_no_walk() {
    // Stanza 1 regular, so foundation; stanza 2 ragged (one syllable against four), so flourish.
    for variant in [Variant::Syllables, Variant::Feet] {
        let score = compile("dA dA\nmA mA\n\ndA\nnA na nA na\n", variant, Variant::Off);
        let carries = |bar: usize| {
            score
                .events()
                .iter()
                .any(|e| e.bar().get() == bar && e.bass().is_some())
        };
        assert!(carries(0) && carries(1), "{variant:?}");
        assert!(!carries(2) && !carries(3), "{variant:?}");
    }
}

#[test]
fn off_has_no_bass() {
    let score = compile(IAMBS, Variant::Off, Variant::Rhyme);
    assert!(score.events().iter().all(|e| e.bass().is_none()));
}
