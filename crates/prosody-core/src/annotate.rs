//! The debug annotation view: `SPEC.md` §8.
//!
//! > Build the debug view alongside it: render the poem with each token annotated by what it
//! > became (role, beat position, pitch class, modulation, seed). This will earn its keep within a
//! > day and is **the only real defence against I4 quietly failing**.
//!
//! # Why this is a pass and not a printf
//!
//! Invariant I4 says every audible event must be attributable to a specific token span. Nothing
//! enforces that by construction — a `Span` proves an event *claims* a range of bytes, not that
//! the claim is right, and a wrong claim is invisible in the output of a backend. What catches it
//! is a reader looking at the poem with the events written under the words.
//!
//! So this module runs the attribution *backwards*. Every other pass goes text → structure; this
//! one takes the finished [`Score`] and asks, for each token the poem actually contains, what
//! became of it. A token with no events is as interesting as one with too many, and both are
//! visible here and nowhere else.
//!
//! # Which pass decided each
//!
//! `CLAUDE.md` §8 asks the view to say not just what a token became but *which pass decided it*,
//! and that is the part that makes a wrong mapping findable rather than merely visible: "this
//! syllable is loud" is a fact, "this syllable is loud because the meter pass called it a primary
//! stress" is a lead. Every annotated value carries its [`Pass`], and every [`Pass`] carries the
//! `SPEC.md` section that specified it.
//!
//! Adding a mapping means adding its contribution here in the same change. There is a test that
//! fails when a mapping reaches the IR without reaching this view — twice now a row of `SPEC.md`
//! §4.3 has been computed, carried, and silently dropped one layer later, and both times the thing
//! that would have caught it is a reader being able to see that the column was empty.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::arrange::Cause;
use crate::bass::{BassNote, BassReason, Chromatic, Direction, Height};
use crate::drums::Drum;
use crate::harmony::{Chord, Harmony, Reason, Silence};
use crate::mapping::Variant;
use crate::meter::FootKind;
use crate::parse::{Poem, Token};
use crate::phoneme::{Phoneme, Provenance};
use crate::role::{Layer, LineBasis, StanzaBasis};
use crate::seed::Seed;
use crate::span::Span;
use crate::voice::{Event, Family, Pitch, Score, Straightened, Velocity};

/// Which analysis pass decided a value, and where `SPEC.md` says so.
///
/// Carried on every annotation rather than inferred from the field name, because the point of the
/// view is to turn "that sounds wrong" into "that pass is wrong".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pass {
    /// Which layer a stanza plays.
    Role,
    /// Where a syllable sounds and how hard.
    Meter,
    /// Which lines are one instrument, and which are figures.
    Repetition,
    /// Word families, alliteration and assonance.
    Sound,
    /// How distinctive a word is.
    Surprisal,
    /// Which note a line takes.
    Rhyme,
    /// How a word is pronounced, and whether that was looked up or guessed.
    Phoneme,
    /// Per-unit seeding.
    Seed,
    /// Which bars of the piece a line sounds in: stated, looping, transforming, overruling.
    Arrange,
    /// Which chord a line's bar has, or why it has none — only when the harmony layer is on.
    Harmony,
    /// Why each note of a walking bass is the note it is — only when the bass walks.
    Bass,
}

impl Pass {
    /// The pass's name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Role => "role",
            Self::Meter => "meter",
            Self::Repetition => "repetition",
            Self::Sound => "sound",
            Self::Surprisal => "surprisal",
            Self::Rhyme => "rhyme",
            Self::Phoneme => "phoneme",
            Self::Seed => "seed",
            Self::Arrange => "arrange",
            Self::Harmony => "harmony",
            Self::Bass => "bass",
        }
    }

    /// The `SPEC.md` section that specified it.
    #[must_use]
    pub const fn section(self) -> &'static str {
        match self {
            Self::Role | Self::Arrange => "§4.1",
            Self::Harmony | Self::Bass => "§4.6",
            Self::Meter => "§4.2",
            Self::Repetition | Self::Sound => "§4.3",
            Self::Surprisal => "§4.4",
            Self::Rhyme => "§4.5",
            Self::Phoneme => "§7.2",
            Self::Seed => "I2",
        }
    }

    /// Every pass, for the legend. The legend lists [`Pass::Harmony`] only when the poem has a
    /// harmony, and [`Pass::Bass`] only when the bass walks, so a view without either reads as it
    /// did before they existed. (The height bass is listed as the `role` pass, as it always was.)
    pub const ALL: [Self; 11] = [
        Self::Role,
        Self::Meter,
        Self::Repetition,
        Self::Sound,
        Self::Surprisal,
        Self::Rhyme,
        Self::Phoneme,
        Self::Seed,
        Self::Arrange,
        Self::Harmony,
        Self::Bass,
    ];
}

/// One syllable of a token, and where it sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syllable {
    bar: usize,
    beat: usize,
    index: usize,
    of: usize,
    foot: FootKind,
    velocity: Velocity,
    light: bool,
    drum: Drum,
}

impl Syllable {
    /// Which bar, beat and subdivision — `SPEC.md` §8's "beat position" ([`Pass::Meter`]).
    #[must_use]
    pub const fn position(&self) -> (usize, usize, usize, usize) {
        (self.bar, self.beat, self.index, self.of)
    }

    /// The foot this beat is.
    #[must_use]
    pub const fn foot(&self) -> FootKind {
        self.foot
    }

    /// How hard it is struck.
    #[must_use]
    pub const fn velocity(&self) -> Velocity {
        self.velocity
    }

    /// Whether the light function words struck it lightly ([`Event::is_light`]): the row's gain
    /// was decided by `Mapping::Function`, not by the dictionary's stress alone.
    #[must_use]
    pub const fn is_light(&self) -> bool {
        self.light
    }

    /// Which piece of the drum kit this syllable plays ([`Event::drum`]).
    #[must_use]
    pub const fn drum(&self) -> Drum {
        self.drum
    }
}

/// What became of one token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    span: Span,
    syllables: Vec<Syllable>,
    pitch: Option<Pitch>,
    bass: Option<BassNote>,
    basses: Vec<Option<BassNote>>,
    modulation: Option<Family>,
    sharpness: Option<crate::surprisal::Surprisal>,
    bank: Option<Phoneme>,
    anchor: Option<crate::voice::Anchor>,
    provenance: Option<Provenance>,
    silent: bool,
}

impl Annotation {
    /// The token this describes.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Every syllable it sounds, in playing order. Empty when the token makes no sound.
    #[must_use]
    pub fn syllables(&self) -> &[Syllable] {
        &self.syllables
    }

    /// Its note, if it is the word a rhyme rests on ([`Pass::Rhyme`]).
    #[must_use]
    pub const fn pitch(&self) -> Option<Pitch> {
        self.pitch
    }

    /// Its first bass note, if the bass gave it one ([`crate::bass`]).
    #[must_use]
    pub const fn bass(&self) -> Option<BassNote> {
        self.bass
    }

    /// The bass note on each of its syllables, index for index with [`Annotation::syllables`]:
    /// a walking bass can put a different note on every one.
    #[must_use]
    pub fn basses(&self) -> &[Option<BassNote>] {
        &self.basses
    }

    /// Its place in a word family — `SPEC.md` §8's "modulation" ([`Pass::Sound`]).
    #[must_use]
    pub const fn modulation(&self) -> Option<Family> {
        self.modulation
    }

    /// How distinctive it is ([`Pass::Surprisal`]).
    #[must_use]
    pub const fn sharpness(&self) -> Option<crate::surprisal::Surprisal> {
        self.sharpness
    }

    /// The phoneme binding it to its alliterating neighbours ([`Pass::Sound`]).
    #[must_use]
    pub const fn bank(&self) -> Option<Phoneme> {
        self.bank
    }

    /// Which structural figure it belongs to ([`Pass::Repetition`]).
    #[must_use]
    pub const fn anchor(&self) -> Option<crate::voice::Anchor> {
        self.anchor
    }

    /// Whether its pronunciation was looked up or guessed ([`Pass::Phoneme`]).
    #[must_use]
    pub const fn provenance(&self) -> Option<Provenance> {
        self.provenance
    }

    /// Whether the phonemiser could not pronounce it at all.
    ///
    /// Distinct from "has no syllables": a token can be absent from the score because nobody could
    /// say it, which `CLAUDE.md` §6 requires be visible rather than read as a short line.
    #[must_use]
    pub const fn is_silent(&self) -> bool {
        self.silent
    }
}

