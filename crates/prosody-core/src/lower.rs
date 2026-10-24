//! Turning the analyses into a [`Score`]: `SPEC.md` §7.5.
//!
//! §10 leaves one question open — "whether the CST → Voice IR lowering is one pass or several" —
//! and answers it in the same breath: "Several is easier to test and to attribute (I4), and cheap
//! enough at poem scale." So this is four steps:
//!
//! 1. `rhythm` — every syllable placed in time, from the meter. This builds the whole event list;
//!    the steps after it only decorate what it produced. Under the light function words
//!    ([`crate::accent`]) a one-syllable function word the meter calls weak is then struck
//!    lightly, before anything reads a velocity.
//! 2. `melody` — pitch, on the rhyming words.
//! 3. the bass ([`crate::bass`]) — a bass note on foundation syllables or feet, after the harmony
//!    layer ([`crate::harmony`]) has decided each line's chord, which W2 walks on.
//! 4. `colour` — sharpness and family modulation, per word.
//! 5. `figures` — the structural figures of §4.3 that a voice cannot express.
//!
//! Splitting them this way is what makes a wrong note attributable to one mapping rather than to
//! "the lowering": a misplaced beat is `rhythm`'s, a wrong note `melody`'s.
//!
//! They are **not** the `fn run(&PrevStage) -> Result<NextStage, _>` shape `CLAUDE.md` §2 asks
//! for, and the difference is real rather than cosmetic: the decorating steps take `&mut [Event]`,
//! so there is no per-stage type and nothing stops them running in the wrong order. That is a
//! deliberate simplification — every decoration is keyed by span and independent of the others,
//! so no ordering exists to get wrong — but if a fifth step ever needs to read what a fourth
//! wrote, this needs the typed staging instead.
//!
//! # Every input must describe the same poem
//!
//! Six analyses arrive here, and spans cannot tell poems apart — "war is kind maiden" and "one two
//! three four" occupy exactly the same bytes. Every one of them therefore carries a
//! [`Poem::fingerprint`](crate::parse::Poem::fingerprint), and [`run`] refuses a mismatched set
//! rather than producing confident nonsense. That check exists because the role pass shipped
//! without it and invented refrains between lines that had never met.

use std::collections::{BTreeMap, BTreeSet};

use crate::mapping::{Mapping, Mappings};
use crate::meter::PoemMeter;
use crate::parse::Poem;
use crate::phoneme::Phoneme;
use crate::repetition::{FigureKind, PoemRepetition, VoiceId};
use crate::rhyme::PoemRhyme;
use crate::role::PoemRoles;
use crate::seed::{Domain, Seed};
use crate::sound::SoundRepetition;
use crate::span::Span;
use crate::surprisal::{PoemSurprisal, Surprisal};
use crate::voice::{
    Anchor, Bar, BeatIndex, Detune, Event, Family, Pitch, Score, Subdivision, Velocity, Voice,
};

/// Everything the lowering reads, gathered so the fingerprint check has one place to happen.
#[derive(Debug, Clone, Copy)]
pub struct Analyses<'a> {
    /// The parsed poem.
    pub poem: &'a Poem,
    /// Its meter (`SPEC.md` §4.2).
    pub meter: &'a PoemMeter,
    /// Its rhyme classes (§4.5).
    pub rhyme: &'a PoemRhyme,
    /// Its line repetitions (§4.3).
    pub repetition: &'a PoemRepetition,
    /// Its word families (§4.3).
    pub sound: &'a SoundRepetition,
    /// Its distinctiveness scores (§4.4).
    pub surprisal: &'a PoemSurprisal,
    /// Its layer assignments (§4.1).
    pub roles: &'a PoemRoles,
}

/// Why a poem could not be lowered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the analyses describe different poems")]
pub struct DifferentPoems;

impl Analyses<'_> {
    /// Whether every analysis describes the poem.
    ///
    /// All six, including the roles. An earlier version checked five and omitted `roles`, which
    /// let §4.1's layer assignment — the spec's "load-bearing decision" — come from a different
    /// poem: the arrangement inverted, and `run` returned `Ok`.
    fn agree(&self) -> bool {
        let poem = self.poem.fingerprint();
        [
            self.meter.poem(),
            self.rhyme.poem(),
            self.repetition.poem(),
            self.sound.poem(),
            self.surprisal.poem(),
            self.roles.poem(),
        ]
        .iter()
        .all(|&other| other == poem)
    }
}

/// Lowers a poem to a [`Score`] (`SPEC.md` §7.5), with every mapping.
///
/// # Errors
///
/// [`DifferentPoems`] if the analyses did not all come from the same poem.
pub fn run(analyses: Analyses<'_>) -> Result<Score, DifferentPoems> {
    run_with(analyses, &Mappings::all())
}

