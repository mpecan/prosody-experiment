//! The four invariants, on the Performance (`docs/player.md` §6).
//!
//! The Performance is what a player of our own will play, so it carries the same promises as the
//! pattern text: the same poem and palette give the same notes (I1); an edit to one stanza leaves
//! the others' notes alone (I2); a palette changes how a note sounds, never which notes there are
//! or where they fall (I3); and every note, every light and every cue points at the words that made
//! it (I4).
//!
//! That the Performance plays what the pattern text says is a separate test, against Strudel itself:
//! `web/test/parity.test.ts`.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::collections::BTreeSet;

use proptest::prelude::*;
use prosody_core::lower::{self, Analyses};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Mapping, Mappings, Poem, Span, Variant};
use prosody_core::{meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::perform::{Cycles, Lane, Note};
use prosody_emit::{Backend as _, Perform, Performance, Style, reading};

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
    fn is_function_word(&self, _: &str) -> bool {
        false
    }

    fn rarity(&self, _: &str) -> Rarity {
        Rarity::band(3)
    }
}

/// The poem, its score under `mappings`, and the palette its header sets.
fn lowered(source: &str, mappings: &Mappings) -> (Poem, Score, Style) {
    let poem = Poem::parse(source.to_owned()).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let meter = meter::scan(&poem, &Spelled);
    let rhyme = rhyme::analyse_with(&poem, &Spelled, mappings.get(Mapping::Rhyme));
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
    (poem, score, style)
}

/// The Performance of `source` under `mappings`, with its reading, or `None` for a poem with
/// nothing to play.
fn performed(source: &str, mappings: &Mappings) -> Option<(Poem, Performance)> {
    let (poem, score, style) = lowered(source, mappings);
    let performance = Perform::new().render(&score, &style).ok()?;
    let reading = reading::plan(&poem, &score, &style, mappings).unwrap();
    Some((poem, performance.read(&reading)))
}

/// Poems of a few stanzas, built from words the test phonemiser can say — a refrain likely, a
/// ragged stanza likely, rhymes likely.
fn poem() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::collection::vec(
            prop::collection::vec(
                prop::sample::select(vec![
                    "bAn", "teA", "rot", "dos", "beA", "bo", "nAt", "roso", "GeA", "zz",
                ]),
                1..4,
            ),
            1..4,
        ),
        1..4,
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

/// A palette header: every key a note's sound reads, varied, swing included.
fn palette() -> impl Strategy<Value = String> {
    (
        prop::sample::select(
            prosody_emit::Scale::ALL
                .iter()
                .map(|scale| scale.name())
                .collect::<Vec<_>>(),
        ),
        prop::sample::select(vec!["C", "D", "F#", "Bb"]),
        60_u16..200,
        0_u8..100,
        prop::sample::select(prosody_emit::style::BANKS.to_vec()),
        prop::sample::select(prosody_emit::style::SYNTHS.to_vec()),
        prop::sample::select(prosody_emit::style::SOUNDFONTS.to_vec()),
        prop::collection::vec(0_u8..=100, 6),
        (
            1000_u16..=20000,
            prop::sample::select(prosody_emit::style::KITS.to_vec()),
            prop::sample::select(prosody_emit::style::VOICINGS.to_vec()),
            prosody_emit::style::BASS_OCTAVES,
            prosody_emit::style::MELODY_OCTAVES,
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
                amounts,
                (cut, kit, voicing, bass_octave, melody_octave),
            )| {
                let amount = |i: usize| f64::from(amounts[i]) / 100.0;
                format!(
                    "+++\n[style]\nscale = \"{scale}\"\nroot = \"{root}\"\n\
                     tempo_range = [{low}, {}]\nswing = {}\nbank = \"{bank}\"\nsynth = \"{synth}\"\n\
                     melody = \"{melody}\"\nreverb = {}\ndelay = {}\ndrive = {}\ncompress = {}\n\
                     humanise = {}\ndensity_level = {}\nhigh_cut = {cut}\nkit = \"{kit}\"\n\
                     voicing = \"{voicing}\"\nbass_octave = {bass_octave}\n\
                     melody_octave = {melody_octave}\n+++\n",
                    low + 6,
                    f64::from(swing) / 100.0,
                    amount(0),
                    amount(1),
                    amount(2),
                    amount(3),
                    amount(4),
                    amount(5),
                )
            },
        )
}

