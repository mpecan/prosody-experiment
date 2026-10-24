//! The harmony layer: a chord for every bar of a foundation stanza, derived from the poem.
//!
//! *Candidate, sound and voice step 5, pending review* — `SPEC.md` §4.6 names "a real chord
//! progression" as the natural second overrule, needing a harmony layer that did not exist;
//! `PLAN.md`'s proposal "the structural half of jazz" gave two ways to derive one, and the user
//! approved both as variants of [`Mapping::Harmony`] to be compared by ear:
//!
//! | Variant | Reads | Gives |
//! |---|---|---|
//! | [`Variant::Rhyme`] (H1) | each line's rhyme class (§4.5) | the chord on that class's degree, I: ABAB → i–ii–i–ii |
//! | [`Variant::Cadence`] (H2) | whether a line's rhyme is *open* (answered later in its stanza — the held lane's test, §4.6) or *closes* one | open → V of its degree (*d*+4); its answer → I on the degree; a line with no rhyme in its stanza → IV of the stanza's key (*k*+3); the stanza's last bar is split — its own chord, then V of the stanza's first chord in its second half, a turnaround |
//! | [`Variant::Both`] | both | H2 where it applies (open and closing lines, the turnaround), H1 in every other bar |
//!
//! # Structure here, colour in the emitter
//!
//! Which bars have a chord, on which degree, with which function, is decided here: that is *when
//! the harmony changes and to what*, which no style may touch (I3). How a chord sounds — which
//! instrument, how many tones, in which register, how loud — is the palette's, in
//! `prosody_emit`, which voices the root through the style's `Voicing`. The core never sees a
//! scale, so a degree here is a number of steps, and `d+4` may run past the scale's length; the
//! emitter folds it back into one octave.
//!
//! # Every chord traces to a word (I4)
//!
//! A chord's span is the *rhyme word* of the line whose bar it is — [`LineRhyme::word`], the last
//! pronounceable word — even when the line rhymes with nothing and H2 gives it IV. A line with no
//! pronounceable word has no rhyme word, so it has no chord: [`Silence::Wordless`].
//!
//! # One chord a bar — two in a stanza's last — and when it sounds
//!
//! A chord is a fact about a *line* of a foundation stanza, so it lives with that line's bar of
//! the [`Score`](crate::voice::Score) and sounds wherever the arrangement plays that line in the
//! foundation layer — stated, looping under a flourish, yielding or arriving in a transformation,
//! repeating in the outro. Never in the call, held or refrain lanes: those are a line sounding
//! *under* another, and a second chord at one instant would be two harmonies at once. The one bar
//! with two chords has them one after the other: a stanza's last bar under H2
//! ([`Harmony::Turning`]), its own chord from the bar's start and the turnaround from its half.
//! [`Score::harmony_in`](crate::voice::Score::harmony_in) is the one place that decides which
//! harmony a slot of the arrangement plays.
//!
//! # The turnaround loops, so it stays inside its stanza (I2)
//!
//! The proposal's turnaround resolved onto the *next* stanza's first chord, which made stanza
//! *n*'s last bar depend on stanza *n*+1's first rhyme: an edit there would change a chord here.
//! A stanza is a loop (§4.6), and its last bar is followed — every pass but the last — by its
//! own first bar, so the turnaround here leads back to the stanza's **own** first chord. Nothing a
//! chord depends on lies outside its stanza except §4.5's poem-wide degree numbering, which the
//! melody already has (`SPEC.md` §4.5, I2's open question), and — under H1 — whether a line's class
//! has a member in *another* stanza, which decides whether it is rhymed at all, exactly as it does
//! for the melody's note.
//!
//! # Choices the proposal left open, made here and recorded in `SPEC.md` §4.6
//!
//! - **A line that both closes and opens** (the middle A of AAA) is open: its class is still
//!   unresolved, so it takes V, and only the last A resolves. AAAA → V V V I.
//! - **The stanza's key**, for IV: the degree of the stanza's first in-stanza rhyme (its first open
//!   line's class); degree 0 when nothing in the stanza rhymes within it.
//! - **The turnaround shares the last bar** (the user's decision, `PLAN.md` "the cadence's
//!   ending"): the bar's own chord — I when the line closes a rhyme, IV when it has none in its
//!   stanza, H1's chord or a rest under `both` — from the bar's start, then V of the stanza's
//!   first chord from the bar's half, leading into the loop's first bar. So ABAB, AABB and ABBA
//!   land on I before they turn round. A one-line stanza has no turnaround — its last bar is its
//!   first — and neither does a last line with no pronounceable word, or a stanza whose earlier
//!   lines have no chord to lead back to.
//! - **The turnaround traces to the last line's rhyme word** — the line whose bar it shares — not
//!   to the first line's, whose chord only gives it its root ([`Reason::Turnaround`]'s `onto`).
//!   Every chord of a bar then traces to the line that bar plays, so an edit to the first line
//!   moves the turnaround's root and never what it is attributed to (I4).
//! - **A flourish stanza has no harmony.** The proposal gives chords to foundation stanzas only;
//!   each flourish line is recorded as a [`Rest`] so the debug view can say why.