/// Lowers a poem to a [`Score`], applying only the candidate mappings in `mappings`
/// ([`crate::mapping`]).
///
/// # Errors
///
/// [`DifferentPoems`] if the analyses did not all come from the same poem.
pub fn run_with(analyses: Analyses<'_>, mappings: &Mappings) -> Result<Score, DifferentPoems> {
    if !analyses.agree() {
        return Err(DifferentPoems);
    }

    let by_line = lines_by_voice(analyses);
    let (mut events, bars, nuclei) = rhythm(analyses, &by_line);
    // Straight after the rhythm, before anything reads a velocity: the light function words
    // (§4.2, `crate::accent`) decide which syllables are accents, and the bass plays accents.
    let function = mappings.get(Mapping::Function);
    crate::accent::run(&mut events, analyses.surprisal, function);
    let degrees = degrees(analyses.rhyme);
    melody(&mut events, analyses, &degrees);
    // The harmony layer reads the rhyme and the roles and nothing the arrangement decides: a chord
    // is a fact about its line, and `Score::harmony_in` finds it wherever the line sounds. It comes
    // before the bass because W2's walk lands on the bar's chord tones.
    let harmony = crate::harmony::run(analyses.rhyme, analyses.roles, &degrees, mappings);
    // After the melody: the height bass leaves a rhyme word to §4.5's note, as it always has.
    // W2 turns round where the chord lane does, at the half of the bar the line is stated in: so
    // it learns that bar's length here, by the arrangement's own rule (`crate::arrange`).
    let walk = mappings.get(Mapping::Bass);
    let own_bars = own_bars(analyses, &events, mappings.has(Mapping::StanzaPeriod));
    crate::bass::run(&mut events, &nuclei, analyses, &harmony, &own_bars, walk);
    colour(&mut events, analyses);
    figures(&mut events, analyses);
    let voices = voices(analyses, &by_line, &events);

    // One explicit total order before anything downstream sees them (`CLAUDE.md` §4). The rhythm
    // pass already emits in this order; sorting anyway means a later pass may reorder freely
    // without the backend's output depending on the order it happened to choose.
    events.sort_by_key(Event::order);
    let score = Score::new(voices, events, bars, silent(analyses))
        .with_harmony(harmony)
        .with_bass(walk)
        // Recorded, never read here: how long a bar lasts is the backends' to lay out (§4.2).
        .with_pulse(mappings.get(Mapping::Pulse))
        // Recorded, never read here: a stacked line is fitted to its host slot by slot, where the
        // arrangement has put the two together (`Score::placed_in`, §4.2 and §4.6).
        .with_stack(mappings.get(Mapping::Stack))
        // Already applied to the events above; recorded so the backends can name it.
        .with_function(function);
    // Last, because it reads the finished material: a slot's length is its host line's beats.
    let arrangement = crate::arrange::run(
        analyses.poem,
        analyses.roles,
        analyses.rhyme,
        &score,
        mappings,
    );
    Ok(score.with_arrangement(arrangement))
}

/// Which instruments the poem needs.
///
/// One per [`VoiceId`], which is the repetition pass's grouping: lines with identical text share a
/// voice, so a refrain is literally one instrument played again (`SPEC.md` §4.3).
///
/// The layer comes from the *first* line that sounds the voice. Every line of one voice has the
/// same text, so a repeated line is a refrain and the role pass gives all its occurrences
/// `Foundation`; a voice whose lines disagreed would be a bug in the role pass, and taking the
/// first is a stable answer rather than an arbitrary one.
fn voices(
    analyses: Analyses<'_>,
    by_line: &BTreeMap<Span, VoiceId>,
    events: &[Event],
) -> Vec<Voice> {
    let source = analyses.poem.source();
    let sounding: BTreeSet<VoiceId> = events.iter().map(|event| event.voice()).collect();
    let mut voices: BTreeMap<VoiceId, Voice> = BTreeMap::new();

    for (_, line) in analyses.roles.lines() {
        let (Some(&id), Some(seed)) = (
            by_line.get(&line.span()),
            // The *line's* span, never the stanza's and never the poem's. This is the first real
            // caller of `Seed::for_unit`, and the type stops a caller passing a bare `&str` while
            // leaving a poem-wide *span* perfectly typeable — so I2 rests here, on this argument.
            Seed::for_unit(Domain::Line, line.span(), source),
        ) else {
            continue;
        };
        // Only voices that sound. A line whose words are all unpronounceable produces no events,
        // and an instrument that never plays is not an instrument — it is a claim in the IR that
        // nothing backs. Those lines are reported by `Score::silent` instead.
        if !sounding.contains(&id) {
            continue;
        }
        voices
            .entry(id)
            .or_insert_with(|| Voice::new(id, line.layer(), line.span(), seed));
    }

    voices.into_values().collect()
}

/// The words that produced no sound, in source order.
fn silent(analyses: Analyses<'_>) -> Vec<Span> {
    let mut spans: Vec<Span> = analyses
        .meter
        .stanzas()
        .iter()
        .flat_map(crate::meter::StanzaMeter::lines)
        .flat_map(crate::meter::LineMeter::unpronounced)
        .copied()
        .collect();
    spans.sort_unstable();
    spans
}

/// Which voice each line belongs to, by span.
///
/// Built once. Every later pass needs it per line, and rescanning the repetition analysis each
/// time would make the lowering quadratic in a poem's line count for no reason.
fn lines_by_voice(analyses: Analyses<'_>) -> BTreeMap<Span, VoiceId> {
    analyses
        .repetition
        .stanzas()
        .iter()
        .flat_map(crate::repetition::StanzaRepetition::lines)
        .map(|line| (line.line(), line.voice()))
        .collect()
}

