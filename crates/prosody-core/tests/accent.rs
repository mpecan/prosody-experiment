//! The light function words, through the real passes: a one-syllable function word struck at
//! secondary force where the meter's scansion calls its position weak (`prosody_core::accent`,
//! `SPEC.md` §4.2), and left at its dictionary stress everywhere else.
//!
//! The phonemiser is a table giving each word `CMUdict`'s stresses (1 primary, 2 secondary, 0
//! unstressed — "or" is `AO1 R`, "immortal" `IH2 M AO1 R T AH0 L`), so each scansion below can be
//! worked by hand from the stresses alone. The lexicon calls a fixed handful of them function words.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use proptest::prelude::{Strategy, prop_assert, prop_assert_eq, prop_assume, proptest};
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::{Score, Velocity};
use prosody_core::{Poem, Span, annotate, meter, repetition, rhyme, role, sound, surprisal};

/// Each word's vowels and their `CMUdict` stresses.
const WORDS: &[(&str, &[(&str, u8)])] = &[
    ("what", &[("AH", 1)]),
    ("immortal", &[("IH", 2), ("AO", 1), ("AH", 0)]),
    ("hand", &[("AE", 1)]),
    ("or", &[("AO", 1)]),
    ("eye", &[("AY", 1)]),
    ("in", &[("IH", 0)]),
    ("the", &[("AH", 0)]),
    ("forests", &[("AO", 1), ("AH", 0)]),
    ("of", &[("AH", 1)]),
    ("night", &[("AY", 1)]),
    ("did", &[("IH", 1)]),
    ("he", &[("IY", 1)]),
    ("who", &[("UW", 1)]),
    ("made", &[("EY", 1)]),
    ("lamb", &[("AE", 1)]),
    ("make", &[("EY", 1)]),
    ("thee", &[("IY", 1)]),
    ("thy", &[("AY", 1)]),
    ("burning", &[("ER", 1), ("IH", 0)]),
    ("bright", &[("AY", 1)]),
];

/// The function words among them.
const FUNCTION: &[&str] = &[
    "did", "he", "in", "of", "or", "the", "thee", "thy", "what", "who",
];

struct Table;

impl Phonemiser for Table {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let lower = word.to_lowercase();
        let (_, vowels) = WORDS
            .iter()
            .find(|(known, _)| *known == lower)
            .ok_or(Unpronounceable::NotKnown)?;
        let phones: Vec<_> = vowels
            .iter()
            .map(|(symbol, stress)| {
                let stress = match stress {
                    1 => Stress::Primary,
                    2 => Stress::Secondary,
                    _ => Stress::Unstressed,
                };
                (ARPABET.phoneme(symbol).unwrap(), stress)
            })
            .collect();
        Ok(Pronunciation::from_dictionary(syllabify(&phones).unwrap()))
    }
}

/// A lexicon whose closed class is the list it holds.
struct Closed(&'static [&'static str]);

impl Lexicon for Closed {
    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }

    fn is_function_word(&self, word: &str) -> bool {
        self.0.contains(&word)
    }
}

fn compile_with(source: &str, mappings: &Mappings) -> Score {
    compile_in(source, mappings, &Closed(FUNCTION))
}

fn compile_in(source: &str, mappings: &Mappings, lexicon: &Closed) -> Score {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Table);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &Table),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &Table),
            surprisal: &surprisal::analyse(&poem, &Table, lexicon),
            roles: &roles,
        },
        mappings,
    )
    .unwrap()
}

fn function(variant: Variant) -> Mappings {
    Mappings::all().with(Mapping::Function.choose(variant).unwrap())
}

fn full(source: &str) -> Score {
    compile_with(source, &Mappings::all())
}

fn light(source: &str) -> Score {
    compile_with(source, &function(Variant::Light))
}

/// The line's feet, by name, from the meter's own scansion.
fn feet(source: &str) -> Vec<&'static str> {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let meter = meter::scan(&poem, &Table);
    meter.stanzas()[0].lines()[0]
        .feet()
        .iter()
        .map(|foot| foot.kind().name())
        .collect()
}