/// One line of the poem, with its role and its tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnnotatedLine {
    span: Span,
    bar: Option<usize>,
    layer: Option<Layer>,
    basis: Option<LineBasis>,
    voice: Option<crate::repetition::VoiceId>,
    seed: Option<Seed>,
    sounds: Vec<(usize, Cause)>,
    walks: Vec<(usize, Cause)>,
    harmony: Option<Harmony>,
    bass: Vec<(Syllable, BassNote)>,
    straightened: Vec<(usize, Straightened)>,
    erosion: Vec<Vec<Span>>,
    tokens: Vec<Annotation>,
}

impl AnnotatedLine {
    /// The line's extent.
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// Which bar it plays in.
    #[must_use]
    pub const fn bar(&self) -> Option<usize> {
        self.bar
    }

    /// Which layer — `SPEC.md` §8's "role" ([`Pass::Role`]).
    #[must_use]
    pub const fn layer(&self) -> Option<Layer> {
        self.layer
    }

    /// Why it got that layer.
    #[must_use]
    pub const fn basis(&self) -> Option<LineBasis> {
        self.basis
    }

    /// Which instrument plays it ([`Pass::Repetition`]).
    #[must_use]
    pub const fn voice(&self) -> Option<crate::repetition::VoiceId> {
        self.voice
    }

    /// Its voice's seed — `SPEC.md` §8's "seed" ([`Pass::Seed`]).
    #[must_use]
    pub const fn seed(&self) -> Option<Seed> {
        self.seed
    }

    /// Every bar of the piece it sounds in, and why ([`Pass::Arrange`]), in playing order.
    #[must_use]
    pub fn sounds(&self) -> &[(usize, Cause)] {
        &self.sounds
    }

    /// The bars of [`AnnotatedLine::sounds`] where its bass sounds too ([`Score::sounds_bass`]):
    /// all of them under the height bass, and under a walking bass only those where it is the
    /// bar's foundation line — one walk a bar.
    #[must_use]
    pub fn walks(&self) -> &[(usize, Cause)] {
        &self.walks
    }

    /// Its bar's chord, or why it has none ([`Pass::Harmony`]) — `None` when the harmony layer
    /// is off.
    #[must_use]
    pub const fn harmony(&self) -> Option<Harmony> {
        self.harmony
    }

    /// Every bass note of the line, with the syllable it sounds on, in playing order
    /// ([`crate::bass`]).
    #[must_use]
    pub fn bass(&self) -> &[(Syllable, BassNote)] {
        &self.bass
    }

    /// Every beat the straight stack fitted to the host of the slot it sounds in, with that
    /// slot's index, in playing order (`crate::stack`) — empty under the default, crossing.
    #[must_use]
    pub fn straightened(&self) -> &[(usize, Straightened)] {
        &self.straightened
    }

    /// What the line keeps on each pass of an erosion, from pass 1 — the spans of the syllables
    /// still sounding, in playing order (`crate::arrange`'s erosion, `SPEC.md` §4.6) — or nothing
    /// when the line never erodes: only a stanza whose loop a long arrival wore away does.
    #[must_use]
    pub fn erosion(&self) -> &[Vec<Span>] {
        &self.erosion
    }

    /// Its tokens, in source order.
    #[must_use]
    pub fn tokens(&self) -> &[Annotation] {
        &self.tokens
    }
}

/// A whole poem, annotated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotated {
    stanzas: Vec<Vec<AnnotatedLine>>,
    basis: Vec<StanzaBasis>,
    bass: Variant,
    pulse: Variant,
    stack: Variant,
    /// [`Score::straightened_beats`]: each fitted beat of a slot once, as the pattern header and
    /// the OSC schedule count it — not once per line fitted there.
    straightened_beats: usize,
    function: Variant,
    /// [`Score::lightened`]: the syllables the light function words struck lightly.
    lightened: usize,
    drums: Variant,
    /// [`Score::articulated`]: the syllables whose drum piece differs from their layer's flat
    /// piece.
    articulated: usize,
    rhyme: Variant,
    anchor: Variant,
    /// [`Score::anchored`]: the syllables carrying an anaphora or epistrophe anchor.
    anchored: usize,
    melody: Variant,
    /// [`Score::assonant`]: the syllables carrying a note from an assonance figure.
    assonant: usize,
    /// [`Arrangement::eroding`](crate::arrange::Arrangement::eroding): the bars played over an
    /// eroding loop.
    eroding: usize,
    lengths: Vec<usize>,
}

impl Annotated {
    /// The poem's stanzas, each a list of lines.
    #[must_use]
    pub fn stanzas(&self) -> &[Vec<AnnotatedLine>] {
        &self.stanzas
    }

    /// Which variant of the pulse the piece is played at (`SPEC.md` §4.2).
    #[must_use]
    pub const fn pulse(&self) -> Variant {
        self.pulse
    }

    /// Which variant of the stack lays out its stacked lines (`SPEC.md` §4.2, §4.6).
    #[must_use]
    pub const fn stack(&self) -> Variant {
        self.stack
    }

    /// How many beats of the piece the straight stack fitted, each `(slot, beat)` once
    /// ([`Score::straightened_beats`]).
    #[must_use]
    pub const fn straightened_beats(&self) -> usize {
        self.straightened_beats
    }

    /// Which variant of the function words struck the syllables (`SPEC.md` §4.2).
    #[must_use]
    pub const fn function(&self) -> Variant {
        self.function
    }

    /// How many syllables the light function words struck lightly ([`Score::lightened`]).
    #[must_use]
    pub const fn lightened(&self) -> usize {
        self.lightened
    }

    /// Which variant of the drums assigned each syllable's drum piece (`SPEC.md` §4.1, §4.2).
    #[must_use]
    pub const fn drums(&self) -> Variant {
        self.drums
    }

    /// How many syllables play a drum piece different from their layer's flat default
    /// ([`Score::articulated`]).
    #[must_use]
    pub const fn articulated(&self) -> usize {
        self.articulated
    }

    /// Which variant of rhyme scoring formed the poem's rhyme classes (`SPEC.md` §4.5).
    #[must_use]
    pub const fn rhyme(&self) -> Variant {
        self.rhyme
    }

    /// Which variant of repetition anchor rendering was chosen (`SPEC.md` §4.3).
    #[must_use]
    pub const fn anchor(&self) -> Variant {
        self.anchor
    }

    /// How many syllables carry an anaphora or epistrophe anchor ([`Score::anchored`]).
    #[must_use]
    pub const fn anchored(&self) -> usize {
        self.anchored
    }

    /// Which variant of melody assignment was chosen (`SPEC.md` §4.3, §4.5).
    #[must_use]
    pub const fn melody(&self) -> Variant {
        self.melody
    }

    /// How many syllables carry a note from an assonance figure ([`Score::assonant`]).
    #[must_use]
    pub const fn assonant(&self) -> usize {
        self.assonant
    }

    /// How many bars play over a loop eroding under a long stanza's arrival (`SPEC.md` §4.6).
    #[must_use]
    pub const fn eroding(&self) -> usize {
        self.eroding
    }

    /// Every bar length of the piece, in beats, smallest first, once each: what the pulse
    /// decides the duration of.
    #[must_use]
    pub fn bar_lengths(&self) -> &[usize] {
        &self.lengths
    }

    /// Which variant of the bass made the bass notes.
    #[must_use]
    pub const fn bass(&self) -> Variant {
        self.bass
    }

    /// Why each stanza got its layer.
    #[must_use]
    pub fn stanza_basis(&self) -> &[StanzaBasis] {
        &self.basis
    }

    /// Every line, in reading order.
    pub fn lines(&self) -> impl Iterator<Item = &AnnotatedLine> + '_ {
        self.stanzas.iter().flatten()
    }

    /// Every token that produced no sound at all.
    ///
    /// The view's most useful single question: a word here is either unpronounceable (visible, and
    /// `CLAUDE.md` §6 requires it be) or a hole in the attribution — a token the pipeline lost.
    pub fn silent(&self) -> impl Iterator<Item = &Annotation> + '_ {
        self.lines()
            .flat_map(AnnotatedLine::tokens)
            .filter(|token| token.syllables.is_empty())
    }
}