/// How many beats each line's bar lasts where the line is stated, its own host: its stanza's
/// period under [`Mapping::StanzaPeriod`], else the line's own beats — what
/// [`crate::arrange::Slot::beats`] gives that slot, computed by the same rule
/// ([`crate::arrange::periods_by`]) before there is a score to arrange.
fn own_bars(analyses: Analyses<'_>, events: &[Event], period: bool) -> BTreeMap<usize, usize> {
    let beats_in = |bar: usize| {
        events
            .iter()
            .filter(|event| event.bar().get() == bar)
            .map(|event| event.beat().get() + 1)
            .max()
            .unwrap_or(0)
    };
    if period {
        crate::arrange::periods_by(analyses.poem, beats_in)
    } else {
        let bars = analyses
            .poem
            .stanzas()
            .iter()
            .map(|s| s.lines().len())
            .sum();
        (0..bars).map(|bar| (bar, beats_in(bar))).collect()
    }
}

/// Places every syllable in time (`SPEC.md` §4.2).
///
/// > foot → beat, stress → accent / velocity, syllable → subdivision within the foot, line → bar.
///
/// Bars are counted across the poem in reading order, so bar *n* is the *n*th line; a stanza break
/// is a unit boundary rather than a rest, and inserting one does not shift the music.
///
/// Syllables the phonemiser could not place — a word it cannot pronounce — produce no event.
/// [`LineMeter::unpronounced`](crate::meter::LineMeter::unpronounced) records them, so the silence
/// is visible rather than a line that merely reads short.
///
/// Also returns each event's vowel, index for index, for the bass ([`crate::bass`]).
fn rhythm(
    analyses: Analyses<'_>,
    by_line: &BTreeMap<Span, VoiceId>,
) -> (Vec<Event>, usize, Vec<Phoneme>) {
    let mut events = Vec::new();
    let mut nuclei = Vec::new();
    let mut bar = 0;

    for stanza in analyses.meter.stanzas() {
        for line in stanza.lines() {
            // Counted before the skip below, so a line that produces nothing still occupies its
            // bar. Incrementing afterwards would re-time every later bar in the poem.
            let this = bar;
            bar += 1;
            let Some(&voice) = by_line.get(&line.span()) else {
                continue;
            };
            for (index, foot) in line.feet().iter().enumerate() {
                let Some(beats) = line.beats_of(foot) else {
                    continue;
                };
                let of = beats.len();
                for (within, beat) in beats.iter().enumerate() {
                    events.push(Event::new(
                        beat.word,
                        voice,
                        Bar::new(this),
                        BeatIndex::new(index),
                        Subdivision::new(within, of),
                        foot.kind(),
                        Velocity::of(beat.stress),
                        beat.provenance,
                    ));
                    nuclei.push(beat.nucleus);
                }
            }
        }
    }

    (events, bar, nuclei)
}

/// Gives the rhyming words their notes (`SPEC.md` §4.5).
///
/// > Treat rhyme as an equivalence relation. Rhyme class → scale degree, assigned in order of
/// > first appearance from the style's scale. […] perfect rhyme → exact pitch; slant rhyme →
/// > detune or neighbour tone.
///
/// Only the word the rhyme rests on is pitched. A line whose rhyme is perfect sits exactly on its
/// degree; one whose rhyme is approximate sits proportionally off it, which is the continuous
/// scoring §4.5 asks for rather than a boolean.
///
/// A line in a class of one has no pitch at all: it rhymes with nothing, so there is no melodic
/// relation for a degree to express, and giving it one would invent a melody the poem never wrote.
fn melody(
    events: &mut [Event],
    analyses: Analyses<'_>,
    degrees: &BTreeMap<crate::rhyme::RhymeClassId, usize>,
) {
    let sounding = sounding(analyses.rhyme);
    let pitches: BTreeMap<Span, Pitch> = sounding
        .iter()
        .filter_map(|(class, word, score)| {
            let degree = *degrees.get(class)?;
            Some((
                *word,
                Pitch::new(*class, degree, Detune::from_score(score.parts())),
            ))
        })
        .collect();

    for event in events {
        if let Some(&pitch) = pitches.get(&event.span()) {
            *event = event.with_pitch(pitch);
        }
    }
}

/// Every line that rhymes with something: its class, its rhyme word and how well it rhymes.
fn sounding(
    rhyme: &PoemRhyme,
) -> Vec<(crate::rhyme::RhymeClassId, Span, crate::rhyme::RhymeScore)> {
    rhyme
        .stanzas()
        .iter()
        .flat_map(crate::rhyme::StanzaRhyme::lines)
        .filter_map(|line| Some((line.class()?, line.word()?, line.score()?)))
        .collect()
}

/// Each sounding rhyme class's scale degree (`SPEC.md` §4.5), shared by the melody and the
/// harmony so a chord on a class's degree is the class's note.
///
/// Degrees are numbered over the classes that actually sound, in order of first appearance.
/// `RhymeClassId` counts *every* line, including the ones that rhyme with nothing and are
/// therefore never pitched — so using it directly meant a poem's first heard note could be degree
/// 5, with degrees 0 to 4 belonging to unrhymed lines nobody hears. §4.5 says "assigned in order of
/// first appearance", and what appears is what sounds.
fn degrees(rhyme: &PoemRhyme) -> BTreeMap<crate::rhyme::RhymeClassId, usize> {
    let mut degrees: BTreeMap<crate::rhyme::RhymeClassId, usize> = BTreeMap::new();
    for (class, ..) in sounding(rhyme) {
        let next = degrees.len();
        degrees.entry(class).or_insert(next);
    }
    degrees
}

