//! The light function words (`Mapping::Function`, `SPEC.md` §4.2) through the real dictionary and
//! the real lexicon, into both backends: "or" in The Tyger's "What immortal hand or eye" is struck
//! at 2/3 under light and at full force under full, in the Strudel gains, the bass lane and the
//! OSC velocities alike, and nothing else in either output moves.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // test code

use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::Score;
use prosody_core::{
    Fallback, Mapping, Mappings, Poem, Variant, meter, repetition, rhyme, role, sound, surprisal,
};
use prosody_emit::{Backend as _, Osc, Strudel, Style};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

const LINE: &str = "What immortal hand or eye,\n";

fn compile(source: &str, variant: Variant) -> (Score, Style) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let meter = meter::scan(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    let score = lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &phonemiser),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &phonemiser),
            surprisal: &surprisal::analyse(&poem, &phonemiser, &EnglishLexicon::new()),
            roles: &roles,
        },
        &Mappings::all().with(Mapping::Function.choose(variant).unwrap()),
    )
    .unwrap();
    (score, style)
}

/// The quoted argument of the first `name("…")` call on the line starting with `lane`.
fn argument<'a>(pattern: &'a str, lane: &str, name: &str) -> &'a str {
    let row = pattern
        .lines()
        .find(|candidate| candidate.trim_start().starts_with(lane))
        .unwrap();
    let open = format!("{name}(\"");
    let start = row.find(&open).unwrap() + open.len();
    &row[start..start + row[start..].find('"').unwrap()]
}

#[test]
fn or_is_light_in_the_strudel_gains_and_the_bass_only_under_light() {
    let (full, style) = compile(LINE, Variant::Full);
    let (light, _) = compile(LINE, Variant::Light);
    let (full, light) = (
        Strudel::new().render(&full, &style).unwrap(),
        Strudel::new().render(&light, &style).unwrap(),
    );
    // CMUdict: WHAT AH1 · IMMORTAL IH2 AO1 AH0 · HAND AE1 · OR AO1 · EYE AY1. The meter scans
    // trochee (What im), trochee (mor tal), spondee (hand or), single (eye); the line falls, so the
    // spondee's strong position is "hand". Gains in thirds: 1, 2/3, 1, 1/3, 1, then "or" 1 under
    // full and 2/3 under light, then 1 — the stated bar, and the outro's pass, the same.
    let bar = |or: &str| format!("[[1.000 0.667] [1.000 0.333] [1.000 {or}] 1.000]@4");
    assert_eq!(
        argument(&full, "s(", "gain"),
        format!("<{} {}>", bar("1.000"), bar("1.000"))
    );
    assert_eq!(
        argument(&light, "s(", "gain"),
        format!("<{} {}>", bar("0.667"), bar("0.667"))
    );
    // The height bass plays primary-stressed foundation syllables: "or" loses its note.
    assert_eq!(
        argument(&full, "note(", "note"),
        "<[[39 ~] [36 ~] [36 36] 36]@4 [[39 ~] [36 ~] [36 36] 36]@4>"
    );
    assert_eq!(
        argument(&light, "note(", "note"),
        "<[[39 ~] [36 ~] [36 ~] 36]@4 [[39 ~] [36 ~] [36 ~] 36]@4>"
    );
    // The header names it under light, and only under light.
    let named = "//   function-words = light: a one-syllable function word off the metre's strong \
                 positions is struck at 2/3 of full force, in 1 syllable(s) (SPEC.md §4.2, \
                 candidate)";
    assert!(light.contains(named), "{light}");
    assert!(!full.contains("function-words"), "{full}");
    // Every other line of the pattern is byte-identical: the drum lane and the bass lane differ,
    // and the one header line is added.
    let others = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| !line.contains("function-words"))
            .filter(|line| {
                !line.trim_start().starts_with("s(") && !line.trim_start().starts_with("note(")
            })
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(others(&full), others(&light));
}

#[test]
fn or_is_sent_at_two_thirds_velocity_only_under_light() {
    let (full, style) = compile(LINE, Variant::Full);
    let (light, _) = compile(LINE, Variant::Light);
    let (full, light) = (
        Osc::new().render(&full, &style).unwrap(),
        Osc::new().render(&light, &style).unwrap(),
    );
    assert_eq!(full.lightened(), None);
    assert_eq!(light.lightened(), Some(1));
    let (full, light) = (full.to_text(), light.to_text());
    assert!(light.contains(
        "# function-words = light: a one-syllable function word off the metre's strong \
         positions sent at 2/3 velocity, in 1 syllable(s)"
    ));
    // "or" sounds at beat 2½ of the stated bar and 6½ in the outro. Its drum velocity falls from
    // 1 to 2/3; it has no bass note any more (degree -1), so the note velocity (the last field)
    // is its own 2/3 too. Every other message is as it was.
    let notes = |text: &str| -> Vec<String> {
        text.lines()
            .filter(|line| line.contains("/prosody/note"))
            .map(str::to_owned)
            .collect()
    };
    let (full_notes, light_notes) = (notes(&full), notes(&light));
    assert_eq!(full_notes.len(), light_notes.len());
    let changed: Vec<(&String, &String)> = full_notes
        .iter()
        .zip(&light_notes)
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(changed.len(), 2, "{changed:?}");
    for (before, after) in changed {
        assert!(before.contains("\"foundation\" 0 1.000 36 "), "{before}");
        assert!(before.ends_with(" 1.000 1.000"), "{before}");
        assert!(after.contains("\"foundation\" 0 0.667 -1 "), "{after}");
        assert!(after.ends_with(" 1.000 0.667"), "{after}");
        assert!(
            before.trim_start().starts_with("2.500") || before.trim_start().starts_with("6.500")
        );
    }
}