/// Each syllable as `word:force`, force in thirds (3 primary, 2 secondary, 1 unstressed), with a
/// `*` where the light function words decided it.
fn forces(score: &Score, source: &str) -> Vec<String> {
    score
        .events()
        .iter()
        .map(|event| {
            let word = &source[event.span().start()..event.span().end()];
            let force = match event.velocity() {
                v if v == Velocity::PRIMARY => 3,
                v if v == Velocity::SECONDARY => 2,
                _ => 1,
            };
            format!("{word}:{force}{}", if event.is_light() { "*" } else { "" })
        })
        .collect()
}

#[test]
fn hand_or_eye_strikes_or_lightly_only_under_light() {
    // What(1) im(2) mor(1) tal(0) hand(1) or(1) eye(1). Scanned on primary stress only:
    // S w S w S S S — trochee, trochee, spondee, single at no mismatch. "or" is the spondee's
    // second syllable; the line has two falling feet and no rising one, so the spondee's strong
    // position is its first, "hand", and "or" is weak. "What" opens a trochee: strong, kept.
    let source = "What immortal hand or eye\n";
    assert_eq!(feet(source), ["trochee", "trochee", "spondee", "single"]);
    let words = |score: &Score| forces(score, source);
    assert_eq!(
        words(&full(source)),
        [
            "What:3",
            "immortal:2",
            "immortal:3",
            "immortal:1",
            "hand:3",
            "or:3",
            "eye:3"
        ]
    );
    assert_eq!(
        words(&light(source)),
        [
            "What:3",
            "immortal:2",
            "immortal:3",
            "immortal:1",
            "hand:3",
            "or:2*",
            "eye:3"
        ]
    );
    assert_eq!(full(source).function(), Variant::Full);
    assert_eq!(full(source).lightened(), 0);
    assert_eq!(light(source).function(), Variant::Light);
    assert_eq!(light(source).lightened(), 1);
}

#[test]
fn a_function_word_on_a_strong_position_keeps_its_accent() {
    // In(0) the(0) for(1) ests(0) of(1) the(0) night(1): w w S w S w S — anapest, iamb, iamb.
    // "of" is the iamb's second syllable, its strong position: Blake's trochaic line promotes it,
    // and the meter says so. Nothing changes under light.
    let source = "In the forests of the night\n";
    assert_eq!(feet(source), ["anapest", "iamb", "iamb"]);
    let expected = [
        "In:1",
        "the:1",
        "forests:3",
        "forests:1",
        "of:3",
        "the:1",
        "night:3",
    ];
    assert_eq!(forces(&full(source), source), expected);
    assert_eq!(forces(&light(source), source), expected);
    assert_eq!(light(source).lightened(), 0);
}

#[test]
fn in_a_rising_line_a_spondees_second_syllable_is_strong() {
    // Did(1) he(1) who(1) made(1) the(0) lamb(1) make(1) thee(1): S S S S w S S S — spondee,
    // spondee, iamb, spondee. One rising foot, no falling one: each spondee's strong position is
    // its second. "Did" and "who" open a spondee, so they are weak; "he" and the rhyme word
    // "thee" close one, so they keep the accent. "made", "lamb" and "make" are content words.
    let source = "Did he who made the lamb make thee\n";
    assert_eq!(feet(source), ["spondee", "spondee", "iamb", "spondee"]);
    assert_eq!(
        forces(&light(source), source),
        [
            "Did:2*", "he:3", "who:2*", "made:3", "the:1", "lamb:3", "make:3", "thee:3"
        ]
    );
}

#[test]
fn in_a_line_without_direction_the_content_word_or_the_beat_takes_the_accent() {
    // hand(1) or(1) eye(1): spondee, single — neither rising nor falling. "or" yields to "hand",
    // which is not a function word.
    assert_eq!(feet("hand or eye\n"), ["spondee", "single"]);
    assert_eq!(
        forces(&light("hand or eye\n"), "hand or eye\n"),
        ["hand:3", "or:2*", "eye:3"]
    );
    // or(1) thy(1) hand(1): spondee, single again, both of the spondee function words — the one
    // on the beat keeps it.
    assert_eq!(feet("or thy hand\n"), ["spondee", "single"]);
    assert_eq!(
        forces(&light("or thy hand\n"), "or thy hand\n"),
        ["or:3", "thy:2*", "hand:3"]
    );
}

