//! The reading's default, `breath` (`SPEC.md` §4.7), as properties over many poems: a stanza is
//! read whole, a line a bar; two stanzas' readings are a silent bar apart at least; a stanza is
//! never read before its first line is heard; and a stanza is cut short only by the end of the
//! piece. Checked against the `arrives` plan of the same poem, which says where each line is first
//! heard, so the property does not restate the implementation.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use std::collections::BTreeMap;

use proptest::prelude::*;

use prosody_cmudict::CmuDict;
use prosody_core::lower::{self, Analyses};
use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{
    Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
};
use prosody_core::surprisal::{Lexicon, Rarity};
use prosody_core::voice::Score;
use prosody_core::{Fallback, Poem, meter, repetition, rhyme, role, sound, surprisal};
use prosody_emit::{Style, reading};
use prosody_lexicon::EnglishLexicon;
use prosody_lts::LetterToSound;

/// Spelling is pronunciation: `A` and `o` stressed vowels, `e` an unstressed one.
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
                'o' => Some((ARPABET.phoneme("OW").unwrap(), Stress::Primary)),
                'e' => Some((ARPABET.phoneme("IH").unwrap(), Stress::Unstressed)),
                'b' => Some((ARPABET.phoneme("B").unwrap(), Stress::Unstressed)),
                'd' => Some((ARPABET.phoneme("D").unwrap(), Stress::Unstressed)),
                'n' => Some((ARPABET.phoneme("N").unwrap(), Stress::Unstressed)),
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

    fn rarity(&self, _word: &str) -> Rarity {
        Rarity::band(1)
    }
}

fn lowered(poem: &Poem, phonemiser: &impl Phonemiser, lexicon: &impl Lexicon) -> Score {
    lowered_with(poem, phonemiser, lexicon, &Mappings::all())
}

fn lowered_with(
    poem: &Poem,
    phonemiser: &impl Phonemiser,
    lexicon: &impl Lexicon,
    mappings: &Mappings,
) -> Score {
    let meter = meter::scan(poem, phonemiser);
    let repetition = repetition::analyse(poem);
    let roles = role::assign(&meter, &repetition).unwrap();
    lower::run_with(
        Analyses {
            poem,
            meter: &meter,
            rhyme: &rhyme::analyse(poem, phonemiser),
            repetition: &repetition,
            sound: &sound::analyse(poem, phonemiser),
            surprisal: &surprisal::analyse(poem, phonemiser, lexicon),
            roles: &roles,
        },
        mappings,
    )
    .unwrap()
}

fn with_reading(variant: Variant) -> Mappings {
    Mappings::all().with(Mapping::Reading.choose(variant).unwrap())
}

/// Every property of the breath, for one poem under one palette.
fn check(poem: &Poem, score: &Score, style: &Style) {
    check_with(poem, score, style, &Mappings::all());
}

