//! Drum kit articulation (`prosody_core::drums`, `Mapping::Drums`, `SPEC.md` §4.1, §4.2): under
//! [`Variant::Flat`] every syllable plays its layer's one piece (`Drum::Pulse` on foundation,
//! `Drum::Ornament` on flourish), and under [`Variant::Articulated`] each syllable's piece is
//! chosen by its layer, beat parity (`beat % 2`) and stress within the foot.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)] // test code; see CLAUDE.md §3

use proptest::prelude::{Strategy, prop_assert, prop_assert_eq, prop_assume, proptest};
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::role::Layer;
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Drum, Poem, Span, annotate, meter, repetition, rhyme, role, sound, surprisal};

/// Each word's vowels and their `CMUdict` stresses.
const WORDS: &[(&str, &[(&str, u8)])] = &[
    ("tyger", &[("AY", 1), ("ER", 0)]),
    ("burning", &[("ER", 1), ("IH", 0)]),
    ("bright", &[("AY", 1)]),
    ("in", &[("IH", 0)]),
    ("the", &[("AH", 0)]),
    ("forests", &[("AO", 1), ("AH", 0)]),
    ("of", &[("AH", 1)]),
    ("night", &[("AY", 1)]),
    ("what", &[("AH", 1)]),
    ("immortal", &[("IH", 2), ("AO", 1), ("AH", 0)]),
    ("hand", &[("AE", 1)]),
    ("or", &[("AO", 1)]),
    ("eye", &[("AY", 1)]),
];

const FUNCTION: &[&str] = &["in", "of", "or", "the", "what"];

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

struct Closed;

impl Lexicon for Closed {
    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }

    fn is_function_word(&self, word: &str) -> bool {
        FUNCTION.contains(&word)
    }
}

fn compile_with(source: &str, mappings: &Mappings) -> Score {
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
            surprisal: &surprisal::analyse(&poem, &Table, &Closed),
            roles: &roles,
        },
        mappings,
    )
    .unwrap()
}

fn flat(source: &str) -> Score {
    compile_with(source, &Mappings::all())
}

fn articulated(source: &str) -> Score {
    compile_with(
        source,
        &Mappings::all().with(Mapping::Drums.choose(Variant::Articulated).unwrap()),
    )
}

fn drums_of(score: &Score) -> Vec<Drum> {
    score.events().iter().map(|e| e.drum()).collect()
}

#[test]
fn trochaic_and_iambic_lines_articulate_their_stress_positions_across_the_kit() {
    // "Tyger Tyger burning bright": trochee, trochee, trochee, single on a foundation line.
    // Under flat, every syllable is `Drum::Pulse`. Under articulated:
    //   beat 0 ("Ty-ger"):   [Pulse, Subdivision]
    //   beat 1 ("Ty-ger"):   [Backbeat, Subdivision]
    //   beat 2 ("burn-ing"): [Pulse, Subdivision]
    //   beat 3 ("bright"):   Backbeat
    let trochaic = "Tyger Tyger burning bright\n";
    assert_eq!(flat(trochaic).drums(), Variant::Flat);
    assert_eq!(flat(trochaic).articulated(), 0);
    assert_eq!(drums_of(&flat(trochaic)), vec![Drum::Pulse; 7]);
    assert_eq!(articulated(trochaic).drums(), Variant::Articulated);
    assert_eq!(articulated(trochaic).articulated(), 5);
    assert_eq!(
        drums_of(&articulated(trochaic)),
        vec![
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
            Drum::Subdivision,
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
        ]
    );

    // "In the forests of the night": anapest ("In the for-"), iamb ("-ests of"), iamb ("the night").
    //   beat 0 ("In the for-"): [Subdivision, Subdivision, Pulse]
    //   beat 1 ("-ests of"):    [Subdivision, Backbeat]
    //   beat 2 ("the night"):   [Subdivision, Pulse]
    let rising = "In the forests of the night\n";
    assert_eq!(
        drums_of(&articulated(rising)),
        vec![
            Drum::Subdivision,
            Drum::Subdivision,
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
            Drum::Subdivision,
            Drum::Pulse,
        ]
    );
}

#[test]
fn a_spondee_splits_between_pulse_and_backbeat_unless_lightened() {
    // "What immortal hand or eye": trochee ("What im-"), trochee ("-mortal"), spondee ("hand or"),
    // single ("eye").
    // Under `function-words = full`, beat 2 ("hand or") has two primary stresses, so "hand" takes
    // `Drum::Pulse` and "or" takes the complementary `Drum::Backbeat`.
    // Under `function-words = light`, "or" is lightened to secondary stress, so beat 2 plays
    // `[Drum::Pulse, Drum::Subdivision]`.
    let source = "What immortal hand or eye\n";
    assert_eq!(
        drums_of(&articulated(source)),
        vec![
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
            Drum::Subdivision,
            Drum::Pulse,
            Drum::Backbeat,
            Drum::Backbeat,
        ]
    );
    let with_light = compile_with(
        source,
        &Mappings::all()
            .with(Mapping::Drums.choose(Variant::Articulated).unwrap())
            .with(Mapping::Function.choose(Variant::Light).unwrap()),
    );
    assert_eq!(
        drums_of(&with_light),
        vec![
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
            Drum::Subdivision,
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Backbeat,
        ]
    );
}