#[test]
fn a_function_word_of_two_syllables_keeps_its_dictionary_stress() {
    // eye(1) bur(1) ning(0) hand(1) bur(1) ning(0): S S w S S w — spondee, iamb, trochee, one
    // rising and one falling foot, so no direction. Were "burning" a one-syllable function word,
    // its "bur" would yield the first spondee to "eye". A lexicon that calls it closed-class still
    // does not lighten it: a word of two syllables has a stress pattern of its own.
    let source = "eye burning hand burning\n";
    assert_eq!(feet(source), ["spondee", "iamb", "trochee"]);
    let lexicon = Closed(&["burning"]);
    let score = compile_in(source, &function(Variant::Light), &lexicon);
    assert_eq!(
        forces(&score, source),
        [
            "eye:3",
            "burning:3",
            "burning:1",
            "hand:3",
            "burning:3",
            "burning:1"
        ]
    );
}

#[test]
fn a_light_function_word_loses_its_height_bass_note_and_nothing_else_moves() {
    // A one-line poem is its own foundation. The height bass plays every primary-stressed
    // foundation syllable but a pitched rhyme word (§4.6), and "eye" rhymes with nothing, so it
    // has no note of §4.5's to leave room for: under full "What", "mor", "hand", "or" and "eye"
    // carry a bass note. Under light "or" is no longer primary, and its note goes with its accent.
    let source = "What immortal hand or eye\n";
    let (full, light) = (full(source), light(source));
    let basses = |score: &Score| -> Vec<bool> {
        score.events().iter().map(|e| e.bass().is_some()).collect()
    };
    assert_eq!(basses(&full), [true, false, true, false, true, true, true]);
    assert_eq!(
        basses(&light),
        [true, false, true, false, true, false, true]
    );
    // Every event is where it was, from the same word, in the same voice.
    let places = |score: &Score| -> Vec<String> {
        score
            .events()
            .iter()
            .map(|e| {
                format!(
                    "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                    e.span(),
                    e.voice(),
                    e.bar(),
                    e.beat(),
                    e.subdivision(),
                    e.foot(),
                    e.pitch(),
                    e.sharpness()
                )
            })
            .collect()
    };
    assert_eq!(places(&full), places(&light));
    assert_eq!(full.voices(), light.voices());
    assert_eq!(full.arrangement(), light.arrangement());
}

#[test]
fn the_explain_view_names_the_variant_and_each_light_syllable() {
    let source = "What immortal hand or eye\n";
    let view = |score: &Score| {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Table);
        let roles = role::assign(&meter, &repetition::analyse(&poem)).unwrap();
        annotate::render(&poem, &annotate::run(&poem, score, &roles))
    };
    let (full_view, light_view) = (view(&full(source)), view(&light(source)));
    assert!(!full_view.contains("function-words"), "{full_view}");
    assert!(
        light_view.contains(
            "function-words light · a one-syllable function word off the metre's strong \
             positions is struck at 0.67, not full force: 1 syllable(s) (§4.2)"
        ),
        "{light_view}"
    );
    let row = |view: &str, word: &str| {
        view.lines()
            .find(|line| line.trim_start().starts_with(word))
            .unwrap()
            .to_owned()
    };
    let or = row(&light_view, "or ");
    assert!(
        or.contains("0.67") && or.ends_with(",function-words"),
        "{or}"
    );
    assert!(!row(&light_view, "hand").contains("function-words"));
    // Only the gain, the bass column and the "from" column of that one row, and the one header
    // line, differ from the full view.
    let differing: Vec<(&str, &str)> = full_view
        .lines()
        .zip(
            light_view
                .lines()
                .filter(|l| !l.starts_with("function-words")),
        )
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(differing.len(), 1, "{differing:?}");
    assert_eq!(light_view.lines().count(), full_view.lines().count() + 1);
}

/// A poem of the table's words: up to three stanzas of up to three lines.
fn table_poem() -> impl Strategy<Value = String> {
    let word = proptest::sample::select(WORDS.iter().map(|(word, _)| *word).collect::<Vec<_>>());
    let line = proptest::collection::vec(word, 1..7).prop_map(|words| words.join(" "));
    let stanza = proptest::collection::vec(line, 1..4).prop_map(|lines| lines.join("\n"));
    proptest::collection::vec(stanza, 1..4)
        .prop_map(|stanzas| format!("{}\n", stanzas.join("\n\n")))
}