/// Annotates a poem with what became of it (`SPEC.md` §8).
///
/// Takes the roles as well as the score because §8 asks for the *role* by name, and the score
/// carries a layer without the reason for it — and the reason is what makes a wrong role findable.
///
/// Pure, like every other pass: rendering is separate, so the annotations can be asserted against
/// rather than scraped from a string.
#[must_use]
pub fn run(poem: &Poem, score: &Score, roles: &crate::role::PoemRoles) -> Annotated {
    // Events by the token they came from. A token may sound several syllables, and they are kept
    // in playing order so the view reads the way the piece plays.
    let mut by_token: BTreeMap<Span, Vec<&Event>> = BTreeMap::new();
    for event in score.events() {
        by_token.entry(event.span()).or_default().push(event);
    }
    let silent: std::collections::BTreeSet<Span> = score.silent().iter().copied().collect();

    // A family of one member is not a family. `SPEC.md` §4.3's morphological row is about *near
    // repetition* — "burn / burning / burnt" — so a word that shares its family with nothing has
    // no variation to modulate, and showing it as one buries the real families in noise. The IR
    // records a family for every word; whether it should is recorded in `PLAN.md`.
    let mut family_sizes: BTreeMap<crate::sound::WordVoiceId, usize> = BTreeMap::new();
    for event in score.events() {
        if let Some(family) = event.family() {
            *family_sizes.entry(family.voice()).or_default() += 1;
        }
    }

    // The line each bar belongs to, so a line can name its bar.
    let mut bars: BTreeMap<Span, usize> = BTreeMap::new();
    for event in score.events() {
        bars.entry(line_of(poem, event.span()).unwrap_or(event.span()))
            .or_insert_with(|| event.bar().get());
    }

    // Which layer each line plays, and why. `PoemRoles` is the only place the *reason* lives.
    let line_roles: BTreeMap<Span, (Layer, LineBasis)> = roles
        .lines()
        .map(|(_, line)| (line.span(), (line.layer(), line.basis())))
        .collect();

    // Every line's bar, counted the way the score counts them — one per line in reading order —
    // so a line with no events still finds its harmony.
    let mut next_bar = 0_usize;
    let stanzas = poem
        .stanzas()
        .iter()
        .map(|stanza| {
            stanza
                .lines()
                .iter()
                .map(|line| {
                    let index = next_bar;
                    next_bar += 1;
                    let tokens: Vec<Annotation> = line
                        .words()
                        .map(|token| annotate(*token, &by_token, &silent, &family_sizes))
                        .collect();
                    let voice = tokens
                        .iter()
                        .find_map(|token| token.syllables.first().map(|_| ()))
                        .and_then(|()| {
                            score
                                .events()
                                .iter()
                                .find(|event| line.span().contains(event.span()))
                                .map(|event| event.voice())
                        });
                    AnnotatedLine {
                        span: line.span(),
                        bar: bars.get(&line.span()).copied(),
                        layer: line_roles.get(&line.span()).map(|(layer, _)| *layer),
                        basis: line_roles.get(&line.span()).map(|(_, basis)| *basis),
                        voice,
                        seed: voice.and_then(|id| score.voice(id)).map(|v| v.seed()),
                        sounds: bars
                            .get(&line.span())
                            .map(|&bar| sounds_of(score, bar))
                            .unwrap_or_default(),
                        walks: bars
                            .get(&line.span())
                            .map(|&bar| bass_sounds_of(score, bar))
                            .unwrap_or_default(),
                        harmony: score.harmony_of(index),
                        bass: score
                            .events()
                            .iter()
                            .filter(|event| event.bar().get() == index)
                            .filter_map(|event| Some((syllable(event), event.bass()?)))
                            .collect(),
                        straightened: straightened_of(score, index),
                        erosion: erosion_of(score, index),
                        tokens,
                    }
                })
                .collect()
        })
        .collect();

    Annotated {
        stanzas,
        basis: roles
            .stanzas()
            .iter()
            .map(crate::role::StanzaRole::basis)
            .collect(),
        bass: score.bass(),
        pulse: score.pulse(),
        stack: score.stack(),
        straightened_beats: score.straightened_beats(),
        function: score.function(),
        lightened: score.lightened(),
        drums: score.drums(),
        articulated: score.articulated(),
        rhyme: score.rhyme(),
        anchor: score.anchor(),
        anchored: score.anchored(),
        melody: score.melody(),
        assonant: score.assonant(),
        eroding: score.arrangement().eroding(),
        lengths: bar_lengths(score),
    }
}

/// Every bar length of the piece, in beats, smallest first, once each.
fn bar_lengths(score: &Score) -> Vec<usize> {
    let mut lengths: Vec<usize> = score
        .arrangement()
        .slots()
        .iter()
        .map(|slot| slot.beats().max(1))
        .collect();
    lengths.sort_unstable();
    lengths.dedup();
    lengths
}

/// Where an event sounds, as the view shows it.
const fn syllable(event: &Event) -> Syllable {
    Syllable {
        bar: event.bar().get(),
        beat: event.beat().get(),
        index: event.subdivision().index(),
        of: event.subdivision().of(),
        foot: event.foot(),
        velocity: event.velocity(),
        light: event.is_light(),
        drum: event.drum(),
    }
}

/// The line a token belongs to.
fn line_of(poem: &Poem, token: Span) -> Option<Span> {
    poem.stanzas()
        .iter()
        .flat_map(crate::parse::Stanza::lines)
        .find(|line| line.span().contains(token))
        .map(crate::parse::Line::span)
}

/// What became of one token.
fn annotate(
    token: Token,
    by_token: &BTreeMap<Span, Vec<&Event>>,
    silent: &std::collections::BTreeSet<Span>,
    family_sizes: &BTreeMap<crate::sound::WordVoiceId, usize>,
) -> Annotation {
    let events = by_token.get(&token.span());
    let syllables = events.map_or_else(Vec::new, |events| {
        events.iter().map(|event| syllable(event)).collect()
    });
    let first = events.and_then(|events| events.first().copied());

    Annotation {
        span: token.span(),
        syllables,
        pitch: events.and_then(|events| events.iter().find_map(|event| event.pitch())),
        bass: events.and_then(|events| events.iter().find_map(|event| event.bass())),
        basses: events.map_or_else(Vec::new, |events| {
            events.iter().map(|event| event.bass()).collect()
        }),
        modulation: first.and_then(|event| event.family()).filter(|family| {
            // Only a family with company: more events in the family than this word contributes.
            // See the note where `family_sizes` is built.
            let mine = events.map_or(0, Vec::len);
            family_sizes.get(&family.voice()).copied().unwrap_or(0) > mine
        }),
        sharpness: first.map(|event| event.sharpness()),
        bank: first.and_then(|event| event.bank()),
        anchor: first.and_then(|event| event.anchor()),
        provenance: first.map(|event| event.provenance()),
        silent: silent.contains(&token.span()),
    }
}