use std::collections::BTreeMap;

use crate::mapping::{Mapping, Mappings, Variant};
use crate::rhyme::{LineRhyme, PoemRhyme, RhymeClassId};
use crate::role::{Layer, PoemRoles, StanzaRole};
use crate::span::Span;
use crate::voice::{Bar, ScaleDegree};

/// What a chord does in its key: the three functions H2 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Function {
    /// I: the chord on the degree itself — rest, resolution.
    Tonic,
    /// IV: the chord a fourth above — away from home without pulling back to it.
    Subdominant,
    /// V: the chord a fifth above — tension that wants the degree it is built on.
    Dominant,
}

impl Function {
    /// Its Roman numeral, relative to the degree it is built on.
    #[must_use]
    pub const fn numeral(self) -> &'static str {
        match self {
            Self::Tonic => "I",
            Self::Subdominant => "IV",
            Self::Dominant => "V",
        }
    }

    /// How many scale steps above the degree it is built on its root lies: 0, 3 and 4 — the
    /// proposal's *d*, *d*+3 and *d*+4.
    #[must_use]
    pub const fn steps(self) -> usize {
        match self {
            Self::Tonic => 0,
            Self::Subdominant => 3,
            Self::Dominant => 4,
        }
    }
}

/// Why a bar has the chord it has — which rule of which variant decided it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// H1: the line's rhyme class, on its degree.
    Class(RhymeClassId),
    /// H2: the line's rhyme is answered later in its stanza, so it is the dominant of its degree.
    Opens(RhymeClassId),
    /// H2: the line answers an earlier line of its stanza, so it is the tonic on its degree.
    Closes(RhymeClassId),
    /// H2: the line has no rhyme within its stanza, so it is the subdominant of the stanza's key.
    Unanswered {
        /// The stanza's key: the degree of its first in-stanza rhyme, or 0.
        key: ScaleDegree,
    },
    /// H2: the second half of the stanza's last bar, the dominant of the stanza's own first chord,
    /// which the loop returns to next. Always the second chord of a [`Harmony::Turning`].
    Turnaround {
        /// The root of the stanza's first chord, which this leads back to.
        onto: ScaleDegree,
    },
}

impl Reason {
    /// Which variant's rule this is: [`Variant::Rhyme`] (H1) or [`Variant::Cadence`] (H2).
    #[must_use]
    pub const fn rule(self) -> Variant {
        match self {
            Self::Class(_) => Variant::Rhyme,
            Self::Opens(_)
            | Self::Closes(_)
            | Self::Unanswered { .. }
            | Self::Turnaround { .. } => Variant::Cadence,
        }
    }
}

/// Where in its bar a chord begins. A chord sounds until the next one, or the bar's end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Onset {
    /// At the bar's start.
    Start,
    /// At the bar's half — the turnaround of a stanza's last bar ([`Harmony::Turning`]).
    Half,
}

/// One bar's chord, or one half of a split bar's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    bar: Bar,
    span: Span,
    root: ScaleDegree,
    function: Function,
    reason: Reason,
}

impl Chord {
    /// The line whose bar it is, by its bar in the score — one bar per line.
    #[must_use]
    pub const fn bar(self) -> Bar {
        self.bar
    }

    /// The rhyme word of that line, which the chord traces to (I4).
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// The degree the chord stands on, counted like a rhyme class's (§4.5) and possibly past the
    /// scale's length: V of degree 5 is degree 9.
    #[must_use]
    pub const fn root(self) -> ScaleDegree {
        self.root
    }

    /// What it does: I, IV or V of [`Chord::of`].
    #[must_use]
    pub const fn function(self) -> Function {
        self.function
    }

    /// The degree the function is relative to: the root, less the function's steps.
    #[must_use]
    pub const fn of(self) -> ScaleDegree {
        ScaleDegree::new(self.root.get().saturating_sub(self.function.steps()))
    }

    /// Which rule decided it.
    #[must_use]
    pub const fn reason(self) -> Reason {
        self.reason
    }

    /// Where in its bar it begins: the turnaround at the bar's half, every other chord at its
    /// start.
    #[must_use]
    pub const fn onset(self) -> Onset {
        match self.reason {
            Reason::Turnaround { .. } => Onset::Half,
            _ => Onset::Start,
        }
    }
}

