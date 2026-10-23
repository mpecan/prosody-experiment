//! Invariant I3, at last testable: style is palette, not structure.
//!
//! > **I3 — Style is palette, not structure.** The moment the style block can alter arrangement,
//! > the poem stops being the score and becomes an ornament on a preset.
//!
//! `SPEC.md` §8 lists this among the properties worth having early — "changing only the style
//! header never changes the event *structure*, only its realisation" — and it could not be
//! written until a style existed. It is the last of the four invariants to get a test.
//!
//! Two halves, and both are needed. That a style change leaves the score untouched is *necessary*
//! but not sufficient: a backend that ignored the style entirely would pass it. So the second half
//! asserts that the style does reach the output.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_core::lower::{self, Analyses};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::{Backend as _, Strudel, Style};

/// Spelling is pronunciation, so a test can choose a poem's shape by writing it.
struct Spelled;

impl Phonemiser for Spelled {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        let phones: Vec<_> = word
            .chars()
            .filter_map(|letter| match letter {
                'A' => Some((ARPABET.phoneme("AA").unwrap(), Stress::Primary)),
                'e' => Some((ARPABET.phoneme("IH").unwrap(), Stress::Unstressed)),
                'o' => Some((ARPABET.phoneme("OW").unwrap(), Stress::Primary)),
                'b' => Some((ARPABET.phoneme("B").unwrap(), Stress::Unstressed)),
                'd' => Some((ARPABET.phoneme("D").unwrap(), Stress::Unstressed)),
                'n' => Some((ARPABET.phoneme("N").unwrap(), Stress::Unstressed)),
                'r' => Some((ARPABET.phoneme("R").unwrap(), Stress::Unstressed)),
                's' => Some((ARPABET.phoneme("S").unwrap(), Stress::Unstressed)),
                't' => Some((ARPABET.phoneme("T").unwrap(), Stress::Unstressed)),
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

/// The body of a poem, without any header.
///
/// Deliberately not uniform: a refrain (so two lines share a voice), a ragged stanza (so the
/// §4.1 fallback fires and some voice is a flourish), a word family, and two rhyme classes. An
/// earlier fixture was four regular lines, every voice `Foundation` with identical sharpness, so
/// half the fields the tests compared were constant and the comparisons were free.
const BODY: &str = "teA bAn\nteA rAn\nteA bAn\n\nbo\nteA dot roso beA nAt\nteA rot\n";

/// Two palettes with nothing in common.
const ONE: &str = "+++\n[style]\nscale = \"phrygian_dominant\"\nroot = \"D\"\n\
                   tempo_range = [128, 134]\nswing = 0.12\nbank = \"tr909\"\nsynth = \"karplus\"\n+++\n";
const TWO: &str = "+++\n[style]\nscale = \"lydian\"\nroot = \"A\"\n\
                   tempo_range = [70, 72]\nswing = 0\nbank = \"casio\"\nsynth = \"sawtooth\"\n+++\n";

fn score_of(source: &str) -> (Score, Style) {
    score_with(source, &prosody_core::Mappings::all())
}

/// [`score_of`], with these candidate mappings.
fn score_with(source: &str, mappings: &prosody_core::Mappings) -> (Score, Style) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let meter = meter::scan(&poem, &Spelled);
    let rhyme = rhyme::analyse(&poem, &Spelled);
    let repetition = repetition::analyse(&poem);
    let sound = sound::analyse(&poem, &Spelled);
    let surprisal = surprisal::analyse(&poem, &Spelled, &Flat);
    let roles = role::assign(&meter, &repetition).unwrap();
    let score = lower::run_with(
        Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme,
            repetition: &repetition,
            sound: &sound,
            surprisal: &surprisal,
            roles: &roles,
        },
        mappings,
    )
    .unwrap();
    (score, style)
}

#[test]
fn i3_changing_the_style_leaves_the_score_identical() {
    // The whole IR, compared. Not a summary of it — an event count would pass while every note
    // moved.
    let (bare, _) = score_of(BODY);
    let (one, _) = score_of(&format!("{ONE}{BODY}"));
    let (two, _) = score_of(&format!("{TWO}{BODY}"));

    // The header shifts every byte offset in the poem, so spans legitimately differ. Everything
    // else about what is played must not.
    let shape = |score: &Score| {
        score
            .events()
            .iter()
            .map(|e| {
                format!(
                    "{} {} {}/{} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                    e.bar().get(),
                    e.beat().get(),
                    e.subdivision().index(),
                    e.subdivision().of(),
                    e.foot(),
                    e.velocity(),
                    e.sharpness(),
                    e.pitch().map(|p| (p.degree().get(), p.detune())),
                    // Its span moves with the header, as every span does; the note must not.
                    e.bass()
                        .map(|b| (b.root().get(), b.step().get(), b.chromatic(), b.velocity())),
                    e.family().map(|f| (f.voice(), f.depth())),
                    e.anchor(),
                    e.voice(),
                    e.bank(),
                )
            })
            .collect::<Vec<_>>()
    };

    // The fixture must actually exercise the cases, or the comparison is over constants.
    assert!(
        bare.voices().len() < bare.bars(),
        "a refrain shares a voice between lines"
    );
    assert!(
        bare.voices()
            .iter()
            .any(|v| v.layer() == prosody_core::role::Layer::Flourish),
        "some voice is a flourish"
    );
    assert!(
        bare.events().iter().any(|e| e.family().is_some()),
        "a family"
    );

    assert_eq!(shape(&bare), shape(&one));
    assert_eq!(shape(&one), shape(&two));
    assert_eq!(bare.bars(), two.bars());
    assert_eq!(bare.voices().len(), two.voices().len());
    for (a, b) in bare.voices().iter().zip(two.voices()) {
        assert_eq!(a.id(), b.id());
        assert_eq!(a.layer(), b.layer(), "a style cannot re-role a stanza");
        // The seed drives per-unit jitter (`CLAUDE.md` §5). A header edit that re-seeded a voice
        // would change every random choice in the poem while leaving the events untouched.
        assert_eq!(a.seed(), b.seed(), "a style cannot re-seed a voice");
    }
}

#[test]
fn i3_but_the_style_does_reach_the_output() {
    // The other half. "The score is unchanged" is satisfied trivially by a backend that ignores
    // the style, so I3 needs this too — otherwise the first test is a tautology about a preset
    // nobody applies.
    let (score, one) = score_of(&format!("{ONE}{BODY}"));
    let (_, two) = score_of(&format!("{TWO}{BODY}"));

    let a = Strudel::new().render(&score, &one).unwrap();
    let b = Strudel::new().render(&score, &two).unwrap();
    assert_ne!(a, b, "two palettes must not sound the same");

    assert!(a.contains("tr909") && b.contains("casio"), "bank");
    assert!(a.contains("karplus") && b.contains("sawtooth"), "synth");
    assert!(a.contains(".swingBy"), "swing reaches the output");
    assert!(!b.contains(".swingBy"), "and no swing means none");
    // Tempo comes from the style's window, positioned by the poem's density, then divided by the
    // beats in a bar because `cpm` counts *cycles* and a cycle is a bar. Both styles must land in
    // their own window before that division, and the windows do not overlap — so the two tempos
    // cannot coincide however the poem is written.
    let cpm = |text: &str| -> f64 {
        text.lines()
            .find_map(|line| line.trim().strip_prefix(".cpm("))
            .and_then(|rest| rest.trim_end_matches(')').parse().ok())
            .unwrap_or(f64::NAN)
    };
    assert!(cpm(&a) > cpm(&b) * 1.5, "{} against {}", cpm(&a), cpm(&b));
    // The windows do not overlap, so no poem could have produced both — the style really is the
    // constraint here, and the poem only positions inside it.
    assert!(two.tempo_range().high().get() < one.tempo_range().low().get());
}

/// The mini-notation of a rendered layer, with every palette token blanked out.
///
/// What survives is the *arrangement*: how many layers, how many bars, how many steps, where the
/// rests are. That is what I3 protects, and it is the comparison the earlier tests were missing —
/// five separate mutations letting the style delete events, reorder the melody, silence bars, or
/// double the tempo all passed a suite that only checked substrings and inequality.
///
/// A chord — one step whose tones are stacked with `,`, `[62,65,69,72]` — is one value: it is one
/// event of the score voiced as several tones (`prosody_emit::Voicing`), and its tones all take
/// that event's one gain, cutoff and stray. [`chords_are_one_event`] checks that claim directly
/// rather than letting this function take it on trust.
///
/// Per layer, the *set* of distinct shapes among its patterns. A palette may add a parameter
/// pattern — `humanise` adds `.nudge` and `.velocity`, and zero leaves them out — but every one
/// is laid on the layer's own grid, so it adds no shape the layer did not already have. A palette
/// that moved, added or dropped a single event would change some pattern's shape and so the set.
fn arrangement(text: &str) -> Vec<std::collections::BTreeSet<String>> {
    text.lines()
        .filter(|line| line.starts_with("  s(") || line.starts_with("  note("))
        .map(|line| {
            let mut shapes = std::collections::BTreeSet::new();
            let mut masked = String::new();
            let mut in_pattern = false;
            for part in line.split('"') {
                if in_pattern {
                    // Keep the shape — brackets, rests, separators — and blank the values.
                    for token in part.split_whitespace() {
                        let token = collapse_chords(token);
                        let shape: String = token
                            .chars()
                            .map(|c| match c {
                                '[' | ']' | '<' | '>' | '~' => c,
                                _ => 'v',
                            })
                            .collect();
                        // Collapse runs of `v` so a note number and a gain compare equal.
                        let mut collapsed = String::new();
                        let mut last = ' ';
                        for c in shape.chars() {
                            if !(c == 'v' && last == 'v') {
                                collapsed.push(c);
                            }
                            last = c;
                        }
                        masked.push_str(&collapsed);
                        masked.push(' ');
                    }
                    shapes.insert(std::mem::take(&mut masked));
                }
                in_pattern = !in_pattern;
            }
            shapes
        })
        .collect()
}

/// A token with every chord in it — `[` numbers joined by `,` `]` — written as the single value `v`.
fn collapse_chords(token: &str) -> String {
    let mut out = String::new();
    let mut rest = token;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let body_end = after
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-'))
            .unwrap_or(after.len());
        let body = &after[..body_end];
        if body.contains(',') && after[body_end..].starts_with(']') {
            out.push('v');
            rest = &after[body_end + 1..];
        } else {
            out.push('[');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn chords_collapse_to_one_value_and_nothing_else_does() {
    assert_eq!(collapse_chords("[62,65,69,72]"), "v");
    assert_eq!(collapse_chords("[~"), "[~");
    assert_eq!(collapse_chords("[62,65,69,72]]"), "v]");
    assert_eq!(collapse_chords("<[~"), "<[~");
    assert_eq!(collapse_chords("[60.500,63.500]"), "v");
    // A bracket of steps is structure, not a chord, and survives.
    assert_eq!(collapse_chords("[60"), "[60");
}

#[test]
fn i3_the_rendered_arrangement_is_the_same_under_any_palette() {
    // The half of I3 that is actually at risk. Comparing two `Score`s cannot fail by construction
    // — `prosody-core` cannot see a `Style` — so the test that matters is at the emitter, the one
    // place a style is legitimately readable.
    let (score, one) = score_of(&format!("{ONE}{BODY}"));
    let (_, two) = score_of(&format!("{TWO}{BODY}"));
    let a = Strudel::new().render(&score, &one).unwrap();
    let b = Strudel::new().render(&score, &two).unwrap();

    let shape_a = arrangement(&a);
    let shape_b = arrangement(&b);
    assert!(shape_a.len() > 2, "the fixture must have several layers");
    assert_eq!(shape_a, shape_b, "\n{a}\n---\n{b}");

    // ...and nothing outside the layers may differ structurally either.
    let strip = |text: &str| {
        text.lines()
            .filter(|line| !line.starts_with("//") && !line.trim_start().starts_with(".cpm"))
            .filter(|line| !line.trim_start().starts_with(".swingBy"))
            .count()
    };
    assert_eq!(
        strip(&a),
        strip(&b),
        "the same number of layers and brackets"
    );
}

#[test]
fn i3_the_notes_move_but_the_degrees_do_not() {
    // The sharpest form of the invariant. A scale and a root change every pitch that sounds, and
    // change nothing about which lines share a pitch — that is what "palette, not structure"
    // means when the palette is a set of notes.
    let (score, one) = score_of(&format!("{ONE}{BODY}"));
    let (_, two) = score_of(&format!("{TWO}{BODY}"));

    // The rhyme notes only: the bass's degrees come from vowels, and are pinned by the same
    // comparison of note lines below.
    let degrees: Vec<_> = score
        .events()
        .iter()
        .filter_map(|e| e.pitch())
        .filter(|p| p.class().is_some())
        .map(|p| p.degree().get())
        .collect();
    // AABB with the first class returning: "bAn" appears twice as a refrain and once more, then
    // "dot"/"rot". Two classes, five pitched events. (An earlier comment called `[0,0,1,1]` ABAB;
    // it is paired resolution, which §4.5 lists as a different consequence.)
    assert_eq!(degrees, vec![0, 0, 0, 1, 1]);

    let a = Strudel::new().render(&score, &one).unwrap();
    let b = Strudel::new().render(&score, &two).unwrap();
    let notes = |text: &str| {
        text.lines()
            .find(|line| line.contains("note("))
            .unwrap()
            .to_owned()
    };
    assert_ne!(notes(&a), notes(&b), "different scales, different notes");

    // Hand-worked from the two palettes. The melody's octave starts at MIDI 60.
    //   D phrygian dominant: root D = 2, degree 0 = 0 semitones  -> 60 + 2 = 62
    //   A lydian:            root A = 9, degree 0 = 0 semitones  -> 60 + 9 = 69
    let first = |text: &str| {
        notes(text)
            .split('"')
            .nth(1)
            .unwrap_or_default()
            .replace(['<', '>', '[', ']'], " ")
            .split_whitespace()
            // A bar's `@` weight is how long it lasts, not a value.
            .map(|t| t.split('@').next().unwrap_or(t))
            .filter(|t| !t.is_empty())
            .find(|token| *token != "~")
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(first(&a), "62", "{}", notes(&a));
    assert_eq!(first(&b), "69", "{}", notes(&b));
}

#[test]
fn i1_rendering_is_reproducible() {
    let (score, style) = score_of(&format!("{ONE}{BODY}"));
    let backend = Strudel::new();
    assert_eq!(
        backend.render(&score, &style).unwrap(),
        backend.render(&score, &style).unwrap()
    );
}

#[test]
fn a_score_with_nothing_to_play_says_so() {
    // CLAUDE.md §6: a backend that emitted a pattern of silence would look like it worked.
    let (score, style) = score_of("zz\n");
    assert!(score.events().is_empty());
    assert!(Strudel::new().render(&score, &style).is_err());
}

#[test]
fn odd_metres_survive_into_the_pattern() {
    // SPEC.md §4.2: "iambic pentameter yields five beats. Poetry naturally produces odd meters.
    // Don't quantise to 4/4." A five-foot line must emit five steps, not four or eight — in each
    // of its two passes, the statement and the outro (§4.6).
    let (score, style) = score_of("bA bA bA bA bA\n");
    let text = Strudel::new().render(&score, &style).unwrap();
    let pattern = text
        .lines()
        .find(|line| line.contains("s(\""))
        .unwrap()
        .split('"')
        .nth(1)
        .unwrap()
        .to_owned();
    assert_eq!(
        pattern.split_whitespace().count(),
        10,
        "five feet, five steps: {pattern}"
    );
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

    /// I3, as a property rather than an example — `SPEC.md` §8 lists it among the three worth
    /// having early, and it was the only invariant of the four with no property test.
    ///
    /// For an arbitrary poem and any two palettes, the *arrangement* of the rendered text is the
    /// same: same layers, same bars, same steps, same rests. Only the values differ.
    #[test]
    fn i3_no_palette_can_change_the_arrangement(
        body in poem(),
        one in palette(),
        two in palette(),
    ) {
        let (score, style_one) = score_of(&format!("{one}{body}"));
        let (_, style_two) = score_of(&format!("{two}{body}"));
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (
            backend.render(&score, &style_one),
            backend.render(&score, &style_two),
        ) else {
            // A poem nobody can pronounce renders under neither palette, which is itself
            // style-independent.
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
    }

    /// I3 with the harmony layer on (`SPEC.md` §4.6): the chord lane is one more pattern, and no
    /// palette moves a chord, adds one or rests one — its instrument, tones, register and level
    /// are palette, which slots have a chord is the score's.
    #[test]
    fn i3_no_palette_can_change_the_chord_lane(
        body in poem(),
        one in palette(),
        two in palette(),
        harmony in proptest::sample::select(vec![
            prosody_core::Variant::Rhyme,
            prosody_core::Variant::Cadence,
            prosody_core::Variant::Both,
        ]),
    ) {
        let mappings = prosody_core::Mappings::all()
            .with(prosody_core::Mapping::Harmony.choose(harmony).unwrap());
        let (score, style_one) = score_with(&format!("{one}{body}"), &mappings);
        let (_, style_two) = score_with(&format!("{two}{body}"), &mappings);
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (
            backend.render(&score, &style_one),
            backend.render(&score, &style_two),
        ) else {
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
        let lane = |text: &str| text.lines().filter(|l| l.contains("// the chords")).count();
        proptest::prop_assert_eq!(lane(&a), lane(&b));
        proptest::prop_assert_eq!(lane(&a), usize::from(score.chords().next().is_some()));
    }

    /// I3 with a walking bass (`SPEC.md` §4.6, W1 and W2), with or without the harmony: no palette
    /// adds, moves or rests a bass note. Which scale, root note and octave it sounds in are
    /// palette; which syllables and beats carry one, and on which step, are the score's.
    #[test]
    fn i3_no_palette_can_change_the_walking_bass(
        body in poem(),
        one in palette(),
        two in palette(),
        bass in proptest::sample::select(vec![
            prosody_core::Variant::Syllables,
            prosody_core::Variant::Feet,
        ]),
        harmony in proptest::sample::select(vec![
            prosody_core::Variant::Off,
            prosody_core::Variant::Rhyme,
            prosody_core::Variant::Cadence,
        ]),
    ) {
        let mappings = prosody_core::Mappings::all()
            .with(prosody_core::Mapping::Bass.choose(bass).unwrap())
            .with(prosody_core::Mapping::Harmony.choose(harmony).unwrap());
        let (score, style_one) = score_with(&format!("{one}{body}"), &mappings);
        let (_, style_two) = score_with(&format!("{two}{body}"), &mappings);
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (
            backend.render(&score, &style_one),
            backend.render(&score, &style_two),
        ) else {
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
        let lane = |text: &str| text.lines().filter(|l| l.contains("// the bass")).count();
        proptest::prop_assert_eq!(lane(&a), lane(&b));
        proptest::prop_assert_eq!(
            lane(&a) > 0,
            score.events().iter().any(|e| e.bass().is_some())
        );
    }

    /// The chord voicing, isolated: one palette rendered with each rhyme note alone and again with
    /// it voiced as a seventh chord. The claim `Voicing` makes against I3 is that this changes the
    /// realisation of each pitched event and nothing else — same events, same positions — so:
    ///
    /// - every line but the melody's is byte for byte the same, header comments aside;
    /// - the melody's pattern has the same steps, and where one has a note the other has a chord
    ///   whose lowest tone *is* that note, with the 3rd, 5th and 7th above it — four tones,
    ///   always, so a chord never stands for more than one event;
    /// - and the melody's other parameters are identical but for the level, shared between the
    ///   tones.
    #[test]
    fn i3_chords_are_one_event_voiced_and_change_nothing_else(body in poem(), one in palette()) {
        let single = one.replace("voicing = \"seventh\"", "voicing = \"note\"");
        let seventh = one.replace("voicing = \"note\"", "voicing = \"seventh\"");
        let (score, style_single) = score_of(&format!("{single}{body}"));
        let (_, style_seventh) = score_of(&format!("{seventh}{body}"));
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (
            backend.render(&score, &style_single),
            backend.render(&score, &style_seventh),
        ) else {
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
        let music = |text: &str| -> Vec<String> {
            text.lines().filter(|l| !l.starts_with("//")).map(str::to_owned).collect()
        };
        let (a_lines, b_lines) = (music(&a), music(&b));
        proptest::prop_assert_eq!(a_lines.len(), b_lines.len());
        for (x, y) in a_lines.iter().zip(&b_lines) {
            if !x.contains("// the melody") {
                proptest::prop_assert_eq!(x, y);
                continue;
            }
            chords_are_one_event(x, y, &style_seventh)?;
        }
    }

    /// The voicing under the harmony layer (`SPEC.md` §4.6, the melody over the chords): with any
    /// harmony variant on, the rhyme melody is single notes whatever `voicing` says, and the
    /// voicing shapes only the chord lane. So between `note` and `seventh` under the harmony:
    ///
    /// - the arrangement is the same (I3: no event moves);
    /// - every line but the chord lane's is byte for byte the same, the melody's included — its
    ///   level too, since a single note takes the single note's level under either voicing;
    /// - no step of the melody is a stack (`,` never appears in its note pattern);
    /// - and the chord lane, where there is one, has the same steps with three tones a chord under
    ///   `note` and four under `seventh`.
    #[test]
    fn i3_under_the_harmony_the_voicing_shapes_only_the_chord_lane(
        body in poem(),
        one in palette(),
        harmony in proptest::sample::select(vec![
            prosody_core::Variant::Rhyme,
            prosody_core::Variant::Cadence,
            prosody_core::Variant::Both,
        ]),
    ) {
        let single = one.replace("voicing = \"seventh\"", "voicing = \"note\"");
        let seventh = one.replace("voicing = \"note\"", "voicing = \"seventh\"");
        let mappings = prosody_core::Mappings::all()
            .with(prosody_core::Mapping::Harmony.choose(harmony).unwrap());
        let (score, style_single) = score_with(&format!("{single}{body}"), &mappings);
        let (_, style_seventh) = score_with(&format!("{seventh}{body}"), &mappings);
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (
            backend.render(&score, &style_single),
            backend.render(&score, &style_seventh),
        ) else {
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
        let music = |text: &str| -> Vec<String> {
            text.lines().filter(|l| !l.starts_with("//")).map(str::to_owned).collect()
        };
        let (a_lines, b_lines) = (music(&a), music(&b));
        proptest::prop_assert_eq!(a_lines.len(), b_lines.len());
        let tones = |line: &str| -> Vec<usize> {
            let pattern = line.split('"').nth(1).unwrap_or_default();
            pattern
                .split(|c: char| c.is_whitespace() || c == '<' || c == '>')
                .map(|t| t.split('@').next().unwrap_or(t))
                .map(|t| t.trim_start_matches('[').trim_end_matches(']'))
                .filter(|t| !t.is_empty() && *t != "~")
                .map(|t| t.split(',').count())
                .collect()
        };
        for (x, y) in a_lines.iter().zip(&b_lines) {
            if x.contains("// the chords") {
                let (three, four) = (tones(x), tones(y));
                proptest::prop_assert_eq!(three.len(), four.len());
                proptest::prop_assert!(three.iter().all(|n| *n == 3), "{}", x);
                proptest::prop_assert!(four.iter().all(|n| *n == 4), "{}", y);
                continue;
            }
            proptest::prop_assert_eq!(x, y);
            if x.contains("// the melody") {
                proptest::prop_assert!(tones(x).iter().all(|n| *n == 1), "{}", x);
            }
        }
        // The header says so under either voicing, wherever there is a chord to be over.
        let header = "as single notes over the chords";
        proptest::prop_assert_eq!(a.contains(header), score.chords().next().is_some());
        proptest::prop_assert_eq!(b.contains(header), score.chords().next().is_some());
    }

    /// Levels that follow density, isolated. `density_level` is the one palette value that reads
    /// the score's structure — how many lines each bar sounds — so the claim it makes against I3 is
    /// checked directly: between `0` and any other amount, every line of the output is byte for
    /// byte the same once its `.gain` pattern is taken out, and each gain is at most what it was.
    #[test]
    fn i3_density_changes_only_gain_values(body in poem(), one in palette(), level in 1_u8..=100) {
        let set = |amount: f64| {
            one.lines()
                .map(|line| {
                    if line.starts_with("density_level") {
                        format!("density_level = {amount}")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let (score, flat) = score_of(&format!("{}\n{body}", set(0.0)));
        let (_, dense) = score_of(&format!("{}\n{body}", set(f64::from(level) / 100.0)));
        let backend = Strudel::new();
        let (Ok(a), Ok(b)) = (backend.render(&score, &flat), backend.render(&score, &dense)) else {
            return Ok(());
        };
        proptest::prop_assert_eq!(arrangement(&a), arrangement(&b));
        let music = |text: &str| -> Vec<(String, Vec<f64>)> {
            text.lines()
                .filter(|l| !l.starts_with("//"))
                .map(|line| {
                    let mut kept = String::new();
                    let mut gains = Vec::new();
                    let mut rest = line;
                    while let Some(at) = rest.find(".gain(\"") {
                        kept.push_str(&rest[..at]);
                        let after = &rest[at + 7..];
                        let end = after.find('"').unwrap();
                        gains.extend(
                            after[..end]
                                .replace(['<', '>', '[', ']'], " ")
                                .split_whitespace()
                // A bar's `@` weight is how long it lasts, not a value.
                .map(|t| t.split('@').next().unwrap_or(t))
                .filter(|t| !t.is_empty())
                                .filter(|t| *t != "~")
                                .map(|t| t.parse::<f64>().unwrap()),
                        );
                        rest = &after[end + 2..];
                    }
                    kept.push_str(rest);
                    (kept, gains)
                })
                .collect()
        };
        let (x, y) = (music(&a), music(&b));
        proptest::prop_assert_eq!(x.len(), y.len());
        for ((line_a, gains_a), (line_b, gains_b)) in x.iter().zip(&y) {
            proptest::prop_assert_eq!(line_a, line_b);
            proptest::prop_assert_eq!(gains_a.len(), gains_b.len());
            for (g, h) in gains_a.iter().zip(gains_b) {
                proptest::prop_assert!(h <= g, "{} became {}", g, h);
            }
        }
    }

    /// ...and the score the arrangement comes from is untouched too.
    #[test]
    fn i3_no_palette_can_change_the_score(body in poem(), one in palette(), two in palette()) {
        let (bare, _) = score_of(&body);
        let (with_one, _) = score_of(&format!("{one}{body}"));
        let (with_two, _) = score_of(&format!("{two}{body}"));
        let shape = |score: &Score| {
            score
                .events()
                .iter()
                .map(|e| {
                    format!(
                        "{} {} {}/{} {:?} {:?} {:?} {:?} {:?} {:?}",
                        e.bar().get(),
                        e.beat().get(),
                        e.subdivision().index(),
                        e.subdivision().of(),
                        e.velocity(),
                        e.sharpness(),
                        e.pitch().map(|p| (p.degree().get(), p.detune())),
                        // Its span moves with the header, as every span does; the note must not.
                        e.bass()
                            .map(|b| (b.root().get(), b.step().get(), b.chromatic(), b.velocity())),
                        e.family().map(|f| (f.voice(), f.depth())),
                        e.voice(),
                    )
                })
                .collect::<Vec<_>>()
        };
        proptest::prop_assert_eq!(shape(&bare), shape(&with_one));
        proptest::prop_assert_eq!(shape(&with_one), shape(&with_two));
    }
}

/// One melody lane under `note` and `seventh` voicing: step for step, a note becomes the chord built
/// on it, and only the level changes beside it.
fn chords_are_one_event(
    single: &str,
    seventh: &str,
    style: &Style,
) -> Result<(), proptest::test_runner::TestCaseError> {
    let pattern = |line: &str| line.split('"').nth(1).unwrap_or_default().to_owned();
    // Steps, in order: rests and notes, brackets and alternation stripped.
    let steps = |text: &str| -> Vec<String> {
        text.split(|c: char| c.is_whitespace() || c == '<' || c == '>')
            // A bar's `@` weight is how long it lasts, the same under either voicing: not a step.
            .map(|t| t.split('@').next().unwrap_or(t))
            .map(|t| t.trim_start_matches('[').trim_end_matches(']').to_owned())
            .filter(|t| !t.is_empty())
            .collect()
    };
    let (notes, chords) = (steps(&pattern(single)), steps(&pattern(seventh)));
    proptest::prop_assert_eq!(notes.len(), chords.len());
    for (note, chord) in notes.iter().zip(&chords) {
        if note == "~" {
            proptest::prop_assert_eq!(chord, "~");
            continue;
        }
        let tones: Vec<f64> = chord.split(',').map(|t| t.parse().unwrap()).collect();
        proptest::prop_assert_eq!(tones.len(), 4, "{} -> {}", note, chord);
        proptest::prop_assert_eq!(chord.split(',').next().unwrap(), note.as_str());
        // Each tone above the root is two scale steps higher: a third, so 3 or 4 semitones in a
        // seven-note scale, and never less than 1 or more than 5 in any scale this build knows.
        for pair in tones.windows(2) {
            let step = pair[1] - pair[0];
            proptest::prop_assert!((1.0..=5.0).contains(&step), "{} in {}", step, chord);
        }
        if style.scale().len() == 7 {
            let span = tones[3] - tones[0];
            proptest::prop_assert!((9.0..=11.0).contains(&span), "a seventh: {}", chord);
        }
    }
    // Everything after the note pattern is the same, but the level.
    let rest = |line: &str| {
        let after = line
            .split_once(".sound(")
            .map(|(_, r)| r)
            .unwrap_or_default();
        after
            .split(".postgain(")
            .next()
            .unwrap_or_default()
            .to_owned()
    };
    proptest::prop_assert_eq!(rest(single), rest(seventh));
    Ok(())
}

/// Poems of a few stanzas, built from words the test phonemiser can say.
fn poem() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::strategy::Strategy as _;
    proptest::collection::vec(
        proptest::collection::vec(
            proptest::collection::vec(
                proptest::sample::select(vec![
                    "bAn", "teA", "rot", "dos", "beA", "bo", "nAt", "roso", "GeA", "zz",
                ]),
                1..4,
            ),
            1..4,
        ),
        1..3,
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

/// Any style header the parser accepts.
fn palette() -> impl proptest::strategy::Strategy<Value = String> {
    use proptest::strategy::Strategy as _;
    (
        proptest::sample::select(
            prosody_emit::Scale::ALL
                .iter()
                .map(|scale| scale.name())
                .collect::<Vec<_>>(),
        ),
        proptest::sample::select(vec!["C", "D", "E", "F#", "Bb", "A"]),
        60_u16..200,
        0_u8..100,
        proptest::sample::select(prosody_emit::style::BANKS.to_vec()),
        proptest::sample::select(prosody_emit::style::SYNTHS.to_vec()),
        proptest::sample::select(prosody_emit::style::SOUNDFONTS.to_vec()),
        proptest::sample::select(prosody_emit::style::PRESETS.to_vec()),
        // reverb, delay, drive, compress, humanise — zero included, which leaves effects out.
        proptest::collection::vec(0_u8..=100, 5),
        (
            1_u8..=100,
            1000_u16..=20000,
            10_u8..=200,
            proptest::sample::select(prosody_emit::style::KITS.to_vec()),
            proptest::sample::select(prosody_emit::style::VOICINGS.to_vec()),
        ),
        (
            prosody_emit::style::BASS_OCTAVES,
            prosody_emit::style::MELODY_OCTAVES,
            0_u8..=100,
            proptest::sample::select(prosody_emit::style::RIDE_FALLBACKS.to_vec()),
        ),
    )
        .prop_map(
            |(
                scale,
                root,
                low,
                swing,
                bank,
                synth,
                melody,
                preset,
                amounts,
                (room, cut, level, kit, voicing),
                (bass_octave, melody_octave, density, ride_fallback),
            )| {
                let amount = |i: usize| f64::from(amounts[i]) / 100.0;
                format!(
                    "+++\n[style]\npreset = \"{preset}\"\nscale = \"{scale}\"\nroot = \"{root}\"\n\
                     tempo_range = [{low}, {}]\nswing = {}\nbank = \"{bank}\"\nsynth = \"{synth}\"\n\
                     melody = \"{melody}\"\nreverb = {}\ndelay = {}\ndrive = {}\ncompress = {}\n\
                     humanise = {}\nroom_size = {}\nhigh_cut = {cut}\nbass_gain = {}\n\
                     kit = \"{kit}\"\nvoicing = \"{voicing}\"\nbass_octave = {bass_octave}\n\
                     melody_octave = {melody_octave}\ndensity_level = {}\n\
                     ride_fallback = \"{ride_fallback}\"\n+++\n",
                    low + 6,
                    f64::from(swing) / 100.0,
                    amount(0),
                    amount(1),
                    amount(2),
                    amount(3),
                    amount(4),
                    f64::from(room) / 10.0,
                    f64::from(level) / 100.0,
                    f64::from(density) / 100.0,
                )
            },
        )
}