/// Renders the annotated poem as text (`SPEC.md` §8).
///
/// The poem, line by line, with each token's fate written under it. Kept here rather than in the
/// CLI so the layout is testable without running a binary — but it is a plain `String`, so nothing
/// about it touches IO.
#[must_use]
#[allow(clippy::too_many_lines)] // one table, written out once; splitting it hides the layout
pub fn render(poem: &Poem, annotated: &Annotated) -> String {
    let source = poem.source();
    let mut out = String::new();

    let _ = writeln!(out, "passes");
    let harmonised = annotated.lines().any(|line| line.harmony().is_some());
    let walking = walks(annotated.bass());
    for pass in Pass::ALL {
        if (pass == Pass::Harmony && !harmonised) || (pass == Pass::Bass && !walking) {
            continue;
        }
        let _ = writeln!(out, "  {:<11} {}", pass.name(), pass.section());
    }
    // §4.2's pulse: how long the bars below last, which no token decides alone.
    let lengths: Vec<String> = annotated
        .bar_lengths()
        .iter()
        .map(ToString::to_string)
        .collect();
    let _ = writeln!(
        out,
        "\npulse {} · bars of {} beats (§4.2)",
        match annotated.pulse() {
            Variant::Elastic => {
                "elastic · every bar the same span, so a bar of more beats hurries them"
            }
            _ => "steady · every bar lasts its beats, so the beat keeps one speed",
        },
        lengths.join(", ")
    );
    // The straight stack (§4.2, §4.6), named only when chosen: under the default each line keeps
    // its own feet wherever it sounds, which every row below already shows.
    if annotated.stack() == Variant::Straight {
        let fitted = annotated.straightened_beats();
        let _ = writeln!(
            out,
            "stack straight · a line stacked under a bar's line is fitted to its subdivision \
             where the two would cross: {fitted} beat(s) (§4.2, §4.6)"
        );
    }
    // The light function words (§4.2), named only when chosen, like the straight stack: under
    // the default every gain below is its syllable's dictionary stress.
    if annotated.function() == Variant::Light {
        let _ = writeln!(
            out,
            "function-words light · a one-syllable function word off the metre's strong \
             positions is struck at 0.67, not full force: {} syllable(s) (§4.2)",
            annotated.lightened()
        );
    }
    if annotated.drums() == Variant::Articulated {
        let _ = writeln!(
            out,
            "drums articulated · each syllable's drum piece is chosen by its layer, beat and \
             stress: {} syllable(s) off the layer's flat piece (§4.1, §4.2)",
            annotated.articulated()
        );
    }
    if annotated.rhyme() == Variant::Slant {
        let _ = writeln!(
            out,
            "rhyme slant · candidate tails from last stress and final syllable rime, with \
             apocopated and continuous slant rhyme scoring (§4.5)"
        );
    }
    if annotated.anchor() == Variant::On {
        let _ = writeln!(
            out,
            "anchor on · anaphora ostinatos accented on the downbeat, epistrophe cadences \
             ring through turnarounds: {} syllable(s) anchored (§4.3)",
            annotated.anchored()
        );
    }
    if annotated.melody() == Variant::Assonance {
        let _ = writeln!(
            out,
            "melody assonance · line-internal assonance figures give melody to unrhymed lines: \
             {} event(s) pitched (§4.3, §4.5)",
            annotated.assonant()
        );
    }
    // The erosion of long stanzas (§4.6), named only where it happened: a piece whose stanzas
    // are all eight lines or fewer is transformed a line a pass under either variant.
    if annotated.eroding() > 0 {
        let _ = writeln!(
            out,
            "long-stanzas erode · a long stanza's arrival thins the old loop out over {} passes \
             until silent, unstressed syllables first, rhyme words last: {} bar(s) over an eroding \
             loop (§4.6)",
            crate::arrange::EROSION_PASSES,
            annotated.eroding()
        );
    }
    let _ = writeln!(out);

    for (index, stanza) in annotated.stanzas().iter().enumerate() {
        let _ = writeln!(out, "stanza {index}");
        for line in stanza {
            let text = line.span().text(source).unwrap_or("<lost>");
            let _ = writeln!(
                out,
                "\n  {text}\n  {}",
                "─".repeat(text.chars().count().max(1))
            );
            let _ = writeln!(
                out,
                "  bar {}  {} ({})  voice {}  seed {}",
                line.bar()
                    .map_or_else(|| "-".to_owned(), |bar| bar.to_string()),
                line.layer().map_or_else(
                    || "silent".to_owned(),
                    |layer| format!("{layer:?}").to_lowercase()
                ),
                line.basis().map_or_else(
                    || "-".to_owned(),
                    |basis| match basis {
                        LineBasis::Refrain(voice) => format!("refrain of voice {}", voice.get()),
                        LineBasis::Inherited(stanza) => format!("{stanza:?}").to_lowercase(),
                    }
                ),
                line.voice()
                    .map_or_else(|| "-".to_owned(), |voice| voice.get().to_string()),
                line.seed()
                    .map_or_else(|| "-".to_owned(), |seed| format!("{seed}")[..8].to_owned()),
            );
            if !line.sounds().is_empty() {
                let _ = writeln!(out, "  sounds {}", describe_sounds(line.sounds()));
            }
            if !line.erosion().is_empty() {
                let _ = writeln!(out, "  erodes {}", describe_erosion(source, line));
            }
            if let Some(harmony) = line.harmony() {
                for row in describe_harmony(harmony) {
                    let _ = writeln!(out, "  harmony {row}");
                }
            }
            if !line.straightened().is_empty() {
                let _ = writeln!(
                    out,
                    "  straight {}",
                    describe_straightened(line.straightened())
                );
            }
            if walking && !line.bass().is_empty() {
                render_bass(&mut out, source, annotated.bass(), harmonised, line);
            }
            let _ = writeln!(
                out,
                "    {:<14} {:<10} {:<7} {:<9} {:<9} {:<7} {:<6} from",
                "token", "beat", "gain", "pitch", "modulation", "sharp", "bank"
            );
            for token in line.tokens() {
                render_token(
                    &mut out,
                    source,
                    token,
                    walking,
                    annotated.drums() == Variant::Articulated,
                    annotated.anchor() == Variant::On,
                );
            }
        }
        let _ = writeln!(out);
    }
    out
}

/// Whether the bass walks: W1 or W2. The height bass is shown in the pitch column, as before.
const fn walks(variant: Variant) -> bool {
    matches!(variant, Variant::Syllables | Variant::Feet)
}

/// A walking line's bass, one note a row: where it sounds, what it traces to, its note, and the
/// rule that chose it (`SPEC.md` §4.6, the bass).
fn render_bass(
    out: &mut String,
    source: &str,
    variant: Variant,
    harmonised: bool,
    line: &AnnotatedLine,
) {
    let what = match variant {
        Variant::Feet => "feet · a note a foot on the beat, by the foot's vowel height (W2)",
        _ => "syllables · every syllable walks: stressed by height, steps between (W1)",
    };
    let _ = writeln!(out, "  bass {what}");
    // One walk a bar (`Score::sounds_bass`): where the line sounds under another bar's foundation
    // line — held, refrain, call — its walk is silent, so two walks never clash by a semitone.
    let silent: Vec<(usize, Cause)> = line
        .sounds()
        .iter()
        .filter(|sound| !line.walks().contains(sound))
        .copied()
        .collect();
    let _ = writeln!(
        out,
        "    walks in {}",
        if line.walks().is_empty() {
            "no bar".to_owned()
        } else {
            describe_sounds(line.walks())
        }
    );
    if !silent.is_empty() {
        let _ = writeln!(
            out,
            "    silent in {} — one walk a bar, the bar's foundation line's",
            describe_sounds(&silent)
        );
    }
    for (syllable, note) in line.bass() {
        let (bar, beat, index, of) = syllable.position();
        let _ = writeln!(
            out,
            "    {:<14} {:<10} {:<9} {}",
            note.span().text(source).unwrap_or("<lost>"),
            format!("{bar}.{beat}.{index}/{of}"),
            tone(*note),
            describe_bass(*note, harmonised)
        );
    }
}

/// A bass note's tone: its step above the root, `-` when a semitone below it, and `@root` when it
/// counts from a chord's root rather than the scale's.
fn tone(note: BassNote) -> String {
    let below = match note.chromatic() {
        Chromatic::Diatonic => "",
        Chromatic::SemitoneBelow => "-",
    };
    let root = note.root().get();
    if root == 0 {
        format!("step {}{below}", note.step().get())
    } else {
        format!("{}{below}@{root}", note.step().get())
    }
}

/// The tone a height names, as words: `AY is open, so the root`.
fn height_words(vowel: crate::phoneme::Phoneme) -> String {
    Height::of(vowel).map_or_else(
        || format!("{} has no height", vowel.symbol()),
        |height| {
            format!(
                "{} is {}, so the {}",
                vowel.symbol(),
                height.name(),
                height.tone_name()
            )
        },
    )
}

/// Which chord a W2 note's tones come from.
fn chord_words(chord: Option<Chord>, harmonised: bool) -> String {
    match chord {
        Some(chord) => format!("{} deg {}", chord.function().numeral(), chord.root().get()),
        None if harmonised => "the scale (this bar has no chord)".to_owned(),
        None => "the scale (harmony off)".to_owned(),
    }
}

/// Why a bass note is the note it is.
fn describe_bass(note: BassNote, harmonised: bool) -> String {
    match note.why() {
        BassReason::Height(vowel) => format!("height: {}", height_words(vowel)),
        BassReason::Passing { from, toward } if from == toward => {
            format!("passing: between two {}s, the step above", from.get())
        }
        BassReason::Passing { from, toward } => format!(
            "passing: a step from {} towards {}, held one short of it",
            from.get(),
            toward.get()
        ),
        BassReason::Approach { onto } => format!(
            "approach: a semitone below the next stressed note, {}",
            onto.get()
        ),
        BassReason::LeadIn { toward } => format!(
            "lead-in: climbing a step at a time into the first stressed note, {}",
            toward.get()
        ),
        BassReason::Tail { from } => format!(
            "tail: stepping down from the last stressed note, {}",
            from.get()
        ),
        BassReason::ChordTone { vowel, chord } => format!(
            "chord tone: {} of {}",
            height_words(vowel),
            chord_words(chord, harmonised)
        ),
        BassReason::Step {
            vowel,
            chord,
            direction,
            next,
        } => {
            let way = match direction {
                Direction::Up => "up",
                Direction::Down => "down",
            };
            let because = match (next, direction) {
                (Some(next), Direction::Up) => {
                    format!("the next foot's {} is higher", next.symbol())
                }
                (Some(next), Direction::Down) => {
                    format!("the next foot's {} is not higher", next.symbol())
                }
                (None, _) => "no next foot".to_owned(),
            };
            format!(
                "step {way}: {} of {} would repeat the last note; {because}",
                height_words(vowel),
                chord_words(chord, harmonised)
            )
        }
    }
}

