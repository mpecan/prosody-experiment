//! The harmony layer (`SPEC.md` §4.6, `prosody_core::harmony`), through the real passes, with
//! hand-worked progressions.
//!
//! The phonemiser is a toy: `a` is an unstressed syllable (AH), and each capital a stressed one with
//! its own vowel — `A` AA, `O` OW, `E` EH, `U` UW — other letters silent. A line's rhyme is its last
//! capital, so `da dA` and `ta tA` rhyme and `ma mE` rhymes with nothing unless another `E` ends a
//! line. Every line here has two syllables, so every stanza is regular — foundation (§4.1) — unless
//! a test says otherwise.

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
    Chord, Harmony, Layer, Onset, Poem, Reason, Silence, meter, repetition, rhyme, role, sound,
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

fn compile(source: &str, harmony: Variant) -> Score {
    let mappings = Mappings::all().with(Mapping::Harmony.choose(harmony).unwrap());
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

/// Each line's harmony, as `"<root> <numeral> <why>"` or `"rest <why>"`, in bar order — and a
/// stanza's split last bar as its own chord (or `rest`), `then`, its turnaround.
fn progression(score: &Score) -> Vec<String> {
    let chord = |chord: Chord| {
        let why = match chord.reason() {
            Reason::Class(class) => format!("class {}", class.letter()),
            Reason::Opens(class) => format!("opens {}", class.letter()),
            Reason::Closes(class) => format!("closes {}", class.letter()),
            Reason::Unanswered { key } => format!("unanswered in {}", key.get()),
            Reason::Turnaround { onto } => format!("turnaround onto {}", onto.get()),
        };
        format!(
            "{} {} {why}",
            chord.root().get(),
            chord.function().numeral()
        )
    };
    score
        .harmony()
        .iter()
        .map(|harmony| match *harmony {
            Harmony::Chord(one) => chord(one),
            Harmony::Turning { own, turn } => format!(
                "{} then {}",
                own.map_or_else(|| "rest".to_owned(), chord),
                chord(turn)
            ),
            Harmony::Rest(rest) => format!("rest {:?}", rest.why()).to_lowercase(),
        })
        .collect()
}

/// Each slot's chords, as (bar, root, onset) in the order they sound.
fn slot_chords(score: &Score) -> Vec<Vec<(usize, usize, Onset)>> {
    score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| {
            score
                .harmony_in(slot)
                .into_iter()
                .flat_map(Harmony::chords)
                .map(|chord| (chord.bar().get(), chord.root().get(), chord.onset()))
                .collect()
        })
        .collect()
}

/// ABAB: A is the first class that sounds (degree 0), B the second (degree 1).
const ABAB: &str = "da dA\nda dO\nta tA\nta tO\n";

/// Two stanzas, by bar:
///
/// 0 `da dO` — O, answered only in stanza 2 (bar 4): no rhyme within its stanza
/// 1 `da dA` — A, answered at bar 2: open
/// 2 `ta tA` — A, answering bar 1: closes
/// 3 `ma mE` — E, rhymes with nothing; the stanza's last bar
/// 4 `sa sO` — O, its partner is in stanza 1
/// 5 `na nU` — U, rhymes with nothing; the stanza's last bar
///
/// Degrees by first appearance among classes that sound: O is 0, A is 1. Scheme letters by first
/// appearance among all lines: O is `A`, A is `B`, E is `C`, U is `D`.
const MIXED: &str = "da dO\nda dA\nta tA\nma mE\n\nsa sO\nna nU\n";

#[test]
fn the_fixtures_are_foundation() {
    for source in [ABAB, MIXED] {
        let score = compile(source, Variant::Rhyme);
        for voice in score.voices() {
            assert_eq!(voice.layer(), Layer::Foundation, "{source}");
        }
    }
}

#[test]
fn off_has_no_harmony_and_is_the_default() {
    assert!(compile(ABAB, Variant::Off).harmony().is_empty());
    assert_eq!(Mappings::all().get(Mapping::Harmony), Variant::Off);
}

#[test]
fn rhyme_makes_abab_alternate_i_and_ii() {
    // H1: each line's chord is its class's degree. ABAB → i–ii–i–ii.
    assert_eq!(
        progression(&compile(ABAB, Variant::Rhyme)),
        vec!["0 I class A", "1 I class B", "0 I class A", "1 I class B"]
    );
}

#[test]
fn cadence_makes_open_rhymes_dominant_and_their_answers_tonic() {
    // H2 on ABAB: A opens (V of 0 is 4), B opens (V of 1 is 5), A closes (I on 0). The last bar
    // closes B, so it lands on I on 1 — and from its half turns round: V of the stanza's first
    // chord's root, 4 + 4.
    assert_eq!(
        progression(&compile(ABAB, Variant::Cadence)),
        vec![
            "4 V opens A",
            "5 V opens B",
            "0 I closes A",
            "1 I closes B then 8 V turnaround onto 4",
        ]
    );
}