#[test]
fn a_flourish_stanza_articulates_across_toms_snare_and_closed_hat() {
    // Stanza 0 has zero metrical variance (foundation); stanza 1 has high variance (1 beat vs 4
    // beats, flourish). In stanza 1's second line ("What immortal hand or eye"):
    //   beat 0 ("What im-"):   [HighTom (primary, even), MidTom (secondary, even)]
    //   beat 1 ("-mor-tal"):   [Backbeat (primary, odd), Subdivision (unstressed)]
    //   beat 2 ("hand or"):    [HighTom (primary, even), MidTom (second primary in spondee, even)]
    //   beat 3 ("eye"):        Backbeat (primary, odd)
    let source = "Tyger burning\nTyger burning\n\nbright\nWhat immortal hand or eye\n";
    let flat_score = flat(source);
    let art_score = articulated(source);
    let flourish_voice = art_score
        .voices()
        .iter()
        .find(|v| v.layer() == Layer::Flourish && v.span().start() > 35)
        .unwrap()
        .id();
    let flat_flourish: Vec<Drum> = flat_score
        .events_of(flourish_voice)
        .map(|e| e.drum())
        .collect();
    assert_eq!(flat_flourish, vec![Drum::Ornament; 7]);
    let art_flourish: Vec<Drum> = art_score
        .events_of(flourish_voice)
        .map(|e| e.drum())
        .collect();
    assert_eq!(
        art_flourish,
        vec![
            Drum::HighTom,
            Drum::MidTom,
            Drum::Backbeat,
            Drum::Subdivision,
            Drum::HighTom,
            Drum::MidTom,
            Drum::Backbeat,
        ]
    );
}

#[test]
fn odd_beat_spondee_and_flourish_low_tom_articulation() {
    // Stanza 0: "bright\nbright\n" (regular, foundation).
    // Stanza 1: "bright\nbright hand or eye\n" (ragged, flourish).
    // In stanza 1's flourish line "bright hand or eye", the four stressed monosyllables scan as two
    // spondees:
    //   beat 0 ("bright hand"): HighTom (first primary, even), MidTom (second primary in spondee, even)
    //   beat 1 ("or eye"):      Backbeat (first primary, odd), LowTom (second primary in spondee, odd!)
    let source = "bright\nbright\n\nbright\nbright hand or eye\n";
    let art_score = articulated(source);
    let flourish_voice = art_score
        .voices()
        .iter()
        .find(|v| v.layer() == Layer::Flourish && v.span().start() > 14)
        .unwrap()
        .id();
    let drums: Vec<Drum> = art_score
        .events_of(flourish_voice)
        .map(|e| e.drum())
        .collect();
    assert_eq!(
        drums,
        vec![Drum::HighTom, Drum::MidTom, Drum::Backbeat, Drum::LowTom]
    );

    // Foundation odd-beat spondee:
    // Stanza 0: "bright\nbright hand or eye\n" (regular ballad-like foundation).
    // beat 0 ("bright hand"): Pulse (first primary, even), Backbeat (second primary in spondee, even)
    // beat 1 ("or eye"):      Backbeat (first primary, odd), Pulse (second primary in spondee, odd!)
    let found_source = "bright\nbright hand or eye\nbright\nbright hand or eye\n";
    let found_score = articulated(found_source);
    let found_voice = found_score
        .voices()
        .iter()
        .find(|v| v.span().start() >= 7)
        .unwrap()
        .id();
    let found_drums: Vec<Drum> = found_score
        .events_of(found_voice)
        .take(4) // line 1 ("bright hand or eye")
        .map(|e| e.drum())
        .collect();
    assert_eq!(
        found_drums,
        vec![Drum::Pulse, Drum::Backbeat, Drum::Backbeat, Drum::Pulse]
    );
}

#[test]
fn pyrrhic_foot_maintains_pulse_on_downbeat() {
    // "in the in the night": "in" (unstressed), "the" (unstressed), "in" (unstressed), "the" (unstressed), "night" (primary).
    // Beat 0 ("in the"): both syllables are unstressed (pyrrhic).
    // The first syllable ("in") takes Drum::Pulse so the downbeat pulse is not lost, and "the" takes Subdivision.
    // Beat 1 ("in the night"): anapest, ending on odd primary stress (Backbeat).
    let source = "in the in the night\nin the in the night\n";
    let score = articulated(source);
    let drums: Vec<Drum> = score.events().iter().take(5).map(|e| e.drum()).collect();
    assert_eq!(
        drums,
        vec![
            Drum::Pulse,
            Drum::Subdivision,
            Drum::Subdivision,
            Drum::Subdivision,
            Drum::Backbeat,
        ]
    );
}

#[test]
fn empty_poem_lowers_cleanly_under_articulated() {
    let score = articulated("");
    assert_eq!(score.drums(), Variant::Articulated);
    assert_eq!(score.articulated(), 0);
    assert!(score.events().is_empty());
}