/// Why a line's bar has no chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Silence {
    /// H1, and H1's half of `both`: the line rhymes with nothing in the poem, so it has no class
    /// degree to stand a chord on — the proposal's "a class of one gets no chord".
    Unrhymed,
    /// The line has no pronounceable word, so no rhyme word for a chord to trace to (I4).
    Wordless,
    /// The line belongs to a flourish stanza, and only foundation stanzas carry harmony.
    Flourish,
}

/// A line whose bar has no chord, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rest {
    bar: Bar,
    line: Span,
    why: Silence,
}

impl Rest {
    /// The line, by its bar in the score.
    #[must_use]
    pub const fn bar(self) -> Bar {
        self.bar
    }

    /// The line's extent.
    #[must_use]
    pub const fn line(self) -> Span {
        self.line
    }

    /// Why it has no chord.
    #[must_use]
    pub const fn why(self) -> Silence {
        self.why
    }
}

/// What the harmony made of one line: a chord, a rest with its reason, or — a stanza's last bar
/// under H2 — its own chord and then the turnaround.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Harmony {
    /// The bar has this chord, for the whole bar.
    Chord(Chord),
    /// The bar has none.
    Rest(Rest),
    /// The stanza's last bar, split (`SPEC.md` §4.6, the user's decision): `own` from the bar's
    /// start, `turn` from its half.
    Turning {
        /// What the bar would have had without the turnaround: a chord, or `None` where the line
        /// rhymes with nothing and H1's half of `both` gives it none ([`Silence::Unrhymed`] — the
        /// only rest a turning bar can have, since a line without a word has no turnaround and a
        /// flourish line no harmony).
        own: Option<Chord>,
        /// The turnaround: V of the stanza's first chord ([`Reason::Turnaround`]).
        turn: Chord,
    },
}

impl Harmony {
    /// The line it is about, by its bar in the score.
    #[must_use]
    pub const fn bar(self) -> Bar {
        match self {
            Self::Chord(chord) | Self::Turning { turn: chord, .. } => chord.bar,
            Self::Rest(rest) => rest.bar,
        }
    }

    /// The chord from the bar's start, if there is one: the bar's chord, or a turning bar's own.
    #[must_use]
    pub const fn chord(self) -> Option<Chord> {
        match self {
            Self::Chord(chord) => Some(chord),
            Self::Turning { own, .. } => own,
            Self::Rest(_) => None,
        }
    }

    /// The turnaround in the bar's second half, if this is a turning bar.
    #[must_use]
    pub const fn turnaround(self) -> Option<Chord> {
        match self {
            Self::Turning { turn, .. } => Some(turn),
            Self::Chord(_) | Self::Rest(_) => None,
        }
    }

    /// The chord sounding from beat `beat` of a bar `beats` long: a turning bar's turnaround from
    /// the bar's half (`2 · beat ≥ beats`), the bar's own chord before it — where the chord lane
    /// changes. What W2's walk lands on (`crate::bass`), given the bar the line is stated in.
    #[must_use]
    pub const fn chord_at(self, beat: usize, beats: usize) -> Option<Chord> {
        match self {
            Self::Turning { turn, .. } if 2 * beat >= beats => Some(turn),
            _ => self.chord(),
        }
    }

    /// Every chord of the bar, in the order they sound: none, one, or a turning bar's two.
    pub fn chords(self) -> impl Iterator<Item = Chord> {
        [self.chord(), self.turnaround()].into_iter().flatten()
    }
}