/// As [`check`], with the reading chosen over `mappings` (whatever they say of the reading).
fn check_with(poem: &Poem, score: &Score, style: &Style, mappings: &Mappings) {
    let reading_as = |variant: Variant| {
        mappings
            .clone()
            .with(Mapping::Reading.choose(variant).unwrap())
    };
    let breath = reading::plan(poem, score, style, &reading_as(Variant::Breath)).unwrap();
    let arrives = reading::plan(poem, score, style, &reading_as(Variant::Arrives)).unwrap();
    let heard: BTreeMap<usize, usize> = arrives
        .cues()
        .iter()
        .map(|cue| (cue.line(), cue.slot()))
        .collect();
    let slots = score.arrangement().slots().len();

    // Which stanza each line is in, and each stanza's lines.
    let mut stanzas = Vec::new();
    let mut first = 0;
    for stanza in poem.stanzas() {
        let count = stanza.lines().len();
        stanzas.push(first..first + count);
        first += count;
    }
    let mut previous_last: Option<usize> = None;
    for lines in stanzas {
        let cues: Vec<(usize, usize)> = breath
            .cues()
            .iter()
            .filter(|cue| lines.contains(&cue.line()))
            .map(|cue| (cue.slot(), cue.line()))
            .collect();
        // Where the music begins the stanza: its first line's first arrival, or the earliest of
        // any of its lines' when its first line never arrives as a host.
        let Some(arrival) = heard.get(&lines.start).copied().or_else(|| {
            lines
                .clone()
                .filter_map(|line| heard.get(&line))
                .min()
                .copied()
        }) else {
            assert!(cues.is_empty(), "{lines:?} read but never heard");
            continue;
        };
        // A silent slot after the previous stanza's last cue.
        let breath_slot = previous_last.map_or(0, |last| last + 2);
        // Never before the music has begun it, never inside the breath, and no later than both
        // ask for.
        let start = arrival.max(breath_slot);
        // A line a slot from its first line, in order, until the piece runs out: a stanza is cut
        // short — or not read at all — only by the end of the piece.
        let expected: Vec<(usize, usize)> = lines
            .clone()
            .enumerate()
            .map(|(offset, line)| (start + offset, line))
            .take_while(|(slot, _)| *slot < slots)
            .collect();
        assert_eq!(cues, expected, "{lines:?}: {:?}", breath.cues());
        if let Some((last, _)) = cues.last() {
            previous_last = Some(*last);
        }
    }
    // I4: each cue speaks its line's source.
    for cue in breath.cues() {
        assert_eq!(cue.span().text(poem.source()), Some(cue.text()));
    }
    // CLAUDE.md §6: every line not read is said, with its own source, and the cue pattern's
    // header says it too. A line of a stanza that was read, or that arrives, lost its turn to the
    // end of the piece; only a line of a stanza never heard "never arrives".
    let read: Vec<usize> = breath.cues().iter().map(reading::Cue::line).collect();
    let lines: usize = poem.stanzas().iter().map(|s| s.lines().len()).sum();
    let unread: Vec<usize> = breath.unread().iter().map(reading::Unread::line).collect();
    assert_eq!(
        unread,
        (0..lines).filter(|l| !read.contains(l)).collect::<Vec<_>>()
    );
    for line in breath.unread() {
        assert_eq!(line.span().text(poem.source()), Some(line.text()));
        assert_eq!(
            line.why() == reading::Why::NeverArrives,
            !heard.contains_key(&line.line())
                && stanza_of(poem, line.line()).all(|l| !read.contains(&l)),
            "{line:?}"
        );
    }
    let said = breath.pattern().contains("lines not read:");
    assert_eq!(said, !unread.is_empty(), "{}", breath.pattern());
    check_natural(poem, &breath, &heard);
}

/// The natural pace (`SPEC.md` §4.7) of a `breath` reading, against where `heard` says each line
/// is first heard: every stanza the music begins is read whole, in order, its first line waiting
/// for the music and the rest following; a stanza the music never begins is said, and only that.
/// Each line's pace and pause are its own: the pace its seed's, the pause its ending's.
fn check_natural(poem: &Poem, breath: &reading::Reading, heard: &BTreeMap<usize, usize>) {
    let mut expected = Vec::new();
    let mut never = Vec::new();
    let mut first = 0;
    for stanza in poem.stanzas() {
        let lines = first..first + stanza.lines().len();
        first = lines.end;
        let start = heard.get(&lines.start).copied().or_else(|| {
            lines
                .clone()
                .filter_map(|line| heard.get(&line))
                .min()
                .copied()
        });
        match start {
            Some(slot) => {
                for line in lines.clone() {
                    let onset = if line == lines.start {
                        reading::Onset::Breath { slot }
                    } else {
                        reading::Onset::Follows
                    };
                    expected.push((onset, line));
                }
            }
            None => never.extend(lines),
        }
    }
    let got: Vec<(reading::Onset, usize)> = breath
        .natural()
        .iter()
        .map(|n| (n.onset(), n.spoken().line()))
        .collect();
    assert_eq!(got, expected);
    let unread: Vec<usize> = breath
        .natural_unread()
        .iter()
        .map(reading::Unread::line)
        .collect();
    assert_eq!(unread, never);
    assert!(
        breath
            .natural_unread()
            .iter()
            .all(|line| line.why() == reading::Why::NeverArrives)
    );
    for natural in breath.natural() {
        let spoken = natural.spoken();
        // I4: the line's own bytes.
        assert_eq!(spoken.span().text(poem.source()), Some(spoken.text()));
        assert_eq!(
            Some(spoken.pace()),
            reading::Pace::of(spoken.span(), poem.source())
        );
        let closes = stanza_of(poem, spoken.line()).end == spoken.line() + 1;
        assert_eq!(spoken.pause(), reading::Pause::after(spoken.text(), closes));
        for word in spoken.words() {
            assert!(word.span().start() >= spoken.span().start());
            assert!(word.span().end() <= spoken.span().end());
            assert!(word.syllables() >= 1);
        }
    }
}