/// Any choice of the candidate mappings that shape where notes fall.
fn mappings() -> impl Strategy<Value = Mappings> {
    (
        prop::sample::select(vec![
            Variant::Off,
            Variant::Rhyme,
            Variant::Cadence,
            Variant::Both,
        ]),
        prop::sample::select(vec![Variant::Height, Variant::Syllables, Variant::Feet]),
        prop::sample::select(vec![Variant::Steady, Variant::Elastic]),
        prop::sample::select(vec![Variant::Crossing, Variant::Straight]),
        prop::sample::select(vec![Variant::Off, Variant::On]),
    )
        .prop_map(|(harmony, bass, pulse, stack, anchor)| {
            let mut chosen = Mappings::all();
            for (mapping, variant) in [
                (Mapping::Harmony, harmony),
                (Mapping::Bass, bass),
                (Mapping::Pulse, pulse),
                (Mapping::Stack, stack),
                (Mapping::Anchor, anchor),
            ] {
                if let Some(choice) = mapping.choose(variant) {
                    chosen = chosen.with(choice);
                }
            }
            chosen
        })
}

/// A note's place in the piece, which no palette may move: its word (relative to the poem's
/// body, since a header shifts every byte), its lane, its bar and its place in the pattern.
fn place(note: &Note, body_start: usize) -> (usize, usize, Lane, usize, (u128, u128)) {
    (
        note.span().start() - body_start,
        note.span().end() - body_start,
        note.lane(),
        note.slot(),
        note.grid().parts(),
    )
}

/// The latest a nudge strikes, in seconds: thirty milliseconds at `humanise = 1`
/// (`perform::MAX_LATE`), printed at three decimals.
const MAX_LATE: f64 = 0.030;

/// When each bar begins, in seconds, in slot order.
fn bar_begins(performance: &Performance) -> Vec<f64> {
    performance
        .bars()
        .iter()
        .map(|bar| bar.at().get())
        .collect()
}

/// Where each bar ends, in cycles and in seconds: where the next begins, or the piece's end.
fn bar_ends(performance: &Performance) -> Vec<(Cycles, f64)> {
    let bars = performance.bars();
    (0..bars.len())
        .map(|slot| {
            bars.get(slot + 1).map_or(
                (performance.cycles(), performance.seconds().get()),
                |next| (next.cycle(), next.at().get()),
            )
        })
        .collect()
}

/// How far past its bar's end a note may sound: swing's delay (`x / 2n` of a cycle, in seconds at
/// the piece's tempo), the nudge, and a nanosecond for the floats.
fn late(performance: &Performance) -> f64 {
    let swing = performance.swing().map_or(0.0, |swing| {
        swing.delay().as_f64() * 60.0 / performance.cycles_per_minute()
    });
    swing + MAX_LATE + 1e-9
}