#[test]
fn the_explain_view_names_the_variant_and_each_syllables_drum_piece() {
    let source = "Tyger Tyger burning bright\n";
    let view = |score: &Score| {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Table);
        let roles = role::assign(&meter, &repetition::analyse(&poem)).unwrap();
        annotate::render(&poem, &annotate::run(&poem, score, &roles))
    };
    let (flat_view, art_view) = (view(&flat(source)), view(&articulated(source)));
    assert!(!flat_view.contains("drums"), "{flat_view}");
    assert!(
        art_view.contains(
            "drums articulated · each syllable's drum piece is chosen by its layer, beat and \
             stress: 5 syllable(s) off the layer's flat piece (§4.1, §4.2)"
        ),
        "{art_view}"
    );
    assert!(art_view.contains("drums(pulse)"), "{art_view}");
    assert!(art_view.contains("drums(backbeat)"), "{art_view}");
    assert!(art_view.contains("drums(subdivision)"), "{art_view}");
}

fn table_poem() -> impl Strategy<Value = String> {
    let word = proptest::sample::select(WORDS.iter().map(|(word, _)| *word).collect::<Vec<_>>());
    let line = proptest::collection::vec(word, 1..6).prop_map(|words| words.join(" "));
    let stanza = proptest::collection::vec(line, 1..4).prop_map(|lines| lines.join("\n"));
    proptest::collection::vec(stanza, 1..4)
        .prop_map(|stanzas| format!("{}\n", stanzas.join("\n\n")))
}

fn every_with(pick: usize, variant: Variant) -> Mappings {
    let every = Mappings::every();
    every[pick % every.len()]
        .clone()
        .with(Mapping::Drums.choose(variant).unwrap())
}

fn shape_without_drum(score: &Score) -> Vec<String> {
    score
        .events()
        .iter()
        .map(|e| {
            format!(
                "{:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                e.span(),
                e.voice(),
                e.bar(),
                e.beat(),
                e.subdivision(),
                e.foot(),
                e.velocity(),
                e.is_light(),
                e.provenance(),
                e.pitch(),
                e.bass(),
                e.sharpness(),
                e.family(),
                e.anchor(),
            )
        })
        .collect()
}

proptest! {
    #[test]
    fn articulated_changes_only_drum_pieces(
        source in table_poem(),
        pick in 0_usize..1 << 16,
    ) {
        let flat = compile_with(&source, &every_with(pick, Variant::Flat));
        let articulated = compile_with(&source, &every_with(pick, Variant::Articulated));
        prop_assert_eq!(shape_without_drum(&flat), shape_without_drum(&articulated));
        prop_assert_eq!(flat.voices(), articulated.voices());
        prop_assert_eq!(flat.arrangement(), articulated.arrangement());
        prop_assert_eq!(flat.harmony(), articulated.harmony());
        prop_assert_eq!(flat.articulated(), 0);
        for event in flat.events() {
            let layer = flat.voice(event.voice()).unwrap().layer();
            prop_assert_eq!(event.drum(), Drum::flat_for(layer));
        }
        for event in articulated.events() {
            let layer = articulated.voice(event.voice()).unwrap().layer();
            match layer {
                Layer::Foundation => {
                    prop_assert!(matches!(
                        event.drum(),
                        Drum::Pulse | Drum::Backbeat | Drum::Subdivision
                    ));
                }
                Layer::Flourish => {
                    prop_assert!(matches!(
                        event.drum(),
                        Drum::HighTom
                            | Drum::MidTom
                            | Drum::LowTom
                            | Drum::Backbeat
                            | Drum::Subdivision
                    ));
                }
            }
        }
        let changed = articulated
            .events()
            .iter()
            .filter(|e| {
                let layer = articulated.voice(e.voice()).unwrap().layer();
                e.drum() != Drum::flat_for(layer)
            })
            .count();
        prop_assert_eq!(articulated.articulated(), changed);
    }

    #[test]
    fn i2_an_edit_leaves_other_stanzas_drums_alone(
        source in table_poem(),
        pick in 0_usize..64,
        with in proptest::sample::select(vec!["tyger", "hand", "bright", "burning", "the"]),
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

        let last_layer_and_drums = |text: &str| {
            let score = articulated(text);
            let parsed = Poem::parse(text.to_owned()).unwrap();
            let last = parsed.stanzas()[stanzas - 1].span();
            let events: Vec<_> = score
                .events()
                .iter()
                .filter(|e| last.contains(e.span()))
                .collect();
            let layers: Vec<Layer> = events
                .iter()
                .map(|e| score.voice(e.voice()).unwrap().layer())
                .collect();
            let drums: Vec<_> = events
                .iter()
                .map(|e| (e.span().start() - last.start(), e.drum()))
                .collect();
            (layers, drums)
        };
        let (before_layers, before_drums) = last_layer_and_drums(&source);
        let (after_layers, after_drums) = last_layer_and_drums(poem.source());
        prop_assume!(before_layers == after_layers);
        prop_assert_eq!(before_drums, after_drums);
    }
}
