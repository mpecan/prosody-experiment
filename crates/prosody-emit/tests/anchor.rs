//! Repetition anchor rendering (`Mapping::Anchor`, `SPEC.md` §4.3): under `off` (the default),
//! anchors are unrendered in Strudel and the output is bit-identical to the baseline; under
//! `on`, line-initial anaphora ostinatos are accented on the downbeat with punchy attack and
//! open cutoff, while line-final epistrophe cadences ring through turnarounds in wide reverb space.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)] // test code

use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::voice::{Anchor, Score};
use prosody_core::{
    Fallback, Mapping, Mappings, Poem, Variant, meter, repetition, rhyme, role, sound, surprisal,
};
use prosody_emit::{Backend as _, Strudel, Style};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

const ANAPHORA: &str = "What do you sell O ye merchants?\nWhat do you weigh, O ye venders?\n";
const EPISTROPHE: &str = "and the tide came in\nand the water came in\n";

fn compile(source: &str, variant: Variant) -> (Score, Style) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let meter = meter::scan(&poem, &phonemiser);
    let rep = repetition::analyse(&poem);
    let roles = role::assign(&meter, &rep).unwrap();
    let score = lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &phonemiser),
            repetition: &rep,
            sound: &sound::analyse(&poem, &phonemiser),
            surprisal: &surprisal::analyse(&poem, &phonemiser, &EnglishLexicon::new()),
            roles: &roles,
        },
        &Mappings::all().with(Mapping::Anchor.choose(variant).unwrap()),
    )
    .unwrap();
    (score, style)
}

#[test]
fn anaphora_and_epistrophe_only_render_in_strudel_under_anchor_on() {
    let (off_score, style) = compile(ANAPHORA, Variant::Off);
    let off_pattern = Strudel::new().render(&off_score, &style).unwrap();
    assert!(!off_pattern.contains("anchor = on"), "{off_pattern}");

    let (on_score, _) = compile(ANAPHORA, Variant::On);
    assert!(on_score.anchored() > 0);
    assert!(
        on_score
            .events()
            .iter()
            .any(|e| e.anchor() == Some(Anchor::Downbeat))
    );

    let on_pattern = Strudel::new().render(&on_score, &style).unwrap();
    assert!(
        on_pattern.contains("//   anchor = on: anaphora ostinatos accented on the downbeat"),
        "{on_pattern}"
    );
    assert_ne!(off_pattern, on_pattern);
}

#[test]
fn epistrophe_cadences_expand_room_under_anchor_on() {
    let (off_score, style) = compile(EPISTROPHE, Variant::Off);
    let off_pattern = Strudel::new().render(&off_score, &style).unwrap();

    let (on_score, _) = compile(EPISTROPHE, Variant::On);
    assert!(
        on_score
            .events()
            .iter()
            .any(|e| e.anchor() == Some(Anchor::Cadence))
    );

    let on_pattern = Strudel::new().render(&on_score, &style).unwrap();
    assert!(
        on_pattern.contains("//   anchor = on: anaphora ostinatos accented on the downbeat"),
        "{on_pattern}"
    );
    assert_ne!(off_pattern, on_pattern);
}