/// `V deg 5 (V of deg 1) · cadence: B opens, answered later in its stanza`: which chord, on which
/// degree, and the rule that chose it.
fn describe_chord(chord: Chord) -> String {
    let numeral = chord.function().numeral();
    let root = chord.root().get();
    let of = chord.of().get();
    let chord_text = if of == root {
        format!("{numeral} deg {root}")
    } else {
        format!("{numeral} deg {root} ({numeral} of deg {of})")
    };
    let why = match chord.reason() {
        Reason::Class(class) => {
            format!("rhyme: class {}, on its degree", class.letter())
        }
        Reason::Opens(class) => format!(
            "cadence: {} opens, answered later in its stanza",
            class.letter()
        ),
        Reason::Closes(class) => format!(
            "cadence: {} closes, answering an earlier line of its stanza",
            class.letter()
        ),
        Reason::Unanswered { key } => format!(
            "cadence: no rhyme in its stanza, so IV of the stanza's key, deg {}",
            key.get()
        ),
        Reason::Turnaround { onto } => format!(
            "cadence: the stanza's last bar, turning round onto its first chord, deg {}",
            onto.get()
        ),
    };
    format!("{chord_text} · {why}")
}

/// Why a bar has no chord.
const fn silence_words(why: Silence) -> &'static str {
    match why {
        Silence::Unrhymed => "rhyme: the line rhymes with nothing, so it has no degree",
        Silence::Wordless => "no pronounceable word to carry a chord (I4)",
        Silence::Flourish => "a flourish stanza carries no harmony",
    }
}

/// One row per chord of the bar: its chord, or `no chord · ` and why not — and, in a stanza's last
/// bar under H2, a second row for the turnaround from the bar's half (§4.6).
fn describe_harmony(harmony: Harmony) -> Vec<String> {
    match harmony {
        Harmony::Chord(chord) => vec![describe_chord(chord)],
        Harmony::Rest(rest) => vec![format!("no chord · {}", silence_words(rest.why()))],
        Harmony::Turning { own, turn } => vec![
            own.map_or_else(
                || format!("no chord · {}", silence_words(Silence::Unrhymed)),
                describe_chord,
            ),
            format!("then, from the bar's half, {}", describe_chord(turn)),
        ],
    }
}

/// Every slot of the arrangement a source bar sounds in.
fn sounds_of(score: &Score, bar: usize) -> Vec<(usize, Cause)> {
    score
        .arrangement()
        .slots()
        .iter()
        .enumerate()
        .flat_map(|(index, slot)| {
            slot.placements()
                .iter()
                .filter(move |placement| placement.bar().get() == bar)
                .map(move |placement| (index, placement.cause()))
        })
        .collect()
}

/// What a source bar keeps on each pass of an erosion, if it erodes anywhere in the piece: the
/// spans of its surviving syllables, pass by pass — the arrangement's own rule, applied to the
/// line's own events, so the view and the music cannot disagree.
fn erosion_of(score: &Score, bar: usize) -> Vec<Vec<Span>> {
    let erodes = score.arrangement().slots().iter().any(|slot| {
        slot.placements().iter().any(|placement| {
            placement.bar().get() == bar && matches!(placement.cause(), Cause::Eroding { .. })
        })
    });
    if !erodes {
        return Vec::new();
    }
    let events: Vec<&Event> = score
        .events()
        .iter()
        .filter(|event| event.bar().get() == bar)
        .collect();
    (1..=crate::arrange::EROSION_PASSES)
        .map(|pass| {
            let mut kept = events.clone();
            crate::arrange::erode(&mut kept, pass);
            kept.iter().map(|event| event.span()).collect()
        })
        .collect()
}

/// `pass 1 keeps 6 of 8 · pass 2 keeps 4 · pass 3 keeps 2: stop, Death`: how a line thins out,
/// and the words that are the last of it.
fn describe_erosion(source: &str, line: &AnnotatedLine) -> String {
    let total = line
        .tokens()
        .iter()
        .map(|token| token.syllables().len())
        .sum::<usize>();
    let mut parts = Vec::new();
    for (index, kept) in line.erosion().iter().enumerate() {
        let pass = index + 1;
        parts.push(if index == 0 {
            format!("pass {pass} keeps {} of {total}", kept.len())
        } else {
            format!("pass {pass} keeps {}", kept.len())
        });
    }
    let mut last: Vec<&str> = Vec::new();
    let mut previous = None;
    for span in line.erosion().last().into_iter().flatten() {
        if previous != Some(*span) {
            last.push(span.text(source).unwrap_or("<lost>"));
        }
        previous = Some(*span);
    }
    format!(
        "{}: {} (unstressed first, rhyme words last, §4.6)",
        parts.join(" · "),
        last.join(", ")
    )
}

/// Every beat of a source bar the straight stack fitted to a host, by slot ([`Score::placed_in`]).
fn straightened_of(score: &Score, bar: usize) -> Vec<(usize, Straightened)> {
    score
        .arrangement()
        .slots()
        .iter()
        .enumerate()
        .flat_map(|(index, slot)| {
            slot.placements()
                .iter()
                .filter(move |placement| placement.bar().get() == bar)
                .flat_map(move |placement| {
                    score
                        .placed_in(slot, *placement)
                        .straightened()
                        .iter()
                        .map(move |fitted| (index, *fitted))
                        .collect::<Vec<_>>()
                })
        })
        .collect()
}

/// `3 into 2 at 1.0 6.0 · 2 into 3 at 4.1`: each fitting, with the slot and beat it happened in.
fn describe_straightened(straightened: &[(usize, Straightened)]) -> String {
    let mut by_kind: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
    for (slot, fitted) in straightened {
        by_kind
            .entry((fitted.own(), fitted.host()))
            .or_default()
            .push(format!("{slot}.{}", fitted.beat()));
    }
    let kinds: Vec<String> = by_kind
        .into_iter()
        .map(|((own, host), at)| format!("{own} into {host} at {}", at.join(" ")))
        .collect();
    format!(
        "{} — fitted to the bar's line (slot.beat)",
        kinds.join(" · ")
    )
}

/// Every slot a source bar sounds in where its bass sounds too ([`Score::sounds_bass`]).
fn bass_sounds_of(score: &Score, bar: usize) -> Vec<(usize, Cause)> {
    score
        .arrangement()
        .slots()
        .iter()
        .enumerate()
        .flat_map(|(index, slot)| {
            slot.placements()
                .iter()
                .filter(move |placement| {
                    placement.bar().get() == bar && score.sounds_bass(slot, placement)
                })
                .map(move |placement| (index, placement.cause()))
        })
        .collect()
}

/// `stated 0 · looping 2-3 5 · arriving 4(1) 6(2) · calling 7-12`: each cause with the bars it
/// accounts for, runs of consecutive bars collapsed, and a transformation's pass in brackets.
fn describe_sounds(sounds: &[(usize, Cause)]) -> String {
    // Each cause's name, and whether a cause is that one — with its pass, where it has one.
    type Select = fn(Cause) -> Option<Option<usize>>;
    let kinds: [(&str, Select); 10] = [
        ("stated", |c| matches!(c, Cause::Stated).then_some(None)),
        ("overruling", |c| {
            matches!(c, Cause::Overruling).then_some(None)
        }),
        ("arriving", |c| match c {
            Cause::Arriving { pass } => Some(Some(pass)),
            _ => None,
        }),
        ("yielding", |c| match c {
            Cause::Yielding { pass } => Some(Some(pass)),
            _ => None,
        }),
        ("eroding", |c| match c {
            Cause::Eroding { pass } => Some(Some(pass)),
            _ => None,
        }),
        ("looping", |c| matches!(c, Cause::Looping).then_some(None)),
        ("calling", |c| matches!(c, Cause::Calling).then_some(None)),
        ("holding", |c| matches!(c, Cause::Holding).then_some(None)),
        ("refraining", |c| {
            matches!(c, Cause::Refraining).then_some(None)
        }),
        ("repeating", |c| {
            matches!(c, Cause::Repeating).then_some(None)
        }),
    ];
    let mut parts = Vec::new();
    for (name, select) in kinds {
        let picked: Vec<(usize, Option<usize>)> = sounds
            .iter()
            .filter_map(|&(slot, cause)| select(cause).map(|pass| (slot, pass)))
            .collect();
        if picked.is_empty() {
            continue;
        }
        let mut words: Vec<String> = Vec::new();
        let mut run: Option<(usize, usize)> = None;
        let flush = |run: Option<(usize, usize)>, words: &mut Vec<String>| {
            if let Some((first, last)) = run {
                words.push(if first == last {
                    first.to_string()
                } else {
                    format!("{first}-{last}")
                });
            }
        };
        for (slot, pass) in picked {
            if let Some(pass) = pass {
                flush(run.take(), &mut words);
                words.push(format!("{slot}({pass})"));
                continue;
            }
            run = match run {
                Some((first, last)) if last + 1 == slot => Some((first, slot)),
                other => {
                    flush(other, &mut words);
                    Some((slot, slot))
                }
            };
        }
        flush(run, &mut words);
        parts.push(format!("{name} {}", words.join(" ")));
    }
    parts.join(" · ")
}