/// Every token span of the poem: the words a note may point at.
fn words(poem: &Poem) -> BTreeSet<Span> {
    poem.stanzas()
        .iter()
        .flat_map(prosody_core::Stanza::lines)
        .flat_map(prosody_core::Line::tokens)
        .map(|token| token.span())
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// I1: the same poem and palette give the same Performance, note for note.
    #[test]
    fn i1_performing_is_reproducible(body in poem(), header in palette(), chosen in mappings()) {
        let source = format!("{header}{body}");
        let once = performed(&source, &chosen).map(|(_, p)| p);
        let again = performed(&source, &chosen).map(|(_, p)| p);
        prop_assert_eq!(once, again);
    }

    /// I4: every note stands on a word of the poem, every light on a word, every cue on its line;
    /// and every note sounds within the piece, in the bar it says: its place in the pattern within
    /// that bar's cycles, and its onset no earlier than the bar begins and no later than the next
    /// one does, but for swing's delay and the nudge.
    #[test]
    fn i4_every_note_resolves_to_a_word(body in poem(), header in palette(), chosen in mappings()) {
        let source = format!("{header}{body}");
        let Some((poem, performance)) = performed(&source, &chosen) else { return Ok(()) };
        let words = words(&poem);
        let (begins, ends) = (bar_begins(&performance), bar_ends(&performance));
        for note in performance.notes() {
            prop_assert!(words.contains(&note.span()), "{:?} is not a word", note.span());
            prop_assert!(note.span().text(poem.source()).is_some_and(|text| !text.is_empty()));
            let bar = &performance.bars()[note.slot()];
            prop_assert!(bar.cycle() <= note.grid(), "{:?} before {:?}", note.grid(), bar);
            prop_assert!(note.grid() < ends[note.slot()].0, "{:?} past slot {}", note.grid(), note.slot());
            prop_assert!(note.onset().get() >= begins[note.slot()]);
            prop_assert!(
                note.onset().get() < ends[note.slot()].1 + late(&performance),
                "{} s is past slot {}, which ends at {} s",
                note.onset().get(), note.slot(), ends[note.slot()].1
            );
            prop_assert!(note.length().get() > 0.0);
        }
        for mark in performance.marks() {
            prop_assert!(words.contains(&mark.span()));
            // A light has no nudge: only swing may carry it past its bar's end.
            prop_assert!(mark.onset().get() >= begins[mark.slot()]);
            prop_assert!(mark.onset().get() < ends[mark.slot()].1 + late(&performance) - MAX_LATE);
        }
        for cue in performance.cues() {
            prop_assert!(cue.span().text(poem.source()).is_some_and(|text| !text.trim().is_empty()));
        }
        // Sorted by exact onset (I1's "sort before you emit").
        for pair in performance.notes().windows(2) {
            prop_assert!(pair[0].exact_onset() <= pair[1].exact_onset());
        }
    }

    /// I3: a palette changes how notes sound, never which words sound where. The same notes stand
    /// on the same words in the same bars at the same places in the pattern under any two palettes
    /// — swing and humanise included, which move a note in seconds but not in the pattern.
    ///
    /// Compared as sets: a voicing may stack one event into several tones, all at the one place.
    #[test]
    fn i3_no_palette_moves_a_note(
        body in poem(),
        one in palette(),
        two in palette(),
        chosen in mappings(),
    ) {
        let first = performed(&format!("{one}{body}"), &chosen);
        let second = performed(&format!("{two}{body}"), &chosen);
        let (Some((_, a)), Some((_, b))) = (first, second) else {
            // Nothing to play under one palette is nothing under the other.
            return Ok(());
        };
        let places = |performance: &Performance, header: &str| -> BTreeSet<_> {
            performance.notes().iter().map(|note| place(note, header.len())).collect()
        };
        prop_assert_eq!(places(&a, &one), places(&b, &two));
        prop_assert_eq!(a.bars().len(), b.bars().len());
        prop_assert_eq!(a.marks().len(), b.marks().len());
    }

    /// I2: editing a word of the first stanza leaves the last stanza's notes sounding as they did —
    /// the same words, in the same lanes, on the same sounds, at the same levels, through the same
    /// voice and sends. Compared relative to the stanza, since an edit shifts every later byte.
    ///
    /// Scoped as `prosody_core`'s own I2 properties are (its bass and harmony ones). When notes
    /// fall is left out, as there the bar is: the edited stanza may now last longer, and the tempo
    /// follows the piece's density. The rhyme melody and the chord lane are left out, as pitch is
    /// there: rhyme is between words across stanzas — in "bAn / bo" the second stanza's word rhymes
    /// with nothing until the first's becomes "roso", and then it carries a note. And the edit is
    /// assumed to leave the last stanza's layer and its rhyme words alone, as there: whether a
    /// stanza is foundation can be poem-relative (§4.1), and the height bass leaves rhyme words to
    /// the melody.
    #[test]
    fn i2_an_edit_leaves_other_stanzas_notes_alone(
        body in poem(),
        pick in 0_usize..64,
        word in prop::sample::select(vec!["bAn", "teA", "roso", "zz"]),
    ) {
        let (poem, _, _) = lowered(&body, &Mappings::all());
        prop_assume!(poem.stanzas().len() > 1);
        let first: Vec<Span> = poem.stanzas()[0]
            .lines()
            .iter()
            .flat_map(prosody_core::Line::tokens)
            .map(|token| token.span())
            .collect();
        let target = first[pick % first.len()];
        let mut edited = body.clone();
        edited.replace_range(target.start()..target.end(), word);
        let (edited_poem, _, _) = lowered(&edited, &Mappings::all());
        prop_assume!(edited_poem.stanzas().len() == poem.stanzas().len());

        let last = |poem: &Poem| poem.stanzas()[poem.stanzas().len() - 1].span();
        let layer = |source: &str| {
            let poem = Poem::parse(source.to_owned()).unwrap();
            let roles = role::assign(&meter::scan(&poem, &Spelled), &repetition::analyse(&poem))
                .unwrap();
            roles.stanzas()[roles.stanzas().len() - 1].layer()
        };
        prop_assume!(layer(&body) == layer(&edited));
        // A refrain is one voice wherever it returns (§4.3's refrain row: the returning line *is*
        // the earlier one), so a last-stanza line that repeats a first-stanza line, before the
        // edit or after it, is that line's voice, and is not another stanza's to keep.
        let texts = |poem: &Poem, stanza: usize| -> BTreeSet<String> {
            poem.stanzas()[stanza]
                .lines()
                .iter()
                .filter_map(|line| line.span().text(poem.source()).map(str::to_owned))
                .collect()
        };
        let closing = texts(&poem, poem.stanzas().len() - 1);
        prop_assume!(texts(&poem, 0).is_disjoint(&closing));
        prop_assume!(texts(&edited_poem, 0).is_disjoint(&closing));
        let notes = |source: &str, within: Span, rhyme: bool| -> BTreeSet<String> {
            let Some((_, performance)) = performed(source, &Mappings::all()) else {
                return BTreeSet::new();
            };
            performance
                .notes()
                .iter()
                .filter(|note| within.contains(note.span()))
                .filter(|note| matches!(note.lane(), Lane::Melody | Lane::Chords) == rhyme)
                .map(|note| {
                    let sends = note.sends();
                    let offset = note.span().start() - within.start();
                    if rhyme {
                        return format!("{offset}");
                    }
                    format!(
                        "{offset} {} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {:?}",
                        note.lane().name(),
                        note.channel(),
                        note.sound(),
                        note.gain(),
                        note.voice(),
                        sends.room(),
                        sends.room_size(),
                        note.postgain(),
                        note.compressor(),
                    )
                })
                .collect()
        };
        prop_assume!(notes(&body, last(&poem), true) == notes(&edited, last(&edited_poem), true));
        prop_assert_eq!(
            notes(&body, last(&poem), false),
            notes(&edited, last(&edited_poem), false)
        );
    }
}