/// The lines of the stanza `line` is in.
fn stanza_of(poem: &Poem, line: usize) -> std::ops::Range<usize> {
    let mut first = 0;
    for stanza in poem.stanzas() {
        let lines = first..first + stanza.lines().len();
        if lines.contains(&line) {
            return lines;
        }
        first = lines.end;
    }
    0..0
}

/// A line of two to four feet, each from a small set of shapes.
fn line() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec!["eA", "teA", "Ae", "beAn", "o", "deo", "nAt"]),
        2..=4,
    )
    .prop_map(|feet| feet.join(" "))
}

fn poem() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::collection::vec(line(), 1..=4), 1..=4).prop_map(|stanzas| {
        stanzas
            .iter()
            .map(|lines| lines.join("\n"))
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n"
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn a_breath_reads_stanzas_whole_and_never_ahead(source in poem()) {
        let poem = Poem::parse(source).unwrap();
        let score = lowered(&poem, &Spelled, &Flat);
        let style = Style::parse("+++\ntempo_range = [120, 120]\n+++").unwrap();
        check(&poem, &score, &style);
        // I3: the palette moves no cue. I1: the same poem, the same bytes.
        let other = Style::parse("+++\ntempo_range = [90, 90]\nscale = \"lydian\"\n+++").unwrap();
        let slots = |style: &Style| -> Vec<(usize, usize)> {
            reading::plan(&poem, &score, style, &Mappings::all())
                .unwrap()
                .cues()
                .iter()
                .map(|cue| (cue.slot(), cue.line()))
                .collect()
        };
        prop_assert_eq!(slots(&style), slots(&other));
        prop_assert_eq!(
            reading::plan(&poem, &score, &style, &Mappings::all()).unwrap().pattern(),
            reading::plan(&poem, &score, &style, &Mappings::all()).unwrap().pattern()
        );
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn an_edit_to_one_stanza_moves_no_other_lines_pace_or_pause(
        source in poem(),
        replacement in prop::collection::vec(line(), 1..=4),
        which in 0_usize..4,
    ) {
        // I2 for the natural pace: replace one stanza; every line of every other stanza keeps
        // its text, and with it its pace and its pause.
        let poem = Poem::parse(source.clone()).unwrap();
        let stanzas: Vec<&str> = source.trim_end().split("\n\n").collect();
        let which = which % stanzas.len();
        let mut edited: Vec<String> = stanzas.iter().map(|s| (*s).to_owned()).collect();
        edited[which] = replacement.join("\n");
        let edited = Poem::parse(edited.join("\n\n") + "\n").unwrap();
        let style = Style::parse("+++\ntempo_range = [120, 120]\n+++").unwrap();
        let read = |poem: &Poem| {
            let score = lowered(poem, &Spelled, &Flat);
            reading::plan(poem, &score, &style, &Mappings::all()).unwrap()
        };
        let (before, after) = (read(&poem), read(&edited));
        let by_line = |reading: &reading::Reading| -> BTreeMap<(usize, usize), (String, u8, reading::Pause)> {
            reading
                .natural()
                .iter()
                .map(|n| {
                    let s = n.spoken();
                    ((s.stanza(), s.line()), (s.text().to_owned(), s.pace().hundredths(), s.pause()))
                })
                .collect()
        };
        let (before, after) = (by_line(&before), by_line(&after));
        // Lines are numbered through the poem, so compare by stanza and place within it.
        let within = |map: &BTreeMap<(usize, usize), (String, u8, reading::Pause)>| {
            let mut out: BTreeMap<(usize, usize), (String, u8, reading::Pause)> = BTreeMap::new();
            let mut first: BTreeMap<usize, usize> = BTreeMap::new();
            for ((stanza, line), value) in map {
                let start = *first.entry(*stanza).or_insert(*line);
                out.insert((*stanza, line - start), value.clone());
            }
            out
        };
        let (before_lines, after_lines) = (within(&before), within(&after));
        // A stanza the music never begins has no natural lines, and must be said to be unread —
        // never skipped silently. Every other stanza is compared line for line, all of it.
        let unread = |reading: &reading::Reading, lines: &[usize]| -> std::collections::BTreeSet<usize> {
            reading
                .natural_unread()
                .iter()
                .map(|u| lines.iter().scan(0, |end, n| { *end += n; Some(*end) }).position(|end| u.line() < end).unwrap())
                .collect()
        };
        let sizes = |source: &str| -> Vec<usize> {
            source.trim_end().split("\n\n").map(|s| s.lines().count()).collect()
        };
        let (sizes_before, sizes_after) = (sizes(&source), sizes(edited.source()));
        let (unread_before, unread_after) = (unread(&read(&poem), &sizes_before), unread(&read(&edited), &sizes_after));
        let mut compared = 0_usize;
        for (stanza, &size) in sizes_before.iter().enumerate() {
            if stanza == which {
                continue;
            }
            let heard_before = before_lines.contains_key(&(stanza, 0));
            let heard_after = after_lines.contains_key(&(stanza, 0));
            prop_assert_eq!(heard_before, !unread_before.contains(&stanza), "stanza {} before", stanza);
            prop_assert_eq!(heard_after, !unread_after.contains(&stanza), "stanza {} after", stanza);
            if !(heard_before && heard_after) {
                continue;
            }
            for place in 0..size {
                let (one, two) = (before_lines.get(&(stanza, place)), after_lines.get(&(stanza, place)));
                prop_assert!(one.is_some() && two.is_some(), "stanza {} line {} missing", stanza, place);
                prop_assert_eq!(one, two);
                compared += 1;
            }
        }
        // Each case compares every line of every stanza heard on both sides, and says so.
        let both = (0..sizes_before.len())
            .filter(|&s| s != which && !unread_before.contains(&s) && !unread_after.contains(&s))
            .map(|s| sizes_before[s])
            .sum::<usize>();
        prop_assert_eq!(compared, both);
    }
}

#[test]
fn every_example_poem_breathes() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let lexicon = EnglishLexicon::new();
    let mut checked = 0;
    for entry in std::fs::read_dir(examples).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "poem") {
            continue;
        }
        let poem = Poem::parse(std::fs::read_to_string(&path).unwrap()).unwrap();
        let style =
            Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
        let score = lowered(&poem, &phonemiser, &lexicon);
        check(&poem, &score, &style);
        // Every example has room for its whole reading: every line read once.
        let lines: usize = poem.stanzas().iter().map(|s| s.lines().len()).sum();
        let read = reading::plan(&poem, &score, &style, &Mappings::all())
            .unwrap()
            .cues()
            .len();
        assert_eq!(read, lines, "{}", path.display());
        checked += 1;
    }
    assert_eq!(checked, 18);
}