/// Marks the structural figures `SPEC.md` §4.3 names but the voice and family rows do not carry.
///
/// > Anaphora | line-initial n-gram repeats | downbeat-anchored ostinato
/// > Epistrophe | line-final n-gram repeats | cadence figure / turnaround
/// > Alliteration / assonance | onset or nucleus phoneme match in window | same sample bank
///
/// All three are computed by the analysis passes and were, until review, simply dropped — three
/// of the five rows of the section the spec calls its highest-value pass. They belong in the IR
/// rather than the backend because "downbeat-anchored" says *where in the bar* an event sits.
fn figures(events: &mut [Event], analyses: Analyses<'_>) {
    let mut anchors: BTreeMap<Span, Anchor> = BTreeMap::new();
    for figure in analyses.repetition.figures() {
        let anchor = match figure.kind() {
            FigureKind::Anaphora => Anchor::Downbeat,
            FigureKind::Epistrophe => Anchor::Cadence,
            // A refrain is already an instrument, not a position within a bar.
            FigureKind::Refrain => continue,
        };
        for occurrence in figure.occurrences() {
            for &word in occurrence.words() {
                anchors.entry(word).or_insert(anchor);
            }
        }
    }

    // First figure wins, so a word in two overlapping figures takes a stable bank rather than
    // whichever the iteration order reached last.
    let mut banks: BTreeMap<Span, crate::Phoneme> = BTreeMap::new();
    for figure in analyses.sound.figures() {
        for &word in figure.words() {
            banks.entry(word).or_insert(figure.phoneme());
        }
    }

    for event in events {
        if let Some(&anchor) = anchors.get(&event.span()) {
            *event = event.with_anchor(anchor);
        }
        if let Some(&bank) = banks.get(&event.span()) {
            *event = event.with_bank(bank);
        }
    }
}