#[test]
fn a_poem_with_nothing_to_play_is_refused() {
    let (_, score, style) = lowered("zz\n", &Mappings::all());
    assert_eq!(
        Perform::new().render(&score, &style),
        Err(prosody_emit::PerformError::Silent)
    );
}

#[test]
fn every_note_of_the_couplet_is_one_of_its_syllables_or_chords() {
    // A hand-sized case of what the properties assert in general: a two-line poem, its notes on
    // its four words, every lane it should have and none it should not.
    let source = "teA bAn\nreA bAn\n";
    let (poem, performance) = performed(source, &Mappings::all()).unwrap();
    let lanes: BTreeSet<Lane> = performance.notes().iter().map(Note::lane).collect();
    assert!(lanes.contains(&Lane::Foundation), "{lanes:?}");
    assert!(
        lanes.contains(&Lane::Melody),
        "two rhyme words, two notes: {lanes:?}"
    );
    assert!(
        !lanes.contains(&Lane::Chords),
        "the harmony is off by default: {lanes:?}"
    );
    let texts: BTreeSet<&str> = performance
        .notes()
        .iter()
        .map(|note| note.span().text(poem.source()).unwrap())
        .collect();
    assert_eq!(texts, BTreeSet::from(["bAn", "reA", "teA"]));
}