#[test]
fn cadence_lands_aabb_and_abba_on_i_before_turning_round() {
    // AABB: A (AA) is degree 0, B (OW) degree 1. A opens (V/4), A closes (I/0), B opens (V/5),
    // B closes (I/1), then the turnaround onto the first chord, 4: V/8.
    assert_eq!(
        progression(&compile("da dA\nta tA\nma mO\nsa sO\n", Variant::Cadence)),
        vec![
            "4 V opens A",
            "0 I closes A",
            "5 V opens B",
            "1 I closes B then 8 V turnaround onto 4",
        ]
    );
    // ABBA: A opens (V/4), B opens (V/5), B closes (I/1), and A closes on the stanza's own home,
    // I/0, before the turnaround onto 4.
    assert_eq!(
        progression(&compile("da dA\nda dO\nta tO\nta tA\n", Variant::Cadence)),
        vec![
            "4 V opens A",
            "5 V opens B",
            "1 I closes B",
            "0 I closes A then 8 V turnaround onto 4",
        ]
    );
}

#[test]
fn an_unanswered_last_line_keeps_its_iv_before_the_turnaround() {
    // ABAC: O and U rhyme with nothing, so only A sounds (degree 0) and the key is 0. A opens
    // (V/4), O is unanswered (IV of 0, 3), A closes (I/0). The last line is unanswered too: the
    // bar is split all the same — what H2 gives it, IV/3, then V of the first chord, 8. Decided
    // (`SPEC.md` §4.6): the user's rule is "its own chord first", and IV–V–(the loop's first
    // chord) is a cadence of its own, so the open line keeps its colour rather than a whole bar
    // of V.
    assert_eq!(
        progression(&compile("da dA\nda dO\nta tA\nta tU\n", Variant::Cadence)),
        vec![
            "4 V opens A",
            "3 IV unanswered in 0",
            "0 I closes A",
            "3 IV unanswered in 0 then 8 V turnaround onto 4",
        ]
    );
}

#[test]
fn the_turnaround_sounds_from_the_bars_half_and_the_rest_from_its_start() {
    // ABAB stated, then the outro's pass: the last bar's two chords in slots 3 and 7, its own at
    // the start and the turnaround at the half; every other slot one chord from its start.
    let start = |bar, root| (bar, root, Onset::Start);
    let chords = slot_chords(&compile(ABAB, Variant::Cadence));
    let pass = vec![
        vec![start(0, 4)],
        vec![start(1, 5)],
        vec![start(2, 0)],
        vec![start(3, 1), (3, 8, Onset::Half)],
    ];
    assert_eq!(chords, [pass.clone(), pass].concat());
}

#[test]
fn cadence_on_the_mixed_fixture() {
    // Stanza 1's key is its first in-stanza rhyme's degree, A's: 1. O has no rhyme in this stanza
    // → IV of the key (1 + 3). The A pair → V of 1 (5), then I on 1. The last bar → V of the first
    // chord's root (4 + 4). Stanza 2 rhymes nothing within itself, so its key is 0: O → IV (3),
    // and its last bar turns round onto that (3 + 4).
    assert_eq!(
        progression(&compile(MIXED, Variant::Cadence)),
        vec![
            "4 IV unanswered in 1",
            "5 V opens B",
            "1 I closes B",
            "4 IV unanswered in 1 then 8 V turnaround onto 4",
            "3 IV unanswered in 0",
            "3 IV unanswered in 0 then 7 V turnaround onto 3",
        ]
    );
}

#[test]
fn rhyme_on_the_mixed_fixture_rests_where_a_line_rhymes_with_nothing() {
    // H1: O and A on their degrees, wherever their partners are; E and U are classes of one, so
    // their bars have no chord — and say why.
    assert_eq!(
        progression(&compile(MIXED, Variant::Rhyme)),
        vec![
            "0 I class A",
            "1 I class B",
            "1 I class B",
            "rest unrhymed",
            "0 I class A",
            "rest unrhymed",
        ]
    );
}

#[test]
fn both_lets_cadence_decide_where_it_applies_and_rhyme_fill_the_rest() {
    // Bar 0: no rhyme in its stanza, so H2 does not apply → H1's O on 0. Bars 1–2: the A pair, H2.
    // Bar 3: E rhymes with nothing, so neither H2 nor H1 gives its first half a chord — it rests —
    // and from its half H2's turnaround onto the first chord, now H1's 0: V/4. Bar 4: H1's O on 0.
    // Bar 5: U rests, then the turnaround onto 0.
    assert_eq!(
        progression(&compile(MIXED, Variant::Both)),
        vec![
            "0 I class A",
            "5 V opens B",
            "1 I closes B",
            "rest then 4 V turnaround onto 0",
            "0 I class A",
            "rest then 4 V turnaround onto 0",
        ]
    );
}