/// Adds each word's distinctiveness and its place in its family (`SPEC.md` §4.4, §4.3).
fn colour(events: &mut [Event], analyses: Analyses<'_>) {
    let sharpness: BTreeMap<Span, Surprisal> = analyses
        .surprisal
        .words()
        .iter()
        .map(|word| (word.span(), word.surprisal()))
        .collect();
    let families: BTreeMap<Span, Family> = analyses
        .sound
        .forms()
        .iter()
        .map(|form| (form.span(), Family::new(form.voice(), form.distance())))
        .collect();

    for event in events {
        if let Some(&value) = sharpness.get(&event.span()) {
            *event = event.with_sharpness(value);
        }
        if let Some(&family) = families.get(&event.span()) {
            *event = event.with_family(family);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Analyses, DifferentPoems, run};
    use crate::phoneme::Provenance;
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };
    use crate::role::Layer;
    use crate::seed::{Domain, Seed};
    use crate::surprisal::{Lexicon, Rarity};
    use crate::voice::{Anchor, Score, Velocity};
    use crate::{Poem, meter, repetition, rhyme, role, sound, surprisal};

    /// A spelling-is-pronunciation phonemiser: `A` and `o` are stressed vowels, `e` an unstressed
    /// one, and the consonants are themselves. A test can therefore write a stress pattern
    /// straight into the poem and read the expected velocities back off it, and can build words
    /// that genuinely inflect ("bAn" / "bAns") so the family pass has something real to find.
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
                    // The middle rung of the velocity ladder. Without a letter for it, no fixture
                    // ever produced a secondary stress and swapping the constant went unnoticed.
                    'i' => Some((ARPABET.phoneme("IY").unwrap(), Stress::Secondary)),
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
            let syllables = syllabify(&phones).unwrap();
            // A word in capitals is "not in the dictionary", so a test can put a guess into a
            // poem and check that the IR keeps saying so.
            if word.starts_with('G') {
                return Ok(Pronunciation::guessed(syllables));
            }
            Ok(Pronunciation::from_dictionary(syllables))
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

    /// The note a rhyme gave an event (§4.5). The bass is a field of its own ([`crate::bass`]).
    fn rhyme_pitch(event: &crate::voice::Event) -> Option<crate::voice::Pitch> {
        event.pitch().filter(|pitch| pitch.class().is_some())
    }

    fn lower(source: &str) -> Score {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &Flat);
        let roles = role::assign(&meter, &repetition).unwrap();
        run(Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme,
            repetition: &repetition,
            sound: &sound,
            surprisal: &surprisal,
            roles: &roles,
        })
        .unwrap()
    }

    #[test]
    fn every_syllable_becomes_an_event() {
        // "eA" is two syllables, unstressed then stressed — an iamb. Two words, two feet, four
        // syllables, all in bar 0.
        let score = lower("eA eA\n");
        assert_eq!(score.events().len(), 4);
        assert_eq!(score.bars(), 1);
        let positions: Vec<_> = score
            .events()
            .iter()
            .map(|e| {
                (
                    e.bar().get(),
                    e.beat().get(),
                    e.subdivision().index(),
                    e.subdivision().of(),
                )
            })
            .collect();
        assert_eq!(
            positions,
            vec![(0, 0, 0, 2), (0, 0, 1, 2), (0, 1, 0, 2), (0, 1, 1, 2)]
        );
    }

    #[test]
    fn stress_becomes_velocity() {
        let score = lower("eA\n");
        let velocities: Vec<_> = score.events().iter().map(|e| e.velocity()).collect();
        assert_eq!(velocities, vec![Velocity::UNSTRESSED, Velocity::PRIMARY]);
    }

    #[test]
    fn all_three_stresses_reach_the_score_in_order() {
        // `CMUdict` distinguishes three stresses and `Velocity` has three rungs. Only the outer
        // two were ever exercised, so swapping the middle constant — making a secondary-stressed
        // syllable quieter than an unstressed one — passed the whole suite.
        let score = lower("eiA\n");
        let velocities: Vec<_> = score.events().iter().map(|e| e.velocity()).collect();
        assert_eq!(
            velocities,
            vec![Velocity::UNSTRESSED, Velocity::SECONDARY, Velocity::PRIMARY]
        );
        assert!(Velocity::UNSTRESSED < Velocity::SECONDARY);
        assert!(Velocity::SECONDARY < Velocity::PRIMARY);
    }

    #[test]
    fn a_line_is_a_bar_and_a_stanza_break_is_not() {
        // SPEC.md §4.2: line → bar. A blank line is a unit boundary, not a rest, so bars stay
        // consecutive across it — inserting a stanza break must not shift the music in time.
        let score = lower("eA\neA\n\neA\n");
        assert_eq!(score.bars(), 3);
        let bars: Vec<_> = score.events().iter().map(|e| e.bar().get()).collect();
        assert_eq!(bars, vec![0, 0, 1, 1, 2, 2]);
    }

    #[test]
    fn a_repeated_line_is_one_instrument_played_again() {
        // SPEC.md §4.3: a refrain is "a literal loop of that voice".
        let score = lower("eA eo\nAe\neA eo\n");
        assert_eq!(score.voices().len(), 2, "{:?}", score.voices());
        let refrain = score.voices()[0];
        assert_eq!(
            score.events_of(refrain.id()).count(),
            8,
            "two bars of four syllables"
        );
        // ...and it is the foundation, because it repeats.
        assert_eq!(refrain.layer(), Layer::Foundation);
    }

    #[test]
    fn only_the_rhyming_word_is_pitched() {
        // SPEC.md §4.5 maps rhyme to melody, and a rhyme is a property of one word. Pitching
        // every syllable by its rhyme would be a monotone and would attribute a rhyme's note to
        // syllables that earned none.
        let score = lower("eA eo\nAe eo\n");
        let pitched: Vec<_> = score
            .events()
            .iter()
            .filter(|e| rhyme_pitch(e).is_some())
            .collect();
        assert!(!pitched.is_empty());
        for event in &pitched {
            assert_eq!(event.span().text("eA eo\nAe eo\n"), Some("eo"));
        }
        assert!(score.events().iter().any(|e| rhyme_pitch(e).is_none()));
    }

    #[test]
    fn a_perfect_rhyme_sits_exactly_on_its_degree() {
        // Both lines end in the same word, so the rhyme is perfect and the pitch is exact.
        let score = lower("eA eo\nAe eo\n");
        let pitches: Vec<_> = score.events().iter().filter_map(rhyme_pitch).collect();
        assert!(!pitches.is_empty());
        for pitch in &pitches {
            assert!(pitch.detune().is_exact(), "{pitch:?}");
        }
        // One class, so one degree — both lines take the same note.
        assert!(pitches.iter().all(|p| p.degree() == pitches[0].degree()));
    }

    #[test]
    fn a_slant_rhyme_sits_off_its_degree() {
        // SPEC.md §4.5's elegant half: "perfect rhyme → exact pitch; slant rhyme → detune or
        // neighbour tone. Rhyme precision maps to pitch precision."
        //
        // Hand-worked. Under `Spelled`, "eAn" ends `AA N` and "eon" ends `OW N`. The score is a
        // *suffix* match from the last stressed vowel, so the shared `N` matches and the vowels
        // do not: 1 of 2, and therefore a detune of 1 − 1/2 = half a step.
        //
        // A shared vowel with different codas would score zero, not a half — rhyme scoring ranks
        // consonance above assonance, which is recorded as an open question in `PLAN.md`. Picking
        // the fixture to suit the metric would have hidden that, so it is named here instead.
        let source = "beA eAn\nbeA eon\n";
        let score = lower(source);
        let detunes: Vec<_> = score
            .events()
            .iter()
            .filter_map(|e| Some((e.span().text(source)?, rhyme_pitch(e)?.detune())))
            .collect();
        assert!(!detunes.is_empty(), "expected pitched rhyme words");
        for (word, detune) in &detunes {
            assert!(
                !detune.is_exact(),
                "{word} rhymes only partly, so it must sit off the degree"
            );
            assert!((detune.as_f64() - 0.5).abs() < 1e-9, "{word}: {detune:?}");
        }
    }

    #[test]
    fn pitch_precision_tracks_rhyme_precision() {
        // The ordering is the mapping: a closer rhyme must sit closer to its degree. Asserting a
        // direction rather than a value, so this stays true if the score's shape is retuned.
        let exact = "beA eAn\nbeA eAn\n";
        let slant = "beA eAn\nbeA eon\n";
        let detune_of = |source: &str| {
            lower(source)
                .events()
                .iter()
                .filter_map(rhyme_pitch)
                .map(crate::voice::Pitch::detune)
                .next()
        };
        let (Some(near), Some(far)) = (detune_of(exact), detune_of(slant)) else {
            unreachable!("both poems rhyme")
        };
        assert!(near.is_exact());
        assert!(far > near, "{far:?} should be further off than {near:?}");
    }

    #[test]
    fn a_line_that_rhymes_with_nothing_has_no_pitch() {
        // A class of one expresses no melodic relation, and inventing a degree for it would be
        // inventing a melody the poem never wrote. (Its stressed `A` still has a bass note, from
        // its vowel — a different mapping, tested below.)
        let score = lower("eA\n");
        assert!(score.events().iter().all(|e| rhyme_pitch(e).is_none()));
    }

    #[test]
    fn a_word_family_modulates_within_its_instrument() {
        // SPEC.md §4.3's morphological row. "bAn" and "bAnd" are a stem and its inflection, so
        // they share a word voice at different depths — one instrument, varied parameters.
        //
        // `bAnd`, not `bAns`: the family pass requires the allomorph English would actually use,
        // and after a voiced `N` that is `D`. A voiceless `S` there is rightly refused.
        let source = "bAn\nbAnd\n";
        let score = lower(source);
        let families: Vec<_> = score
            .events()
            .iter()
            .filter_map(|e| e.family().map(|f| (e.span().text(source), f)))
            .collect();
        assert!(!families.is_empty(), "expected families");
        let root = families
            .iter()
            .find(|(text, _)| *text == Some("bAn"))
            .expect("root");
        let varied = families
            .iter()
            .find(|(text, _)| *text == Some("bAnd"))
            .expect("inflection");
        assert_eq!(root.1.voice(), varied.1.voice(), "one family");
        assert!(root.1.depth().is_root());
        assert!(varied.1.depth().get() > 0, "varied is off the root");
    }

    #[test]
    fn events_come_out_in_playing_order() {
        let score = lower("eA eo Ae\neA\n\nAe eo\n");
        let order: Vec<_> = score.events().iter().map(super::Event::order).collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted);
    }

    #[test]
    fn analyses_of_different_poems_are_refused() {
        // The M5b defect, generalised: six analyses arrive here and spans cannot tell poems
        // apart.
        let a = Poem::parse("eA eo\n").unwrap();
        let b = Poem::parse("Ae oe\n").unwrap();
        assert_eq!(a.source().len(), b.source().len(), "same bytes");
        let meter = meter::scan(&a, &Spelled);
        let repetition = repetition::analyse(&a);
        let roles = role::assign(&meter, &repetition).unwrap();
        let result = run(Analyses {
            poem: &a,
            meter: &meter,
            rhyme: &rhyme::analyse(&b, &Spelled),
            repetition: &repetition,
            sound: &sound::analyse(&a, &Spelled),
            surprisal: &surprisal::analyse(&a, &Spelled, &Flat),
            roles: &roles,
        });
        assert_eq!(result, Err(DifferentPoems));
    }

    #[test]
    fn every_line_seeds_its_own_voice() {
        // `CLAUDE.md` §5 says `Seed::for_unit` takes a span so that a caller cannot pass the whole
        // poem where a unit belongs — but a *span* covering the whole poem still type-checks, and
        // this pass is its first real caller. Three mutations survived the suite until this test:
        // seeding from the stanza, from the poem, and under the wrong domain. All are invisible
        // to I1, which is exactly why §5 warns that no test of `Seed::for_unit` itself can catch
        // them.
        let source = "bAn\nteA\n\ndeA\nrAn\n";
        let score = lower(source);
        let seeds: Vec<_> = score.voices().iter().map(|v| v.seed()).collect();
        assert_eq!(seeds.len(), 4, "four distinct lines");
        for (i, a) in seeds.iter().enumerate() {
            for b in seeds.iter().skip(i + 1) {
                assert_ne!(a, b, "distinct lines must not share a seed");
            }
        }

        // ...the derivation itself is pinned, not merely its locality: seeding under the wrong
        // `Domain` is invisible to every property above, since it stays deterministic and stays
        // per-line.
        let first = score.voices()[0];
        assert_eq!(
            first.seed(),
            Seed::for_unit(Domain::Line, first.span(), source).unwrap()
        );
        assert_ne!(
            first.seed(),
            Seed::for_unit(Domain::Stanza, first.span(), source).unwrap(),
            "a line is not a stanza"
        );

        // ...and a line seeds identically wherever it sits, which is what makes an edit local.
        let moved = lower("deA\nrAn\n\nbAn\nteA\n");
        let there: Vec<_> = moved.voices().iter().map(|v| v.seed()).collect();
        assert_eq!(seeds.len(), there.len());
        for seed in &seeds {
            assert!(
                there.contains(seed),
                "a line's seed is its own content, not its position"
            );
        }
    }

    #[test]
    fn editing_one_line_leaves_every_other_voices_seed_alone() {
        // I2 in its own words: "changing one word reshuffles the whole piece" is what must not
        // happen. Seeding from the stanza or the poem breaks this and nothing else observes it.
        let before = lower("bAn\nteA\n\ndeA\nrAn\n");
        let after = lower("bAnd\nteA\n\ndeA\nrAn\n");
        // Voices come out ordered by `VoiceId`, which is assigned by first appearance, so editing
        // the first line cannot reorder the later ones — the sequence is comparable as it stands.
        let untouched = |score: &Score| {
            score
                .voices()
                .iter()
                .filter(|v| v.span().start() > 8)
                .map(|v| v.seed())
                .collect::<Vec<_>>()
        };
        assert_eq!(untouched(&before).len(), 2);
        assert_eq!(untouched(&before), untouched(&after));
    }

    #[test]
    fn different_rhyme_classes_take_different_degrees() {
        // ABAB. SPEC.md §4.5: "rhyme class → scale degree, assigned in order of first appearance."
        // Asserting only that class-mates agree is satisfied by a monotone, which is what a
        // surviving mutation proved: every degree collapsed to 0 and the whole suite passed.
        // The endings must differ in their *final* phoneme, not just their vowel: rhyme is scored
        // as a suffix match, so "bAn" and "don" would share a class on the `N` alone.
        let source = "teA bAn\nteA dot\nteA rAn\nteA rot\n";
        let score = lower(source);
        let degrees: Vec<_> = score
            .events()
            .iter()
            .filter_map(|e| Some((e.span().text(source)?, e.pitch()?.degree().get())))
            .collect();
        let of = |word: &str| degrees.iter().find(|(w, _)| *w == word).map(|(_, d)| *d);
        assert_eq!(of("bAn"), of("rAn"), "the A rhymes share a degree");
        assert_eq!(of("dot"), of("rot"), "the B rhymes share a degree");
        assert_ne!(of("bAn"), of("dot"), "different classes, different notes");
    }

    #[test]
    fn degrees_are_numbered_over_the_classes_that_sound() {
        // A poem whose first lines rhyme with nothing. Those classes exist in the rhyme analysis
        // and are never heard, so counting them would make the first *audible* note degree 2.
        let source = "teA bos\nteA bod\nteA bAn\nteA rAn\n";
        let score = lower(source);
        let mut degrees: Vec<_> = score
            .events()
            .iter()
            .filter_map(rhyme_pitch)
            .map(|p| p.degree().get())
            .collect();
        degrees.sort_unstable();
        degrees.dedup();
        assert_eq!(
            degrees,
            vec![0],
            "the one class that sounds is the first degree"
        );
    }

    #[test]
    fn anaphora_anchors_its_words_to_the_downbeat() {
        // SPEC.md §4.3: line-initial repetition becomes a "downbeat-anchored ostinato". The
        // figure is detected by the repetition pass and was dropped entirely by lowering until
        // review — one of three of §4.3's five rows that never reached the IR.
        let source = "teA bo rAn\nteA bo don\nteA bo ben\n";
        let score = lower(source);
        let anchored: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.anchor() == Some(Anchor::Downbeat))
            .filter_map(|e| e.span().text(source))
            .collect();
        assert!(!anchored.is_empty(), "expected an anaphora");
        assert!(
            anchored.iter().all(|w| *w == "teA" || *w == "bo"),
            "{anchored:?}"
        );
    }

    #[test]
    fn a_guessed_pronunciation_stays_visible_in_the_ir() {
        // CLAUDE.md §6: fallbacks must be visible. §8 makes the debug view the enforcement
        // mechanism for I4, and it can only render what the IR kept. Without this, an event whose
        // position, velocity, pitch and family all rest on a letter-to-sound guess is
        // byte-identical to one resting on the dictionary.
        let source = "bAn GeA\n";
        let score = lower(source);
        let guessed: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.rests_on_a_guess())
            .filter_map(|e| e.span().text(source))
            .collect();
        assert_eq!(guessed, vec!["GeA", "GeA"], "both syllables of the guess");
        let known: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.provenance() == Provenance::Dictionary)
            .filter_map(|e| e.span().text(source))
            .collect();
        assert_eq!(known, vec!["bAn"]);
    }

    #[test]
    fn a_word_nobody_can_pronounce_is_reported_rather_than_dropped() {
        // "zz" has no vowel, so `Spelled` refuses it. The line still occupies its bar and the
        // word is named, so a consumer holding only a Score can tell silence from brevity.
        let source = "bAn zz\n";
        let score = lower(source);
        assert_eq!(score.bars(), 1);
        assert_eq!(
            score
                .silent()
                .iter()
                .filter_map(|s| s.text(source))
                .collect::<Vec<_>>(),
            vec!["zz"]
        );
    }

    #[test]
    fn a_trailing_silent_line_still_occupies_its_bar() {
        // `bars()` was derived from the last event, so a poem ending in an unpronounceable line
        // came back a bar short and a backend would drop it.
        let score = lower("bAn\nteA\nzz\n");
        assert_eq!(score.bars(), 3, "three lines, three bars");
        assert!(score.events().iter().all(|e| e.bar().get() < 2));
    }

    #[test]
    fn a_new_rhyme_class_transposes_the_classes_after_it() {
        // Pinning a known I2 break rather than leaving it implicit. §4.5 numbers rhyme classes
        // "in order of first appearance", which is a fact about the whole poem — so introducing a
        // rhyme earlier shifts the degree of every class after it, and a byte-identical later
        // stanza is transposed.
        //
        // This test exists to make the behaviour visible and to fail loudly if someone changes it
        // by accident. See the module docs and `PLAN.md`; fixing it is a spec question.
        let degree_of = |source: &str, word: &str| {
            lower(source)
                .events()
                .iter()
                .find(|e| e.span().text(source) == Some(word))
                .and_then(|e| e.pitch())
                .map(|p| p.degree().get())
        };
        let one = "teA bAn\nteA rAn\n\nteA dot\nteA rot\n";
        let two = "teA bos\nteA ros\n\nteA bAn\nteA rAn\n\nteA dot\nteA rot\n";
        assert_eq!(degree_of(one, "dot"), Some(1));
        assert_eq!(
            degree_of(two, "dot"),
            Some(2),
            "an earlier rhyme shifts a later, untouched class"
        );
    }

    #[test]
    fn roles_from_another_poem_are_refused() {
        // The layer assignment is §4.1's "load-bearing decision", and it was the one input not
        // checked — so a poem could be lowered with another poem's arrangement and come back
        // `Ok`, with every voice playing the wrong layer.
        let a = Poem::parse("bAn teA\nbAn teA\n").unwrap();
        let b = Poem::parse("bAn teA\nrot dos\n").unwrap();
        assert_eq!(a.source().len(), b.source().len(), "same bytes");
        let meter = meter::scan(&a, &Spelled);
        let repetition = repetition::analyse(&a);
        let other_meter = meter::scan(&b, &Spelled);
        let other_roles = role::assign(&other_meter, &repetition::analyse(&b)).unwrap();
        let result = run(Analyses {
            poem: &a,
            meter: &meter,
            rhyme: &rhyme::analyse(&a, &Spelled),
            repetition: &repetition,
            sound: &sound::analyse(&a, &Spelled),
            surprisal: &surprisal::analyse(&a, &Spelled, &Flat),
            roles: &other_roles,
        });
        assert_eq!(result, Err(DifferentPoems));
    }

    #[test]
    fn lowering_is_deterministic() {
        assert_eq!(lower("eA eo\nAe eo\n"), lower("eA eo\nAe eo\n"));
    }

    #[test]
    fn an_empty_poem_lowers_to_silence() {
        let score = lower("");
        assert!(score.events().is_empty());
        assert!(score.voices().is_empty());
        assert_eq!(score.bars(), 0);
    }

    #[test]
    fn a_foundation_lines_stressed_syllables_carry_its_bass() {
        use crate::bass::BassReason;
        // Hand-worked. `bA teo` twice is a regular stanza, so foundation. Its primary stresses are
        // `A` (AA, open: degree 0) and `o` (OW, mid: degree 2); `e` is unstressed and gets nothing.
        // `teo` ends both lines, so it is a perfect rhyme and §4.5's note replaces its bass.
        let source = "bA teo\nbA teo\n";
        let score = lower(source);
        let notes: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.bar().get() == 0)
            .map(|e| (e.span().text(source).unwrap(), e.bass(), e.pitch()))
            .collect();
        let aa = ARPABET.phoneme("AA").unwrap();
        let bass = notes[0].1.unwrap();
        assert_eq!(notes[0].0, "bA");
        assert_eq!(bass.why(), BassReason::Height(aa));
        assert_eq!((bass.root().get(), bass.step().get()), (0, 0));
        assert!(
            notes
                .iter()
                .filter(|(word, ..)| *word == "teo")
                .all(|(_, bass, pitch)| bass.is_none() && pitch.is_some()),
            "the rhyme's note, and no bass, on a rhyme word: {notes:?}"
        );
    }

    #[test]
    fn vowel_height_chooses_the_bass_degree() {
        // `bo` is OW, a mid vowel: degree 2 (`bA` above showed the open AA at 0). `ti` carries
        // only secondary stress, so no bass — the bass follows the primary accents. `tA` and `sA`
        // rhyme, so they take §4.5's note.
        let source = "bo ti tA\nbo ti sA\n";
        let score = lower(source);
        let bass: Vec<_> = score
            .events()
            .iter()
            .filter(|e| e.bar().get() == 0)
            .filter_map(|e| Some((e.span().text(source)?, e.bass()?)))
            .map(|(word, note)| (word, note.step().get()))
            .collect();
        assert_eq!(bass, vec![("bo", 2)]);
    }

    #[test]
    fn a_flourish_line_has_no_bass() {
        // Stanza 1 is regular, so foundation; stanza 2 — one syllable against four — is
        // irregular, so flourish. §4.1's bass belongs to the foundation only.
        let score = lower("tA tA\nsA sA\n\ndA\nnA nA nA nA\n");
        let has_bass = |bar: usize| {
            score
                .events()
                .iter()
                .any(|e| e.bar().get() == bar && e.bass().is_some())
        };
        assert!(has_bass(0) && has_bass(1));
        assert!(!has_bass(2) && !has_bass(3));
    }
}
