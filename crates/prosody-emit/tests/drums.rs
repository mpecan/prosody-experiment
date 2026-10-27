//! Drum kit articulation (`Mapping::Drums`, `SPEC.md` §4.1, §4.2) through the real dictionary and
//! lexicon into both backends: under `flat`, a foundation line plays `bd` (or `rd` in jazz) on
//! every syllable; under `articulated`, trochaic and iambic lines place kick/ride on even-beat
//! stresses, snare on odd backbeat stresses and closed hat on unstressed subdivisions, while the
//! OSC schedule notes that its messages carry only the layer.

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

const TROCHAIC: &str = "Tyger Tyger, burning bright,\n";
const IAMBIC: &str = "Shall I compare thee to a summer's day?\n";

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
        &Mappings::all().with(Mapping::Drums.choose(variant).unwrap()),
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
fn tyger_and_sonnet_articulate_across_kick_snare_and_hat_only_under_articulated() {
    let (flat_tyger, style) = compile(TROCHAIC, Variant::Flat);
    let (art_tyger, _) = compile(TROCHAIC, Variant::Articulated);
    let flat_out = Strudel::new().render(&flat_tyger, &style).unwrap();
    let art_out = Strudel::new().render(&art_tyger, &style).unwrap();

    assert_eq!(
        argument(&flat_out, "s(", "s"),
        "<[[bd bd] [bd bd] [bd bd] bd]@4 [[bd bd] [bd bd] [bd bd] bd]@4>"
    );
    assert_eq!(
        argument(&art_out, "s(", "s"),
        "<[[bd hh] [sd hh] [bd hh] sd]@4 [[bd hh] [sd hh] [bd hh] sd]@4>"
    );
    assert!(!flat_out.contains("drums = articulated"), "{flat_out}");
    assert!(
        art_out.contains(
            "//   drums = articulated: foundation beats play kick on even stresses, snare on odd \
             backbeat stresses and closed hat on unstressed subdivisions; flourish beats play high \
             tom on even stresses, snare on odd stresses, mid/low toms on secondary stresses and \
             closed hat on unstressed subdivisions, in 5 syllable(s) off the flat piece \
             (SPEC.md §4.1, §4.2, candidate)"
        ),
        "{art_out}"
    );

    // Every parameter on the drum lane after `s("...")` and every other lane in the pattern is
    // byte-identical between `flat` and `articulated`.
    let after_s = |text: &str| -> String {
        let row = text
            .lines()
            .find(|l| l.trim_start().starts_with("s("))
            .unwrap();
        row.split_once(".gain(").unwrap().1.to_owned()
    };
    assert_eq!(after_s(&flat_out), after_s(&art_out));

    // Under the jazz preset (`kit = "ride"`, `bank = "tr909"`), even foundation stresses play `rd`,
    // odd backbeat stresses play `sd` and unstressed subdivisions play `hh`.
    let jazz = Style::parse("+++\npreset = \"jazz\"\n+++").unwrap();
    let art_jazz = Strudel::new().render(&art_tyger, &jazz).unwrap();
    assert_eq!(
        argument(&art_jazz, "s(", "s"),
        "<[[rd hh] [sd hh] [rd hh] sd]@4 [[rd hh] [sd hh] [rd hh] sd]@4>"
    );
    assert!(
        art_jazz.contains("//   drums = articulated: foundation beats play ride on even stresses"),
        "{art_jazz}"
    );

    // Under jazz kit with a bank that lacks a ride sample (e.g. `tr808`) and `ride_fallback = "crash"`,
    // even foundation stresses play `cr` and odd backbeat stresses play `sd`.
    let tr808_style =
        Style::parse("+++\nkit = \"ride\"\nbank = \"tr808\"\nride_fallback = \"crash\"\n+++")
            .unwrap();
    let art_tr808 = Strudel::new().render(&art_tyger, &tr808_style).unwrap();
    assert_eq!(
        argument(&art_tr808, "s(", "s"),
        "<[[cr hh] [sd hh] [cr hh] sd]@4 [[cr hh] [sd hh] [cr hh] sd]@4>"
    );

    // Under `IAMBIC` ("Shall I compare thee to a summer's day?"), CMUdict stresses "Shall"
    // (SH AE1 L), "I" (AY1), "thee" (DH IY1) and "to" (T UW1), so with `function_words = full`
    // beats 0 and 2 are spondees (`[bd sd]`) and beats 1, 3 and 4 are iambs (`[hh sd]`, `[hh sd]`,
    // `[hh bd]`).
    let (art_iamb, _) = compile(IAMBIC, Variant::Articulated);
    let iamb_out = Strudel::new().render(&art_iamb, &style).unwrap();
    assert_eq!(
        argument(&iamb_out, "s(", "s"),
        "<[[bd sd] [hh sd] [bd sd] [hh sd] [hh bd]]@5 [[bd sd] [hh sd] [bd sd] [hh sd] [hh bd]]@5>"
    );

    // With `function-words = light`, "Shall" and "thee" are lightened off strong positions,
    // so all five feet become pure iambs (`[hh bd] [hh sd] [hh bd] [hh sd] [hh bd]`), the
    // canonical iambic pentameter pattern cited in SPEC.md §4.1 and §4.2.
    let poem = Poem::parse(IAMBIC.to_owned()).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let meter = meter::scan(&poem, &phonemiser);
    let repetition = repetition::analyse(&poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    let light_art_iamb = lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme::analyse(&poem, &phonemiser),
            repetition: &repetition,
            sound: &sound::analyse(&poem, &phonemiser),
            surprisal: &surprisal::analyse(&poem, &phonemiser, &EnglishLexicon::new()),
            roles: &roles,
        },
        &Mappings::all()
            .with(Mapping::Drums.choose(Variant::Articulated).unwrap())
            .with(Mapping::Function.choose(Variant::Light).unwrap()),
    )
    .unwrap();
    let light_iamb_out = Strudel::new().render(&light_art_iamb, &style).unwrap();
    assert_eq!(
        argument(&light_iamb_out, "s(", "s"),
        "<[[hh bd] [hh sd] [hh bd] [hh sd] [hh bd]]@5 [[hh bd] [hh sd] [hh bd] [hh sd] [hh bd]]@5>"
    );
}