/// The pitch column of a token's `nth` syllable's row.
fn pitch_cell(token: &Annotation, nth: usize, walking: bool) -> String {
    let first = nth == 0;
    if walking {
        // Walking, a token can carry a different bass note on every syllable, and a rhyme
        // word both its melody note and a bass note: each row shows what sounds on its own
        // syllable, as the line's bass rows do (`deg 0~ bass 0@4`).
        let melody = token.pitch().filter(|_| first).map(|pitch| {
            if pitch.detune().is_exact() {
                format!("deg {}", pitch.degree().get())
            } else {
                format!("deg {}~", pitch.degree().get())
            }
        });
        let bass = token
            .basses()
            .get(nth)
            .copied()
            .flatten()
            .map(|note| format!("bass {}", tone(note).trim_start_matches("step ")));
        match (melody, bass) {
            (Some(melody), Some(bass)) => format!("{melody} {bass}"),
            (Some(one), None) | (None, Some(one)) => one,
            (None, None) if first => "—".to_owned(),
            (None, None) => String::new(),
        }
    } else if first {
        match (token.pitch(), token.bass()) {
            (Some(pitch), _) if pitch.detune().is_exact() => {
                format!("deg {}", pitch.degree().get())
            }
            (Some(pitch), _) => format!("deg {}~", pitch.degree().get()),
            // The height bass: `bass 2 AH`, its degree and the vowel that chose it.
            (None, Some(note)) => match note.why() {
                BassReason::Height(vowel) => {
                    format!("bass {} {}", note.step().get(), vowel.symbol())
                }
                _ => format!("bass {}", tone(note).trim_start_matches("step ")),
            },
            (None, None) => "—".to_owned(),
        }
    } else {
        String::new()
    }
}