/// Every way of choosing the other mappings, with the function words at `variant`.
fn every_with(pick: usize, variant: Variant) -> Mappings {
    let every = Mappings::every();
    every[pick % every.len()]
        .clone()
        .with(Mapping::Function.choose(variant).unwrap())
}

/// Everything an event carries but its force and its bass note.
fn shape(score: &Score) -> Vec<String> {
    score
        .events()
        .iter()
        .map(|e| {
            format!(
                "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                e.span(),
                e.voice(),
                e.bar(),
                e.beat(),
                e.subdivision(),
                e.foot(),
                e.provenance(),
                e.pitch(),
                e.sharpness(),
                e.family(),
                e.anchor(),
                e.bank()
            )
        })
        .collect()
}

proptest! {
    /// Light moves nothing but force: under any choice of the other mappings, the same events
    /// in the same places, voices, arrangement and harmony; every changed event is a lightened
    /// one-syllable function word, primary under full and secondary under light; and a lightened
    /// syllable is never on a non-spondee foot's strong position.
    #[test]
    fn light_changes_only_the_force_of_function_words(
        source in table_poem(),
        pick in 0_usize..1 << 16,
    ) {
        let full = compile_with(&source, &every_with(pick, Variant::Full));
        let light = compile_with(&source, &every_with(pick, Variant::Light));
        prop_assert_eq!(shape(&full), shape(&light));
        prop_assert_eq!(full.voices(), light.voices());
        prop_assert_eq!(full.arrangement(), light.arrangement());
        prop_assert_eq!(full.harmony(), light.harmony());
        prop_assert!(full.events().iter().all(|e| !e.is_light()));
        for (before, after) in full.events().iter().zip(light.events()) {
            if before.velocity() == after.velocity() {
                prop_assert!(!after.is_light());
                continue;
            }
            prop_assert!(after.is_light());
            prop_assert_eq!(before.velocity(), Velocity::PRIMARY);
            prop_assert_eq!(after.velocity(), Velocity::SECONDARY);
            let word = &source[after.span().start()..after.span().end()];
            prop_assert!(FUNCTION.contains(&word.to_lowercase().as_str()), "{}", word);
            let syllables = light.events().iter().filter(|e| e.span() == after.span()).count();
            prop_assert_eq!(syllables, 1);
            let strong = after.foot().template()[after.subdivision().index()];
            prop_assert!(
                !strong || after.foot() == prosody_core::FootKind::Spondee,
                "{} lightened on a strong position of a {:?}", word, after.foot()
            );
        }
        prop_assert_eq!(
            light.lightened(),
            light.events().iter().filter(|e| e.is_light()).count()
        );
    }

    /// I2 under light: an edit to the first stanza leaves the last stanza's forces, and which of
    /// them are light, as they were — the decision reads only a syllable's own word and line.
    #[test]
    fn i2_an_edit_leaves_other_stanzas_forces_alone(
        source in table_poem(),
        pick in 0_usize..64,
        with in proptest::sample::select(vec!["or", "hand", "thy", "burning", "the"]),
    ) {
        let mut poem = Poem::parse(source.clone()).unwrap();
        let stanzas = poem.stanzas().len();
        prop_assume!(stanzas > 1);
        let words: Vec<Span> = poem.stanzas()[0]
            .lines()
            .iter()
            .flat_map(|line| line.words().map(|token| token.span()).collect::<Vec<_>>())
            .collect();
        poem.apply(words[pick % words.len()], with).unwrap();
        prop_assume!(poem.stanzas().len() == stanzas);
        let forces = |source: &str| {
            let score = light(source);
            let parsed = Poem::parse(source.to_owned()).unwrap();
            let last = parsed.stanzas()[stanzas - 1].span();
            score
                .events()
                .iter()
                .filter(|e| last.contains(e.span()))
                .map(|e| (e.span().start() - last.start(), e.velocity(), e.is_light()))
                .collect::<Vec<_>>()
        };
        prop_assert_eq!(forces(&source), forces(poem.source()));
    }
}