/// Decides every line's harmony under the [`Mapping::Harmony`] variant chosen — nothing at all
/// when it is off, so a piece without the mapping is the piece it was.
///
/// One entry per line of the poem, in bar order. `degrees` is §4.5's numbering of the classes that
/// sound, the one the melody uses, so a chord on a class's degree is the rhyme note's degree.
pub(crate) fn run(
    rhyme: &PoemRhyme,
    roles: &PoemRoles,
    degrees: &BTreeMap<RhymeClassId, usize>,
    mappings: &Mappings,
) -> Vec<Harmony> {
    let variant = mappings.get(Mapping::Harmony);
    if variant == Variant::Off {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut first_bar = 0;
    for (index, stanza) in rhyme.stanzas().iter().enumerate() {
        let lines = stanza.lines();
        let foundation =
            roles.stanzas().get(index).map(StanzaRole::layer) == Some(Layer::Foundation);
        if foundation {
            out.extend(stanza_harmony(lines, first_bar, degrees, variant));
        } else {
            out.extend(lines.iter().enumerate().map(|(within, line)| {
                Harmony::Rest(Rest {
                    bar: Bar::new(first_bar + within),
                    line: line.line(),
                    why: Silence::Flourish,
                })
            }));
        }
        first_bar += lines.len();
    }
    out
}

/// One chord candidate before the turnaround: its root, function and reason.
type Choice = (usize, Function, Reason);

/// H2's turnaround: V of the stanza's own first chord, the bar the loop plays next — its own
/// stanza, not the next one, so it stays inside the stanza (I2). It shares the last bar with that
/// bar's own chord rather than replacing it (§4.6): the root it returns to is the first earlier
/// line's. A one-line stanza, a last line with no word, or a stanza whose earlier lines have no
/// chord, has none.
fn turnaround(lines: &[LineRhyme], chosen: &[Option<Choice>]) -> Option<Choice> {
    let last = lines.len().checked_sub(1)?;
    lines.get(last).and_then(LineRhyme::word)?;
    let onto = chosen
        .iter()
        .take(last)
        .find_map(|choice| choice.map(|(root, ..)| root))?;
    Some((
        onto + Function::Dominant.steps(),
        Function::Dominant,
        Reason::Turnaround {
            onto: ScaleDegree::new(onto),
        },
    ))
}

/// The harmony of one foundation stanza, whose first line is bar `first_bar`.
fn stanza_harmony(
    lines: &[LineRhyme],
    first_bar: usize,
    degrees: &BTreeMap<RhymeClassId, usize>,
    variant: Variant,
) -> Vec<Harmony> {
    let class_of = |line: &LineRhyme| line.class();
    // The held lane's test (`crate::arrange`): answered later in the stanza, or answering earlier.
    let opens = |at: usize| {
        lines.get(at).and_then(class_of).is_some_and(|class| {
            lines
                .iter()
                .skip(at + 1)
                .any(|later| later.class() == Some(class))
        })
    };
    let closes = |at: usize| {
        lines.get(at).and_then(class_of).is_some_and(|class| {
            lines
                .iter()
                .take(at)
                .any(|earlier| earlier.class() == Some(class))
        })
    };
    // A class's degree, if it sounds: §4.5's numbering, which gives a class of one none.
    let degree = |line: &LineRhyme| {
        line.score()
            .and(line.class())
            .and_then(|class| Some((class, *degrees.get(&class)?)))
    };
    // The stanza's key: its first in-stanza rhyme's degree, or 0 when nothing rhymes within it.
    let key = (0..lines.len())
        .find(|&at| opens(at))
        .and_then(|at| lines.get(at))
        .and_then(degree)
        .map_or(0, |(_, degree)| degree);

    let chosen: Vec<Option<Choice>> = lines
        .iter()
        .enumerate()
        .map(|(at, line)| {
            line.word()?;
            // H2's own rules, where they apply: an open rhyme is a suspension (V), its answer the
            // resolution (I) — `SPEC.md` §4.6's held lane, heard as harmony. Open wins over
            // closing: a line that answers and is answered again leaves its class unresolved.
            let h2 = degree(line).and_then(|(class, d)| {
                if opens(at) {
                    Some((
                        d + Function::Dominant.steps(),
                        Function::Dominant,
                        Reason::Opens(class),
                    ))
                } else if closes(at) {
                    Some((d, Function::Tonic, Reason::Closes(class)))
                } else {
                    None
                }
            });
            // H1: the scheme you can see on the page, heard as changes — the class's own degree.
            let h1 = degree(line).map(|(class, d)| (d, Function::Tonic, Reason::Class(class)));
            match variant {
                Variant::Rhyme => h1,
                // H2's choice for a line with no rhyme in its stanza: away from the key, not back.
                Variant::Cadence => h2.or(Some((
                    key + Function::Subdominant.steps(),
                    Function::Subdominant,
                    Reason::Unanswered {
                        key: ScaleDegree::new(key),
                    },
                ))),
                Variant::Both => h2.or(h1),
                _ => None,
            }
        })
        .collect();

    let turn = if matches!(variant, Variant::Cadence | Variant::Both) {
        turnaround(lines, &chosen)
    } else {
        None
    };
    let last = lines.len().saturating_sub(1);

    lines
        .iter()
        .zip(chosen)
        .enumerate()
        .map(|(at, (line, choice))| {
            let bar = Bar::new(first_bar + at);
            let chord = |(root, function, reason): Choice, span: Span| Chord {
                bar,
                span,
                root: ScaleDegree::new(root),
                function,
                reason,
            };
            match (choice, line.word(), turn.filter(|_| at == last)) {
                // The last bar: its own chord (or none), then the turnaround — both traced to this
                // line's rhyme word, the line the bar plays.
                (own, Some(span), Some(turn)) => Harmony::Turning {
                    own: own.map(|own| chord(own, span)),
                    turn: chord(turn, span),
                },
                (Some(choice), Some(span), None) => Harmony::Chord(chord(choice, span)),
                (_, None, _) => Harmony::Rest(Rest {
                    bar,
                    line: line.line(),
                    why: Silence::Wordless,
                }),
                (None, Some(_), None) => Harmony::Rest(Rest {
                    bar,
                    line: line.line(),
                    why: Silence::Unrhymed,
                }),
            }
        })
        .collect()
}