/// One token's row, or several when it sounds more than one syllable.
fn render_token(
    out: &mut String,
    source: &str,
    token: &Annotation,
    walking: bool,
    articulated: bool,
    anchored: bool,
) {
    let text = token.span().text(source).unwrap_or("<lost>");
    if token.syllables().is_empty() {
        let _ = writeln!(
            out,
            "    {text:<14} {:<10} {:<7} {:<9} {:<9} {:<7} {:<6} {}",
            "—",
            "—",
            "—",
            "—",
            "—",
            "—",
            if token.is_silent() {
                "no pronunciation (phoneme §7.2)"
            } else {
                "not in the score — attribution lost"
            }
        );
        return;
    }

    for (nth, syllable) in token.syllables().iter().enumerate() {
        let (bar, beat, index, of) = syllable.position();
        // Everything a token contributes is a property of the word, so it is written once, on the
        // row of its first syllable — repeating it would read as several different pitches.
        let first = nth == 0;
        let pitch = pitch_cell(token, nth, walking);
        let modulation = if first {
            token.modulation().map_or_else(
                || "—".to_owned(),
                |family| format!("f{}+{}", family.voice().get(), family.depth().get()),
            )
        } else {
            String::new()
        };
        let sharp = if first {
            token
                .sharpness()
                .map_or_else(|| "—".to_owned(), |value| format!("{:.2}", value.as_f64()))
        } else {
            String::new()
        };
        let bank = if first {
            token
                .bank()
                .map_or_else(|| "—".to_owned(), |phoneme| phoneme.symbol().to_owned())
        } else {
            String::new()
        };

        let mut from = vec!["meter"];
        if first {
            if let Some(pitch) = token.pitch() {
                match pitch.source() {
                    crate::voice::PitchSource::Rhyme(_) => from.push("rhyme"),
                    crate::voice::PitchSource::Assonance(_) => from.push("assonance"),
                }
            }
            match token.bass() {
                Some(_) if walking => from.push("bass"),
                // The height bass was always credited to the role pass: it is the foundation's.
                Some(_) if token.pitch().is_none() => from.push("role"),
                _ => {}
            }
            if token.modulation().is_some() {
                from.push("sound");
            }
            if token.sharpness().is_some() {
                from.push("surprisal");
            }
            if let Some(anchor) = token.anchor() {
                if anchored {
                    from.push(match anchor {
                        crate::voice::Anchor::Downbeat => "repetition,anchor(downbeat)",
                        crate::voice::Anchor::Cadence => "repetition,anchor(cadence)",
                    });
                } else {
                    from.push("repetition");
                }
            }
            if token.provenance() == Some(Provenance::LetterToSound) {
                from.push("phoneme(guessed)");
            }
        }
        // Per syllable, not per token: this gain is the light function words' (§4.2).
        if syllable.is_light() {
            from.push("function-words");
        }
        if articulated {
            from.push(syllable.drum().annotation());
        }

        let _ = writeln!(
            out,
            "    {:<14} {:<10} {:<7} {:<9} {:<9} {:<7} {:<6} {}",
            if first { text } else { "" },
            format!("{bar}.{beat}.{index}/{of}"),
            format!("{:.2}", syllable.velocity().as_f64()),
            pitch,
            modulation,
            sharp,
            bank,
            from.join(",")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Pass, describe_sounds, render, run};
    use crate::arrange::Cause;

    #[test]
    fn where_a_line_sounds_reads_as_runs_per_cause() {
        // A line stated at bar 0, looping at 2, 3 and 5, then yielding on the first pass of a
        // transformation at bar 7.
        let sounds = [
            (0, Cause::Stated),
            (2, Cause::Looping),
            (3, Cause::Looping),
            (5, Cause::Looping),
            (7, Cause::Yielding { pass: 1 }),
            (8, Cause::Calling),
            (9, Cause::Calling),
        ];
        assert_eq!(
            describe_sounds(&sounds),
            "stated 0 · yielding 7(1) · looping 2-3 5 · calling 8-9"
        );
    }

    use crate::lower::{self, Analyses};
    use crate::phoneme::arpabet::ARPABET;
    use crate::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };
    use crate::surprisal::{Lexicon, Rarity};
    use crate::{Poem, meter, repetition, rhyme, role, sound, surprisal};

    /// Spelling is pronunciation. A word starting `G` is "not in the dictionary".
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
            let syllables = syllabify(&phones).unwrap();
            if word.starts_with('G') {
                return Ok(Pronunciation::guessed(syllables));
            }
            Ok(Pronunciation::from_dictionary(syllables))
        }
    }

    struct ByLength;

    impl Lexicon for ByLength {
        // No closed class: every word keeps its dictionary stress under either variant.
        fn is_function_word(&self, _: &str) -> bool {
            false
        }

        fn rarity(&self, word: &str) -> Rarity {
            Rarity::band(u8::try_from(word.chars().count()).unwrap_or(u8::MAX))
        }
    }

    fn view(source: &str) -> (String, super::Annotated) {
        view_with(source, &crate::mapping::Mappings::all())
    }

    fn view_with(source: &str, mappings: &crate::mapping::Mappings) -> (String, super::Annotated) {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &ByLength);
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
        let annotated = run(&poem, &score, &roles);
        (render(&poem, &annotated), annotated)
    }

    /// A poem exercising every mapping the project has: a refrain, a family, an alliteration, two
    /// rhyme classes, a guessed word and an unpronounceable one.
    const RICH: &str = "bAn bAd bot\n\
                        teA bAn\n\
                        bAn bAd bot\n\
                        \n\
                        do rot GeA zz\n\
                        teA bAnd\n";

    #[test]
    fn every_token_of_the_poem_appears_in_the_view() {
        // The view's whole job. A token missing here is a token whose fate nobody can check, which
        // is invariant I4 failing exactly as quietly as `SPEC.md` §8 warns.
        let source = RICH;
        let (text, annotated) = view(source);
        let poem = Poem::parse(source.to_owned()).unwrap();
        let words: Vec<&str> = poem
            .stanzas()
            .iter()
            .flat_map(crate::parse::Stanza::lines)
            .flat_map(crate::parse::Line::words)
            .filter_map(|token| token.span().text(source))
            .collect();
        assert!(!words.is_empty());
        for word in &words {
            assert!(text.contains(*word), "{word} is missing from the view");
        }
        assert_eq!(
            annotated
                .lines()
                .flat_map(super::AnnotatedLine::tokens)
                .count(),
            words.len(),
            "one annotation per word, no more and no fewer"
        );
    }

    #[test]
    fn every_mapping_the_project_has_reaches_the_view() {
        // `CLAUDE.md` §8: "Every new mapping adds its contribution to this view in the same change
        // that adds the mapping." Twice now a row of §4.3 has been computed, carried through the
        // IR, and dropped one layer later — both times because nothing asserted it was visible.
        let (text, _) = view(RICH);
        for (mapping, needle) in [
            ("§4.1 role", "foundation"),
            ("§4.2 beat position", "0.0.0/"),
            ("§4.2 stress", "gain"),
            ("§4.3 refrain", "refrain of voice"),
            ("§4.3 modulation", "f0+"),
            ("§4.3 alliteration", "bank"),
            ("§4.4 sharpness", "sharp"),
            ("§4.5 pitch", "deg "),
            ("§7.2 guessed", "phoneme(guessed)"),
            ("§7.2 unpronounceable", "no pronunciation"),
            ("I2 seed", "seed "),
            ("§4.2 pulse", "pulse steady · every bar lasts its beats"),
        ] {
            assert!(
                text.contains(needle),
                "{mapping} does not reach the debug view (looked for {needle:?}):\n{text}"
            );
        }
    }

    #[test]
    fn the_view_names_the_pass_that_decided_each_value() {
        // The half that turns "that sounds wrong" into "that pass is wrong".
        let (text, _) = view(RICH);
        // The harmony pass is listed only when the harmony layer is on, and the bass pass only
        // when the bass walks; see their own tests.
        for pass in Pass::ALL
            .into_iter()
            .filter(|pass| !matches!(pass, Pass::Harmony | Pass::Bass))
        {
            assert!(text.contains(pass.name()), "{} missing", pass.name());
            assert!(text.contains(pass.section()), "{} missing", pass.section());
        }
        // ...and a pitched token says the rhyme pass put it there.
        assert!(text.contains("meter,rhyme"), "{text}");
    }

    /// Choosing the harmony's `variant`.
    fn harmony(variant: crate::mapping::Variant) -> crate::mapping::Mappings {
        crate::mapping::Mappings::all()
            .with(crate::mapping::Mapping::Harmony.choose(variant).unwrap())
    }

    /// Choosing the bass's and the harmony's variants.
    fn bass(
        bass: crate::mapping::Variant,
        harmony: crate::mapping::Variant,
    ) -> crate::mapping::Mappings {
        crate::mapping::Mappings::all()
            .with(crate::mapping::Mapping::Bass.choose(bass).unwrap())
            .with(crate::mapping::Mapping::Harmony.choose(harmony).unwrap())
    }

    /// Each bass row of bar `line` (counting lines from 0): its tone and why, as the view writes
    /// them.
    fn bass_rows(text: &str, line: usize) -> Vec<(String, String)> {
        let block = text
            .split("\n  bass ")
            .filter(|block| block.starts_with("syllables") || block.starts_with("feet"))
            .nth(line)
            .expect("a bass block for the line");
        // `    {word:<14} {position:<10} {tone:<9} {why}`: the tone at column 30, the why at 40.
        block
            .lines()
            .skip(1)
            .filter(|row| {
                let row = row.trim_start();
                !row.starts_with("walks in") && !row.starts_with("silent in")
            })
            .take_while(|row| !row.trim_start().starts_with("token"))
            .map(|row| (row[30..39].trim_end().to_owned(), row[40..].to_owned()))
            .collect()
    }

    #[test]
    fn the_view_says_why_each_note_of_a_syllable_walk_is_what_it_is() {
        use crate::mapping::Variant;
        // W1: `bo de de bA` — OW mid (the third, 2), a passing tone towards AA's root (1), the
        // approach a semitone below the root (0-), then the root.
        let source = "bo de de bA\nso te te sA\n";
        let (text, annotated) = view_with(source, &bass(Variant::Syllables, Variant::Off));
        assert_eq!(annotated.bass(), Variant::Syllables);
        let row = |tone: &str, why: &str| (tone.to_owned(), why.to_owned());
        assert_eq!(
            bass_rows(&text, 0),
            vec![
                row("step 2", "height: OW is mid, so the third"),
                row(
                    "step 1",
                    "passing: a step from 2 towards 0, held one short of it"
                ),
                row(
                    "step 0-",
                    "approach: a semitone below the next stressed note, 0"
                ),
                row("step 0", "height: AA is open, so the root"),
            ]
        );
        assert!(
            text.contains("\n  bass        §4.6\n"),
            "the legend lists it:\n{text}"
        );
        assert!(
            text.contains("meter,rhyme,bass"),
            "a rhyme word carries both:\n{text}"
        );
        // The height bass, the default, has no block and no legend entry: the view is as it was.
        let (plain, _) = view(source);
        assert!(!plain.contains("\n  bass "), "{plain}");
    }

    /// The pitch column of each token row in bar `line`'s table, by the row's beat position —
    /// for a line of one-syllable words, whose every row is a token's first and so has all four
    /// one-word columns after the pitch (modulation, sharp, bank, from).
    fn pitch_cells(text: &str, line: usize) -> Vec<(String, String)> {
        let table = text
            .split("\n    token ")
            .nth(line + 1)
            .expect("a token table for the line");
        // `    {token:<14} {beat:<10} {gain:<7} {pitch:<9} …`: the beat at column 19, the pitch
        // from 38 — wider than its column when a rhyme word carries both notes.
        table
            .lines()
            .skip(1)
            .take_while(|row| row.starts_with("    ") && row.len() > 38)
            .map(|row| {
                let words: Vec<&str> = row[38..].split_whitespace().collect();
                let cell = words[..words.len() - 4].join(" ");
                (row[19..29].trim_end().to_owned(), cell)
            })
            .collect()
    }

    #[test]
    fn the_pitch_column_shows_what_each_syllable_of_a_walk_sounds() {
        use crate::mapping::Variant;
        // W1 over `bo de de bA`: each syllable's own bass note on its own row — 2, 1, a semitone
        // below 0, then 0 — and the rhyme word `bA` both its melody degree (the first class, 0)
        // and its bass note, as the bass rows above list them.
        let (text, _) = view_with(
            "bo de de bA\nso te te sA\n",
            &bass(Variant::Syllables, Variant::Off),
        );
        let cell = |beat: &str, pitch: &str| (beat.to_owned(), pitch.to_owned());
        assert_eq!(
            pitch_cells(&text, 0),
            vec![
                cell("0.0.0/2", "bass 2"),
                cell("0.0.1/2", "bass 1"),
                cell("0.1.0/2", "bass 0-"),
                cell("0.1.1/2", "deg 0 bass 0"),
            ],
            "{text}"
        );
    }

    #[test]
    fn the_view_says_where_a_held_line_walks_and_where_it_is_silent() {
        use crate::mapping::Variant;
        // ABAB: the A line sounds stated in slot 0, held under the B line in slot 1, and again in
        // the outro's slot 4. One walk a bar (`Score::sounds_bass`): it walks where it is the
        // bar's foundation line and not where it is held.
        let (text, annotated) = view_with(
            "de bA de bA\nde bo de bo\nse sA se sA\nse so se so\n",
            &bass(Variant::Feet, Variant::Off),
        );
        let first = annotated.lines().next().unwrap();
        assert_eq!(
            first.walks(),
            &[(0, Cause::Stated), (4, Cause::Repeating)][..]
        );
        assert!(
            text.contains("    walks in stated 0 · repeating 4\n"),
            "{text}"
        );
        assert!(
            text.contains(
                "    silent in holding 1 — one walk a bar, the bar's foundation line's\n"
            ),
            "{text}"
        );
        // The height bass sounds wherever its line does.
        let (_, plain) = view("de bA de bA\nde bo de bo\nse sA se sA\nse so se so\n");
        let first = plain.lines().next().unwrap();
        assert_eq!(first.walks(), first.sounds());
    }

    #[test]
    fn the_view_says_which_chord_tone_each_foot_lands_on() {
        use crate::mapping::Variant;
        // W2 under H1. Lines end O, A, O, A, so O is degree 0 and A degree 1; bar 0 is I on 0,
        // bar 1 I on 1. Bar 1's feet: `de bo` (OW, mid: the third) then `de bA` (AA, open: the
        // root), counted from degree 1.
        let source = "de bA de bo\nde bo de bA\nse sA se so\nse so se sA\n";
        let (text, _) = view_with(source, &bass(Variant::Feet, Variant::Rhyme));
        let row = |tone: &str, why: &str| (tone.to_owned(), why.to_owned());
        assert_eq!(
            bass_rows(&text, 1),
            vec![
                row("2@1", "chord tone: OW is mid, so the third of I deg 1"),
                row("0@1", "chord tone: AA is open, so the root of I deg 1"),
            ]
        );
        // Harmony off, the same feet stand on the scale.
        let (text, _) = view_with(source, &bass(Variant::Feet, Variant::Off));
        assert_eq!(
            bass_rows(&text, 1),
            vec![
                row(
                    "step 2",
                    "chord tone: OW is mid, so the third of the scale (harmony off)"
                ),
                row(
                    "step 0",
                    "chord tone: AA is open, so the root of the scale (harmony off)"
                ),
            ]
        );
        // A repeated tone steps, and says which vowel chose the direction: `dA dA` repeats the
        // root with no next foot, so down.
        let (text, _) = view_with(
            "de bA de bA\nse sA se sA\n",
            &bass(Variant::Feet, Variant::Off),
        );
        assert_eq!(
            bass_rows(&text, 0)[1],
            row(
                "step -1",
                "step down: AA is open, so the root of the scale (harmony off) would repeat the \
                 last note; no next foot"
            )
        );
    }

    #[test]
    fn the_view_says_which_chord_each_bar_has_and_why() {
        // ABAB of one-syllable stressed words, every line two of them: a foundation stanza. A is
        // degree 0 and B degree 1. Under cadence: A opens (V of 0 = deg 4), B opens (V of 1 =
        // deg 5), A closes (I on 0), and the last bar closes B (I on 1) and, from its half, turns
        // round onto the first chord (V of 4) — two rows for the one line.
        let source = "dA bA\nro bo\nsA tA\nno do\n";
        let (text, annotated) = view_with(source, &harmony(crate::mapping::Variant::Cadence));
        let rows: Vec<&str> = text
            .lines()
            .filter_map(|line| line.strip_prefix("  harmony "))
            .filter(|row| !row.starts_with(' '))
            .collect();
        assert_eq!(
            rows,
            vec![
                "V deg 4 (V of deg 0) · cadence: A opens, answered later in its stanza",
                "V deg 5 (V of deg 1) · cadence: B opens, answered later in its stanza",
                "I deg 0 · cadence: A closes, answering an earlier line of its stanza",
                "I deg 1 · cadence: B closes, answering an earlier line of its stanza",
                "then, from the bar's half, V deg 8 (V of deg 4) · cadence: the stanza's last \
                 bar, turning round onto its first chord, deg 4",
            ]
        );
        assert!(text.contains("  harmony     §4.6"), "{text}");
        assert_eq!(
            annotated.lines().filter(|l| l.harmony().is_some()).count(),
            4
        );
    }

    #[test]
    fn the_view_says_a_turning_bar_rests_before_its_turnaround() {
        // Under `both`, `ro bAn` (AA N) rhymes with nothing: neither H2 nor H1 gives it a chord. As
        // the stanza's last bar, it rests for its first half and from its half turns round onto
        // the stanza's first chord — `dA bA` opens A, so H2's V of 0, deg 4 — with V of 4, deg 8.
        let source = "dA bA\nsA tA\nro bAn\n";
        let (text, _) = view_with(source, &harmony(crate::mapping::Variant::Both));
        assert!(
            text.contains(
                "  harmony no chord · rhyme: the line rhymes with nothing, so it has no degree\n  \
                 harmony then, from the bar's half, V deg 8 (V of deg 4) · cadence: the stanza's \
                 last bar, turning round onto its first chord, deg 4\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn the_view_says_why_a_bar_has_no_chord() {
        // H1: `ro bAn` ends on AA N, which nothing else ends on — a class of one, so no chord.
        let source = "dA bA\nro bAn\nsA tA\n";
        let (text, _) = view_with(source, &harmony(crate::mapping::Variant::Rhyme));
        assert!(
            text.contains(
                "harmony no chord · rhyme: the line rhymes with nothing, so it has no degree"
            ),
            "{text}"
        );
        assert!(
            text.contains("harmony I deg 0 · rhyme: class A, on its degree"),
            "{text}"
        );
    }

    #[test]
    fn without_the_harmony_the_view_is_as_it_was() {
        // Off is the default: no harmony row and no harmony in the legend.
        let (text, annotated) = view("dA bA\nro bo\nsA tA\nno do\n");
        assert!(!text.contains("harmony"), "{text}");
        assert!(annotated.lines().all(|line| line.harmony().is_none()));
    }

    #[test]
    fn a_word_nobody_can_pronounce_is_shown_as_silent_not_as_absent() {
        // `CLAUDE.md` §6: a fallback must be visible. The two ways a token can have no events are
        // completely different problems, and the view must not collapse them.
        let (text, annotated) = view(RICH);
        let silent: Vec<_> = annotated.silent().collect();
        assert_eq!(silent.len(), 1, "{silent:?}");
        assert!(silent[0].is_silent(), "it is silent, not lost");
        assert!(text.contains("no pronunciation"), "{text}");
        assert!(
            !text.contains("attribution lost"),
            "nothing in this poem is unaccounted for:\n{text}"
        );
    }

    #[test]
    fn every_syllable_of_a_line_lands_somewhere_different() {
        // The view's beat column is `SPEC.md` §8's "beat position", and it is the one column whose
        // whole value is that the numbers differ. Printing a constant satisfied every other
        // assertion in this module.
        let (text, annotated) = view("beAn teA bot\n");
        let positions: Vec<_> = annotated
            .lines()
            .flat_map(super::AnnotatedLine::tokens)
            .flat_map(super::Annotation::syllables)
            .map(super::Syllable::position)
            .collect();
        assert!(positions.len() > 3, "{positions:?}");
        let distinct: std::collections::BTreeSet<_> = positions.iter().collect();
        assert_eq!(
            distinct.len(),
            positions.len(),
            "two syllables cannot sound in the same place: {positions:?}"
        );
        // ...and the rendered column shows that, rather than one repeated value.
        let printed: std::collections::BTreeSet<&str> = text
            .lines()
            .filter_map(|line| line.split_whitespace().find(|token| token.contains('/')))
            .collect();
        assert!(printed.len() > 3, "the beat column is constant: {text}");
    }

    #[test]
    fn a_tokens_syllables_are_listed_in_playing_order() {
        let (_, annotated) = view("beAn teA\n");
        let first = &annotated.lines().next().unwrap().tokens()[0];
        assert_eq!(first.syllables().len(), 2, "beAn is two syllables");
        let positions: Vec<_> = first
            .syllables()
            .iter()
            .map(super::Syllable::position)
            .collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        assert_eq!(positions, sorted);
    }

    #[test]
    fn a_line_carries_its_bar_layer_voice_and_seed() {
        // The four line-grain facts §8 asks for by name.
        let (_, annotated) = view(RICH);
        let line = annotated.lines().next().unwrap();
        assert_eq!(line.bar(), Some(0));
        assert!(line.layer().is_some());
        assert!(line.voice().is_some());
        assert!(line.seed().is_some(), "I2's seed is one of §8's five");
    }

    #[test]
    fn a_refrain_shares_its_voice_and_the_view_says_so() {
        let (text, annotated) = view(RICH);
        let voices: Vec<_> = annotated
            .lines()
            .filter_map(super::AnnotatedLine::voice)
            .collect();
        let unique: std::collections::BTreeSet<_> = voices.iter().collect();
        assert!(
            unique.len() < voices.len(),
            "the fixture must contain a refrain: {voices:?}"
        );
        assert!(text.contains("refrain of voice"), "{text}");
    }

    #[test]
    fn the_view_is_deterministic() {
        assert_eq!(view(RICH).0, view(RICH).0);
    }

    #[test]
    fn an_empty_poem_renders_without_panicking() {
        let (text, annotated) = view("");
        assert!(annotated.lines().next().is_none());
        assert!(text.contains("passes"), "the legend is always there");
    }
}