#[test]
fn without_the_outro_a_reading_that_runs_out_says_which_lines_it_lost() {
    // Hopkins, Pied Beauty, with the outro off (the page's setting B, with the reading set to
    // breath). Hand-worked from SPEC.md §4.6: two stanzas, six lines and five, each stated once —
    // stanza 1 in bars 1–6, stanza 2 in bars 7–11 — and nothing after, so 11 bars. `arrives` reads
    // every line in its own bar. `breath` reads stanza 1 in bars 1–6; stanza 2 arrives in bar 7
    // but waits for its breath, max(7, 6 + 2) = bar 8, so bars 8–12 — and bar 12 is past the
    // piece. Its last line, "Praise him.", is not read, and the reading says so.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/hopkins-pied-beauty.poem"),
    )
    .unwrap();
    let poem = Poem::parse(source).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let no_outro = Mappings::all().without(Mapping::Outro);
    let score = lowered_with(&poem, &phonemiser, &EnglishLexicon::new(), &no_outro);
    assert_eq!(score.arrangement().slots().len(), 11);
    check_with(&poem, &score, &style, &no_outro);
    let plan = |variant: Variant| {
        reading::plan(
            &poem,
            &score,
            &style,
            &no_outro
                .clone()
                .with(Mapping::Reading.choose(variant).unwrap()),
        )
        .unwrap()
    };
    let bars = |variant: Variant| -> Vec<usize> {
        plan(variant)
            .cues()
            .iter()
            .map(|cue| cue.slot() + 1)
            .collect()
    };
    assert_eq!(bars(Variant::Arrives), (1..=11).collect::<Vec<_>>());
    assert!(plan(Variant::Arrives).unread().is_empty());
    assert!(plan(Variant::Stanza).unread().is_empty());
    assert_eq!(bars(Variant::Breath), vec![1, 2, 3, 4, 5, 6, 8, 9, 10, 11]);

    let breath = plan(Variant::Breath);
    let [lost] = breath.unread() else {
        panic!("{:?}", breath.unread());
    };
    assert_eq!(lost.line(), 10);
    assert_eq!(lost.text().trim_start(), "Praise him.");
    assert_eq!(lost.why(), reading::Why::PieceEnds);
    let pattern = breath.pattern();
    assert!(
        pattern.contains("// 1 of 11 lines not read:\n"),
        "{pattern}"
    );
    assert!(
        pattern.contains(&format!(
            "//   line 11 (bytes {}..{}) is not read, the piece ends before its turn: {}\n",
            lost.span().start(),
            lost.span().end(),
            lost.text()
        )),
        "{pattern}"
    );
    // Off reads nothing by choice, and loses nothing.
    assert!(plan(Variant::Off).unread().is_empty());
}

