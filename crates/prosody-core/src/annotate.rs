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
use crate::harmony::{Harmony, Reason, Silence};
use crate::meter::FootKind;
use crate::parse::{Poem, Token};
use crate::phoneme::{Phoneme, Provenance};
use crate::role::{Layer, LineBasis, StanzaBasis};
use crate::seed::Seed;
use crate::span::Span;
use crate::voice::{Event, Family, Pitch, Score, Velocity};

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
        }
    }

    /// The `SPEC.md` section that specified it.
    #[must_use]
    pub const fn section(self) -> &'static str {
        match self {
            Self::Role | Self::Arrange => "§4.1",
            Self::Harmony => "§4.6",
            Self::Meter => "§4.2",
            Self::Repetition | Self::Sound => "§4.3",
            Self::Surprisal => "§4.4",
            Self::Rhyme => "§4.5",
            Self::Phoneme => "§7.2",
            Self::Seed => "I2",
        }
    }

    /// Every pass, for the legend. The legend lists [`Pass::Harmony`] only when the poem has a
    /// harmony, so a view without one reads as it did before the layer existed.
    pub const ALL: [Self; 10] = [
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
}

/// What became of one token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotation {
    span: Span,
    syllables: Vec<Syllable>,
    pitch: Option<Pitch>,
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
    harmony: Option<Harmony>,
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

    /// Its bar's chord, or why it has none ([`Pass::Harmony`]) — `None` when the harmony layer
    /// is off.
    #[must_use]
    pub const fn harmony(&self) -> Option<Harmony> {
        self.harmony
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
}

impl Annotated {
    /// The poem's stanzas, each a list of lines.
    #[must_use]
    pub fn stanzas(&self) -> &[Vec<AnnotatedLine>] {
        &self.stanzas
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
                        harmony: score.harmony_of(index),
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
        events
            .iter()
            .map(|event| Syllable {
                bar: event.bar().get(),
                beat: event.beat().get(),
                index: event.subdivision().index(),
                of: event.subdivision().of(),
                foot: event.foot(),
                velocity: event.velocity(),
            })
            .collect()
    });
    let first = events.and_then(|events| events.first().copied());

    Annotation {
        span: token.span(),
        syllables,
        pitch: events.and_then(|events| events.iter().find_map(|event| event.pitch())),
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
    for pass in Pass::ALL {
        if pass == Pass::Harmony && !harmonised {
            continue;
        }
        let _ = writeln!(out, "  {:<11} {}", pass.name(), pass.section());
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
            if let Some(harmony) = line.harmony() {
                let _ = writeln!(out, "  harmony {}", describe_harmony(harmony));
            }
            let _ = writeln!(
                out,
                "    {:<14} {:<10} {:<7} {:<9} {:<9} {:<7} {:<6} from",
                "token", "beat", "gain", "pitch", "modulation", "sharp", "bank"
            );
            for token in line.tokens() {
                render_token(&mut out, source, token);
            }
        }
        let _ = writeln!(out);
    }
    out
}

/// `V deg 5 (V of deg 1) · cadence: B opens, answered later in its stanza`: which chord, on which
/// degree, and the rule that chose it — or `no chord · ` and why not.
fn describe_harmony(harmony: Harmony) -> String {
    match harmony {
        Harmony::Chord(chord) => {
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
        Harmony::Rest(rest) => format!(
            "no chord · {}",
            match rest.why() {
                Silence::Unrhymed => "rhyme: the line rhymes with nothing, so it has no degree",
                Silence::Wordless => "no pronounceable word to carry a chord (I4)",
                Silence::Flourish => "a flourish stanza carries no harmony",
            }
        ),
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

/// `stated 0 · looping 2-3 5 · arriving 4(1) 6(2) · calling 7-12`: each cause with the bars it
/// accounts for, runs of consecutive bars collapsed, and a transformation's pass in brackets.
fn describe_sounds(sounds: &[(usize, Cause)]) -> String {
    // Each cause's name, and whether a cause is that one — with its pass, where it has one.
    type Select = fn(Cause) -> Option<Option<usize>>;
    let kinds: [(&str, Select); 9] = [
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

/// One token's row, or several when it sounds more than one syllable.
fn render_token(out: &mut String, source: &str, token: &Annotation) {
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
        let pitch = if first {
            token.pitch().map_or_else(
                || "—".to_owned(),
                |pitch| match pitch.source() {
                    // The bass: `bass 2 AH`, its degree and the vowel that chose it.
                    crate::voice::PitchSource::Vowel(vowel) => {
                        format!("bass {} {}", pitch.degree().get(), vowel.symbol())
                    }
                    crate::voice::PitchSource::Rhyme(_) if pitch.detune().is_exact() => {
                        format!("deg {}", pitch.degree().get())
                    }
                    crate::voice::PitchSource::Rhyme(_) => {
                        format!("deg {}~", pitch.degree().get())
                    }
                },
            )
        } else {
            String::new()
        };
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
            match token.pitch().map(crate::voice::Pitch::source) {
                Some(crate::voice::PitchSource::Rhyme(_)) => from.push("rhyme"),
                Some(crate::voice::PitchSource::Vowel(_)) => from.push("role"),
                None => {}
            }
            if token.modulation().is_some() {
                from.push("sound");
            }
            if token.sharpness().is_some() {
                from.push("surprisal");
            }
            if token.anchor().is_some() {
                from.push("repetition");
            }
            if token.provenance() == Some(Provenance::LetterToSound) {
                from.push("phoneme(guessed)");
            }
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
        // The harmony pass is listed only when the harmony layer is on; see its own test.
        for pass in Pass::ALL.into_iter().filter(|pass| *pass != Pass::Harmony) {
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

    #[test]
    fn the_view_says_which_chord_each_bar_has_and_why() {
        // ABAB of one-syllable stressed words, every line two of them: a foundation stanza. A is
        // degree 0 and B degree 1. Under cadence: A opens (V of 0 = deg 4), B opens (V of 1 =
        // deg 5), A closes (I on 0), and the last bar turns round onto the first chord (V of 4).
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
                "V deg 8 (V of deg 4) · cadence: the stanza's last bar, turning round onto \
                 its first chord, deg 4",
            ]
        );
        assert!(text.contains("  harmony     §4.6"), "{text}");
        assert_eq!(
            annotated.lines().filter(|l| l.harmony().is_some()).count(),
            4
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