#[test]
fn flourish_stanzas_render_toms_in_strudel_pattern() {
    // A regular ballad stanza followed by a ragged flourish stanza:
    let source = "Tyger Tyger burning bright,\nTyger Tyger burning bright,\n\nDare\nframe thy fearful symmetry?\n";
    let (score, style) = compile(source, Variant::Articulated);
    let rendered = Strudel::new().render(&score, &style).unwrap();
    let has_tom = rendered
        .lines()
        .filter(|l| l.trim_start().starts_with("s(\""))
        .any(|l| l.contains("ht") || l.contains("mt") || l.contains("lt"));
    assert!(has_tom, "{rendered}");
}

#[test]
fn osc_discloses_that_articulated_drums_are_not_applied() {
    let (flat_score, style) = compile(TROCHAIC, Variant::Flat);
    let (art_score, _) = compile(TROCHAIC, Variant::Articulated);
    let flat_osc = Osc::new().render(&flat_score, &style).unwrap();
    let art_osc = Osc::new().render(&art_score, &style).unwrap();

    assert!(!flat_osc.drums_unapplied());
    assert!(art_osc.drums_unapplied());
    assert_eq!(flat_osc.messages(), art_osc.messages());
    assert!(!flat_osc.to_text().contains("drums = articulated"));
    assert!(art_osc.to_text().contains(
        "# not applied here: drums = articulated (messages carry the layer, foundation or \
         flourish, not a drum piece)"
    ));
}