#[test]
fn every_example_without_its_outro_says_what_it_does_not_read() {
    // The breath's properties hold with the outro off too, and any line lost is listed.
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let lexicon = EnglishLexicon::new();
    let no_outro = Mappings::all().without(Mapping::Outro);
    for entry in std::fs::read_dir(examples).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "poem") {
            continue;
        }
        let poem = Poem::parse(std::fs::read_to_string(&path).unwrap()).unwrap();
        let style =
            Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
        let score = lowered_with(&poem, &phonemiser, &lexicon, &no_outro);
        check_with(&poem, &score, &style, &no_outro);
    }
}

#[test]
fn the_tyger_reads_as_planned() {
    // Hand-worked from the `arrives` plan (SPEC.md §4.7, the PLAN.md log): The Tyger's six
    // stanzas of four lines; stanza 1 stated in bars 1–4; each later stanza transforms the loop,
    // its first line arriving in bars 5, 21, 37, 53, 69 (every 16 bars: four passes of four).
    // Stanza 2 waits for its breath: max(5, 4 + 2) = bar 6, so bars 6–9. Every later stanza's
    // arrival is long after the previous one's breath, so it starts where it arrives.
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/blake-tyger.poem"),
    )
    .unwrap();
    let poem = Poem::parse(source).unwrap();
    let style =
        Style::parse_optional(poem.frontmatter().and_then(|s| s.text(poem.source()))).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let score = lowered(&poem, &phonemiser, &EnglishLexicon::new());
    let bars = |variant: Variant| -> Vec<usize> {
        reading::plan(&poem, &score, &style, &with_reading(variant))
            .unwrap()
            .cues()
            .iter()
            .map(|cue| cue.slot() + 1)
            .collect()
    };
    assert_eq!(
        bars(Variant::Arrives),
        vec![
            1, 2, 3, 4, 5, 10, 15, 20, 21, 26, 31, 36, 37, 42, 47, 52, 53, 58, 63, 68, 69, 74, 79,
            84
        ]
    );
    assert_eq!(
        bars(Variant::Breath),
        vec![
            1, 2, 3, 4, 6, 7, 8, 9, 21, 22, 23, 24, 37, 38, 39, 40, 53, 54, 55, 56, 69, 70, 71, 72
        ]
    );
}