#[test]
fn a_line_that_answers_and_is_answered_again_stays_open() {
    // AAAO: bar 1 closes bar 0 but is answered by bar 2, so it is still a suspension (V); bar 2
    // closes (I on 0); bar 3's O rhymes with nothing — IV of the key, 0 + 3 — then the turnaround
    // onto 4.
    assert_eq!(
        progression(&compile("da dA\nta tA\nma mA\nsa sO\n", Variant::Cadence)),
        vec![
            "4 V opens A",
            "4 V opens A",
            "0 I closes A",
            "3 IV unanswered in 0 then 8 V turnaround onto 4",
        ]
    );
}

#[test]
fn a_one_line_stanza_has_no_turnaround() {
    // Its last bar is its first: there is nothing to turn round onto, so the bar is not split. A
    // line alone rhymes with nothing, so H2 gives it IV of degree 0 for the whole bar — under
    // `both` H1 gives it nothing either, so it rests.
    let score = compile("da dA\n", Variant::Cadence);
    assert_eq!(progression(&score), vec!["3 IV unanswered in 0"]);
    assert!(score.harmony()[0].turnaround().is_none());
    assert_eq!(slot_chords(&score)[0], vec![(0, 3, Onset::Start)]);
    assert_eq!(
        progression(&compile("da dA\n", Variant::Both)),
        vec!["rest unrhymed"]
    );
}

#[test]
fn every_chord_traces_to_its_lines_rhyme_word() {
    // I4: the span is the line's last word.
    let score = compile(MIXED, Variant::Both);
    let poem = Poem::parse(MIXED.to_owned()).unwrap();
    let words: Vec<&str> = score
        .chords()
        .map(|chord| chord.span().text(poem.source()).unwrap())
        .collect();
    assert_eq!(words, vec!["dO", "dA", "tA", "mE", "sO", "nU"]);
    // Under cadence the last bar has two chords, and both trace to its own rhyme word — the
    // turnaround's root comes from the first line, its span from the line whose bar it shares.
    let poem = Poem::parse(ABAB.to_owned()).unwrap();
    let words: Vec<&str> = compile(ABAB, Variant::Cadence)
        .chords()
        .map(|chord| chord.span().text(poem.source()).unwrap())
        .collect();
    assert_eq!(words, vec!["dA", "dO", "tA", "tO", "tO"]);
}

#[test]
fn a_flourish_stanza_has_no_chord_and_says_so() {
    // A one-syllable line beside a six-syllable one: irregular, so flourish (§4.1).
    let source = "da dA\nta tA\n\nmA\nma ma ma ma ma mO\n";
    let score = compile(source, Variant::Rhyme);
    let progression = progression(&score);
    assert_eq!(progression[..2], ["0 I class A", "0 I class A"]);
    assert_eq!(progression[2..], ["rest flourish", "rest flourish"]);
    assert!(score.harmony()[2..].iter().all(|harmony| matches!(
        harmony,
        Harmony::Rest(rest) if rest.why() == Silence::Flourish
    )));
}

#[test]
fn a_slot_plays_the_chord_of_its_foundation_line_and_nothing_held_under_it() {
    // ABAB stated, then the outro's pass: eight slots, the A line held under the B line in slot
    // 1 — but a slot's chord is its foundation host's, so it is still i–ii–i–ii, twice.
    let score = compile(ABAB, Variant::Rhyme);
    let pass = vec![
        vec![(0, 0, Onset::Start)],
        vec![(1, 1, Onset::Start)],
        vec![(2, 0, Onset::Start)],
        vec![(3, 1, Onset::Start)],
    ];
    assert_eq!(slot_chords(&score), [pass.clone(), pass].concat());
}

#[test]
fn a_foundation_loop_keeps_its_chords_under_a_flourish() {
    // Stanza 1 (foundation, bars 0–1: A A), then a flourish stanza (bars 2–3) stated over it:
    // slots 2 and 3 are hosted by the flourish, and the foundation loop's bars 0 and 1 sound under
    // them — with their chords.
    let source = "da dA\nta tA\n\nmA\nma ma ma ma ma mO\n";
    let score = compile(source, Variant::Cadence);
    // Cadence on AA: bar 0 opens (V of 0 = 4); bar 1, the last bar, closes (I on 0) and turns round
    // onto 4 from its half (8) — under the flourish as where it is stated.
    let loop_ = vec![
        vec![(0, 4, Onset::Start)],
        vec![(1, 0, Onset::Start), (1, 8, Onset::Half)],
    ];
    assert_eq!(slot_chords(&score)[..4], [loop_.clone(), loop_].concat());
}
