//! The reading: the poem's own lines, spoken over the music.
//!
//! *A candidate mapping, page-side, pending review* (`SPEC.md` §4.7), chosen between by
//! [`Mapping::Reading`]'s variants:
//!
//! - **`breath`** (the default). Each stanza is read through, one line a bar, in order. Stanza *k*
//!   starts in the later of two slots: the one its first line first arrives in as a host (see
//!   `arrives`), so a stanza is never read before the music has begun it; and the second slot
//!   after stanza *k − 1*'s last cue, so one silent bar always separates two stanzas' readings. A
//!   stanza that would run past the piece's last slot reads the lines that fit and loses the rest
//!   (`SPEC.md` §4.7) — and says which, in [`Reading::unread`] and the cue pattern's header.
//! - **`arrives`**. Each line is spoken once, in the slot of the arrangement where it first sounds
//!   as a host — [`Cause::Stated`], [`Cause::Arriving`] or [`Cause::Overruling`] — and never where
//!   it only loops, yields, holds, calls, refrains in its lane or repeats in the outro. So the
//!   reading follows the poem in order, one line per bar it arrives in.
//! - **`stanza`**. A stanza's lines are spoken one a bar, in order, from the first slot any of
//!   them is heard in that way. A stanza that *transforms* a running loop (§4.6) brings its lines
//!   in one a pass, so under `arrives` they are read several bars apart; under `stanza` the stanza
//!   is read through as written, while the music is still turning into it.
//! - **`off`**. No cues.
//!
//! Every utterance is traceable to its line's span (I4) under every variant.
//!
//! # Two paces
//!
//! The variants say *which* lines are read and *from where*; the page reads them at one of two
//! paces, its own setting (`SPEC.md` §4.7):
//!
//! - **natural** (the page's default, *decided by the user, 2026-10-24*): each line at the voice's
//!   own speed times its [`Pace`] — 0.92 to 1.08, drawn from the line's own seed — and the next
//!   line after it has finished plus its [`Pause`], read from how it ends. A line is no longer
//!   given a bar; its [`Onset`] says what it waits for, and [`Reading::natural_pattern`] carries
//!   the bars that start them on Strudel's clock. The page times the rest on the audio clock,
//!   since only it knows how long a voice takes.
//! - **fitted**: each line fitted to its bar at a rate ([`Reading::pattern`]), as it shipped.
//!
//! # Why a separate artefact, and why here
//!
//! It is **not** part of [`Strudel`](crate::Strudel)'s pattern text, for three reasons:
//!
//! - Strudel cannot speak. The cue pattern only *carries* a line number on Strudel's clock; the
//!   speaking is a callback the page attaches (`Pattern.onTrigger`, `@strudel/web` 1.3.0). Pasted
//!   into strudel.cc on its own, a cue pattern would ask the default output for a sound named
//!   nothing — so it must not be in the text a writer pastes.
//! - It is a candidate the page chooses between, and the pattern text is what the CLI prints and
//!   the corpus snapshots pin. Keeping it apart means changing the reading changes nothing the
//!   rest of the project measures.
//! - It carries words, not parameters: the page needs each line's text, which is data, not code.
//!
//! It lives in `prosody-emit` rather than in the page because it has to land on the *same* clock
//! as the pattern — the same cycles per minute, the same slots — and the arrangement and the
//! tempo are both read here. Here it is deterministic and tested without a browser; the page only
//! speaks what it is told, when Strudel's scheduler says the cue's bar begins.
//!
//! # What is structure and what is palette
//!
//! *Which* lines are spoken in *which* bar is structure, read from the arrangement and the chosen
//! [`Mappings`] alone — no [`Style`] field moves a cue (I3). *How fast* a line is spoken is a
//! parameter: at the fitted pace the rate is fitted so the line fits its bar at the piece's tempo,
//! and the tempo is palette, so a faster palette reads faster; at the natural pace a line's pace
//! and pause are its own text's, and no palette moves them. Which engine and voice speak is the
//! page's.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::parse::Poem;
use prosody_core::span::Span;
use prosody_core::voice::Score;
use prosody_core::{Cause, Domain, Lane, Seed};
use rand_core::Rng as _;

use crate::fmt_param;
use crate::strudel::{cycles_per_minute, nested, one_a_cycle, starts, weights};
use crate::style::Style;

/// How many syllables a speech synthesiser says a second at rate 1.
///
/// Conversational English runs at about 160–180 words a minute, at roughly 1.4 syllables a word:
/// four syllables a second. Browser voices differ; this is the middle of them, and the bounds on
/// the rate absorb the rest.
pub const SYLLABLES_PER_SECOND: f64 = 4.0;

/// How much of its bar a line is fitted into: a little short of all of it, so one line has ended
/// before the next bar's line begins.
pub const FILL: f64 = 0.85;

/// The slowest a line is read. Slower than this a synthesised voice smears into a drone; a short
/// line in a long bar simply ends early.
pub const SLOWEST: f64 = 0.6;

/// The fastest a line is read. Faster than this the words stop being words; a long line in a
/// short bar overruns, and the page cuts it off when the next line is due.
pub const FASTEST: f64 = 1.6;

/// The cycles a second of the scheduler the pattern plays on.
///
/// `@strudel/core`'s `.cpm(x)` is `fast(x / 60)` — it assumes a scheduler at one cycle a second
/// (1.2.6, `pattern.mjs`) — while the scheduler starts at 0.5 (`cyclist.mjs`). strudel.cc never
/// sees the mismatch: its REPL redefines `.cpm` against the scheduler's own rate before
/// evaluating (`repl.mjs`). The page evaluates with a plain `eval`, which gets core's `.cpm`, so
/// it sets its scheduler to this rate once Strudel has started, and every piece played there at
/// half its stated tempo until it did. The page and this constant must agree: the reading is
/// fitted to the bar as it actually sounds.
pub const SCHEDULER_CPS: f64 = 1.0;

/// One line of the poem as the reading speaks it, whichever pace the page reads at: the words, how
/// many syllables each sounds, and the two values the natural pace reads it with — its [`Pace`]
/// and the [`Pause`] after it.
#[derive(Debug, Clone, PartialEq)]
pub struct Spoken {
    line: usize,
    stanza: usize,
    span: Span,
    text: String,
    syllables: usize,
    words: Vec<Word>,
    pace: Pace,
    pause: Pause,
}

impl Spoken {
    /// Which line of the poem it speaks, in reading order from 0: the line's bar in the score.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// Which stanza the line is in, from 0 — what the page ducks the music for as one reading.
    #[must_use]
    pub const fn stanza(&self) -> usize {
        self.stanza
    }

    /// The line's span in the source (I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The words to speak: the line exactly as written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many syllables the line sounds — what the fitted rate is fitted to.
    #[must_use]
    pub const fn syllables(&self) -> usize {
        self.syllables
    }

    /// The line's words, in order, each with the syllables it sounds.
    #[must_use]
    pub fn words(&self) -> &[Word] {
        &self.words
    }

    /// How fast the natural pace reads it, against the voice's own speed.
    #[must_use]
    pub const fn pace(&self) -> Pace {
        self.pace
    }

    /// How long the natural pace waits after it, from how it ends.
    #[must_use]
    pub const fn pause(&self) -> Pause {
        self.pause
    }
}

/// A word of a spoken line and how many syllables it sounds. Where a speech engine reports no
/// word timings, the page spreads a rendered line's duration over its words by these counts, so
/// the word marker moves with the syllables rather than the letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Word {
    span: Span,
    syllables: usize,
}

impl Word {
    /// The word's span in the source (I4).
    #[must_use]
    pub const fn span(self) -> Span {
        self.span
    }

    /// The syllables it sounds in the score; a word nobody could pronounce counts one, since a
    /// voice will still say it.
    #[must_use]
    pub const fn syllables(self) -> usize {
        self.syllables
    }
}

/// How fast a line is read at the natural pace, in hundredths of the voice's own speed:
/// [`Pace::SLOWEST`]`..=`[`Pace::FASTEST`].
///
/// *Decided by the user, 2026-10-24* (`SPEC.md` §4.7): squeezing every line into its bar sounds
/// forced, so by default a line is read at the voice's natural speed with a small variation of
/// its own, as a reader never says two lines at quite the same speed. The variation is drawn from
/// the line's **own** seed ([`Seed::for_unit`] on the line's span, `Domain::Line`, child
/// `"reading pace"`), so it is the same every time (I1), an edit to one line moves only that
/// line's pace (I2), and a refrain line, whose text is the same, returns at the pace it left at.
/// Kept in whole hundredths, so it is an integer everywhere but the value handed to a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pace(u8);

impl Pace {
    /// The slowest, in hundredths: 0.92 of the voice's speed.
    pub const SLOWEST: u8 = 92;
    /// The fastest, in hundredths: 1.08 of the voice's speed.
    pub const FASTEST: u8 = 108;
    /// The voice's own speed, for a line whose span cannot be seeded (a caller's bug, and said:
    /// the header prints the pace of every line).
    pub const NEUTRAL: Self = Self(100);

    /// The pace of the line at `span` of `source`, or `None` if the span is not in it.
    #[must_use]
    pub fn of(span: Span, source: &str) -> Option<Self> {
        let seed = Seed::for_unit(Domain::Line, span, source)?.child("reading pace");
        Some(Self::from_draw(seed.rng().next_u32()))
    }

    /// One draw onto the seventeen steps from [`Pace::SLOWEST`] to [`Pace::FASTEST`].
    fn from_draw(draw: u32) -> Self {
        let steps = u32::from(Self::FASTEST - Self::SLOWEST) + 1;
        Self(Self::SLOWEST + u8::try_from(draw % steps).unwrap_or(0))
    }

    /// The pace in hundredths.
    #[must_use]
    pub const fn hundredths(self) -> u8 {
        self.0
    }

    /// The pace as the speed a voice is asked for (1 is its own). A value only; print it through
    /// [`fmt_param`].
    #[must_use]
    pub fn speed(self) -> f64 {
        f64::from(self.0) / 100.0
    }
}

/// The pause after a line at the natural pace, from how the line ends (`SPEC.md` §4.7).
///
/// The values are estimates of a reader's pauses, not measurements: a line break with nothing at
/// it is still a breath's catch, a comma a longer one, a full stop longer again, and the end of a
/// stanza the longest. They are the page's *least* waits — a stanza also waits for its music and
/// a silent bar (the breath rule), so at a stanza's end that pause is a floor the bar rule
/// usually exceeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pause {
    /// The line ends on a word: 250 ms.
    RunOn,
    /// A comma, semicolon, colon or dash: 450 ms.
    Comma,
    /// A full stop, question mark, exclamation mark or ellipsis: 800 ms.
    Stop,
    /// The stanza's last line, however it ends: 1500 ms.
    StanzaEnd,
}

impl Pause {
    /// After the line of `text`, the last of its stanza when `closes_stanza`.
    ///
    /// Read from the line's last character that is not space or a closing quote or bracket, so
    /// `"…bright,”` pauses as a comma.
    #[must_use]
    pub fn after(text: &str, closes_stanza: bool) -> Self {
        if closes_stanza {
            return Self::StanzaEnd;
        }
        let closers = |c: char| c.is_whitespace() || "\"'”’»)]}".contains(c);
        match text.trim_end_matches(closers).chars().next_back() {
            Some('.' | '?' | '!' | '…') => Self::Stop,
            Some(',' | ';' | ':' | '—' | '–' | '-') => Self::Comma,
            _ => Self::RunOn,
        }
    }

    /// How long, in milliseconds.
    #[must_use]
    pub const fn millis(self) -> u32 {
        match self {
            Self::RunOn => 250,
            Self::Comma => 450,
            Self::Stop => 800,
            Self::StanzaEnd => 1500,
        }
    }

    /// How long, in seconds — the value the page waits. A value only; print it through
    /// [`fmt_param`].
    #[must_use]
    pub fn seconds(self) -> f64 {
        f64::from(self.millis()) / 1000.0
    }

    /// What it follows, in words, as the cue pattern's header and the page say it.
    #[must_use]
    pub const fn says(self) -> &'static str {
        match self {
            Self::RunOn => "run-on",
            Self::Comma => "comma",
            Self::Stop => "full stop",
            Self::StanzaEnd => "stanza end",
        }
    }
}

/// When a line may begin at the natural pace (`SPEC.md` §4.7). The page times it on the audio
/// clock from where the music is and when the line before it actually ended — which only the
/// page knows, since it is the voice's own length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Onset {
    /// A stanza's first line under `breath`: in `slot`, where the music has begun the stanza —
    /// or, if the previous stanza's reading ended too late for that, on the first bar that
    /// begins at least the stanza-end pause after it and after one whole silent bar.
    Breath {
        /// The slot the music begins the stanza in.
        slot: usize,
    },
    /// In `slot` — or, if the line before it and its pause have not finished by then, as soon
    /// as they have.
    At {
        /// The slot it is first heard in (`arrives`) or its stanza is (`stanza`).
        slot: usize,
    },
    /// As soon as the line before it and its pause have finished.
    Follows,
}

impl Onset {
    /// The slot it waits for, if any.
    #[must_use]
    pub const fn slot(self) -> Option<usize> {
        match self {
            Self::Breath { slot } | Self::At { slot } => Some(slot),
            Self::Follows => None,
        }
    }

    /// Its name in the `/reading` answer.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Breath { .. } => "breath",
            Self::At { .. } => "at",
            Self::Follows => "follows",
        }
    }
}

/// One line, spoken in one slot: the fitted pace, each line fitted to its bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    slot: usize,
    rate: f64,
    spoken: Spoken,
}

impl Cue {
    /// The slot of the arrangement — the cycle — it is spoken in, from 0.
    #[must_use]
    pub const fn slot(&self) -> usize {
        self.slot
    }

    /// Which line of the poem it speaks, in reading order from 0: the line's bar in the score.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.spoken.line
    }

    /// The line's span in the source (I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.spoken.span
    }

    /// The words to speak: the line exactly as written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.spoken.text
    }

    /// How many syllables the line sounds — what the rate is fitted to.
    #[must_use]
    pub const fn syllables(&self) -> usize {
        self.spoken.syllables
    }

    /// The speaking rate, `SLOWEST..=FASTEST`, for a speech synthesiser's `rate`. A value only;
    /// print it through [`fmt_param`].
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// The line as spoken: its words, stanza, pace and pause.
    #[must_use]
    pub const fn spoken(&self) -> &Spoken {
        &self.spoken
    }
}

/// One line at the natural pace: when it may begin, and the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Natural {
    onset: Onset,
    spoken: Spoken,
}

impl Natural {
    /// When it may begin.
    #[must_use]
    pub const fn onset(&self) -> Onset {
        self.onset
    }

    /// The line as spoken.
    #[must_use]
    pub const fn spoken(&self) -> &Spoken {
        &self.spoken
    }
}

/// A line the reading does not speak although it was not switched off, and why — so the loss is
/// said, never silent (CLAUDE.md §6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    line: usize,
    span: Span,
    text: String,
    why: Why,
}

/// Why a line is left unread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The piece ends before the line's turn: its stanza was cut short, or would start after the
    /// last slot (`breath` and `stanza`, `SPEC.md` §4.7).
    PieceEnds,
    /// The line is never heard as a host, so no variant has a slot to read it from.
    NeverArrives,
}

impl Why {
    /// The reason, in words, as the cue pattern's comment and the page say it.
    #[must_use]
    pub const fn says(self) -> &'static str {
        match self {
            Self::PieceEnds => "the piece ends before its turn",
            Self::NeverArrives => "it never arrives in the music",
        }
    }
}

impl Unread {
    /// Which line of the poem, in reading order from 0.
    #[must_use]
    pub const fn line(&self) -> usize {
        self.line
    }

    /// The line's span in the source (I4).
    #[must_use]
    pub const fn span(&self) -> Span {
        self.span
    }

    /// The line exactly as written.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Why it is not read.
    #[must_use]
    pub const fn why(&self) -> Why {
        self.why
    }
}

/// Why a reading could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReadingError {
    /// The score is not this poem's lowering: it has a different number of lines.
    #[error("the score has {bars} bars but the poem has {lines} lines: not its lowering")]
    NotThisPoem {
        /// Bars in the score.
        bars: usize,
        /// Lines in the poem.
        lines: usize,
    },
}

/// The reading's own variants, read once from [`Mapping::Reading`]'s choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Off,
    Arrives,
    Stanza,
    Breath,
}

impl Kind {
    const fn of(variant: Variant) -> Self {
        match variant {
            Variant::Off => Self::Off,
            Variant::Stanza => Self::Stanza,
            Variant::Breath => Self::Breath,
            // The harmony's and the bass's variants are not the reading's: `Mapping::choose`
            // never stores them for it, so they fall with the default.
            Variant::Arrives
            | Variant::On
            | Variant::Rhyme
            | Variant::Cadence
            | Variant::Both
            | Variant::Height
            | Variant::Syllables
            | Variant::Feet
            | Variant::Steady
            | Variant::Elastic
            | Variant::Crossing
            | Variant::Straight
            | Variant::Full
            | Variant::Light
            | Variant::Gradual
            | Variant::Erode
            | Variant::Flat
            | Variant::Articulated
            | Variant::Strict
            | Variant::Slant => Self::Arrives,
        }
    }
}

/// The poem's reading: every line at most once, in the slot the chosen variant puts it in — at
/// the fitted pace, a line a bar, and at the natural pace, a line after the last.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    cues: Vec<Cue>,
    natural: Vec<Natural>,
    unread: Vec<Unread>,
    natural_unread: Vec<Unread>,
    slots: usize,
    cpm: f64,
    variant: Variant,
    kind: Kind,
    /// How many cycles each slot lasts — the music's own ([`weights`]) — or `None` under the
    /// elastic pulse, where every slot is one cycle and the cue pattern is written as it always
    /// was.
    weights: Option<Vec<u64>>,
    /// The cycle each slot begins on, then the piece's length ([`starts`]).
    starts: Vec<u64>,
}

impl Reading {
    /// Every cue of the fitted pace, in playing order — which is reading order.
    #[must_use]
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// Every line of the natural pace, in reading order, with when it may begin.
    #[must_use]
    pub fn natural(&self) -> &[Natural] {
        &self.natural
    }

    /// The lines left unread at the fitted pace, in reading order, and why: empty when every line
    /// is read, and always empty under `off`, where nothing is read by choice.
    #[must_use]
    pub fn unread(&self) -> &[Unread] {
        &self.unread
    }

    /// The lines the natural pace has no onset for — a stanza the music never begins. Where the
    /// natural pace runs out of piece depends on how long the voice takes, so the page says that.
    #[must_use]
    pub fn natural_unread(&self) -> &[Unread] {
        &self.natural_unread
    }

    /// The cycle each slot begins on, from 0, and last the piece's length — the music's own bars,
    /// for the page to find a bar on the audio clock.
    #[must_use]
    pub fn bars(&self) -> &[u64] {
        &self.starts
    }

    /// The pattern's cycles per minute: a cycle lasts `60 / cpm` seconds on the page's scheduler.
    #[must_use]
    pub const fn cpm(&self) -> f64 {
        self.cpm
    }

    /// The cue pattern of the fitted pace, as Strudel text: one element per slot, the spoken
    /// line's index where a line is first heard and a rest elsewhere — weighted as the music's
    /// bars are under the steady pulse (`<0@3 ~@4>`), one a cycle under the elastic — at the
    /// pattern's own cycles per minute, so each cue falls on the first beat of its bar.
    ///
    /// The page evaluates this, attaches its speaking callback with `.onTrigger(…, true)` — the
    /// `true` keeps Strudel's default output from trying to sound the cue — and stacks it with the
    /// music. Comment lines first, so the pattern says where every cue came from.
    #[must_use]
    pub fn pattern(&self) -> String {
        let what = match self.kind {
            Kind::Breath => {
                "each stanza read through a line a bar, a silent bar after the last, never before \
                 the music begins it"
            }
            Kind::Stanza => "each stanza read a line a bar from where it is first heard",
            Kind::Off => "switched off, nothing spoken",
            Kind::Arrives => "each line spoken in the bar it is first heard",
        };
        let mut out = format!(
            "// the reading: {what} (SPEC.md §4.7, candidate; reading = {})\n",
            self.variant.name()
        );
        let _ = writeln!(
            out,
            "// {} lines over {} bars; rate fitted to {} of a bar at {} syllables a second, within {}..{}",
            self.cues.len(),
            self.slots,
            fmt_param(FILL),
            fmt_param(SYLLABLES_PER_SECOND),
            fmt_param(SLOWEST),
            fmt_param(FASTEST),
        );
        // A lost line is said where the reading is described, not left for the listener to notice
        // (CLAUDE.md §6): `breath` can run out of piece (SPEC.md §4.7).
        say_unread(&mut out, &self.unread, self.cues.len());
        for cue in &self.cues {
            let _ = writeln!(
                out,
                "//   bar {} speaks line {} (bytes {}..{}), {} syllables at rate {}: {}",
                cue.slot + 1,
                cue.line() + 1,
                cue.span().start(),
                cue.span().end(),
                cue.syllables(),
                fmt_param(cue.rate),
                comment_safe(cue.text())
            );
        }
        let at = self.cues.iter().map(|cue| (cue.slot, cue.line()));
        let _ = writeln!(out, "{}", self.cue_line(at));
        out
    }

    /// The cue pattern of the natural pace, as Strudel text: one element per slot, a line's
    /// index in the slot it waits for ([`Onset::slot`]) and a rest elsewhere, on the music's
    /// clock exactly as [`Reading::pattern`] is. Its header lists every line, with its onset,
    /// pace and pause.
    ///
    /// A cue here is the *earliest* its line may begin: the page starts it on that bar, or later
    /// if the line before it is still being read, and chains every [`Onset::Follows`] line onto
    /// the end of the one before.
    #[must_use]
    pub fn natural_pattern(&self) -> String {
        let what = match self.kind {
            Kind::Breath => {
                "a stanza starts in the bar the music begins it, or a silent bar after the \
                 previous stanza's spoken end, and its lines follow each other"
            }
            Kind::Stanza => {
                "a stanza starts in the bar it is first heard, or when the line before ends, and \
                 its lines follow each other"
            }
            Kind::Off => "switched off, nothing spoken",
            Kind::Arrives => {
                "a line starts in the bar it is first heard, or when the line before ends"
            }
        };
        let mut out = format!(
            "// the reading at the natural pace: {what} (SPEC.md §4.7, candidate; reading = {})\n",
            self.variant.name()
        );
        let _ = writeln!(
            out,
            "// {} lines over {} bars, each at the voice's own speed times its pace ({}..{}, from \
             the line's own seed), then a pause: {} s after a run-on line, {} after a comma, {} \
             after a full stop, {} at a stanza's end",
            self.natural.len(),
            self.slots,
            fmt_param(f64::from(Pace::SLOWEST) / 100.0),
            fmt_param(f64::from(Pace::FASTEST) / 100.0),
            fmt_param(Pause::RunOn.seconds()),
            fmt_param(Pause::Comma.seconds()),
            fmt_param(Pause::Stop.seconds()),
            fmt_param(Pause::StanzaEnd.seconds()),
        );
        say_unread(&mut out, &self.natural_unread, self.natural.len());
        for natural in &self.natural {
            let spoken = &natural.spoken;
            let when = match natural.onset {
                Onset::Breath { slot } => {
                    format!("bar {}, a stanza, after a silent bar,", slot + 1)
                }
                Onset::At { slot } => format!("bar {}, or after the line before,", slot + 1),
                Onset::Follows => "then".to_owned(),
            };
            let _ = writeln!(
                out,
                "//   {when} line {} (bytes {}..{}): {} syllables, pace {}, then {} s ({}): {}",
                spoken.line + 1,
                spoken.span.start(),
                spoken.span.end(),
                spoken.syllables,
                fmt_param(spoken.pace.speed()),
                fmt_param(spoken.pause.seconds()),
                spoken.pause.says(),
                comment_safe(&spoken.text)
            );
        }
        let at = self
            .natural
            .iter()
            .filter_map(|natural| Some((natural.onset.slot()?, natural.spoken.line)));
        let _ = writeln!(out, "{}", self.cue_line(at));
        out
    }

    /// The `n("<…>")` line of a cue pattern: `line` in each of its slots, a rest elsewhere.
    fn cue_line(&self, at: impl Iterator<Item = (usize, usize)>) -> String {
        let mut steps = vec!["~".to_owned(); self.slots.max(1)];
        for (slot, line) in at {
            if let Some(step) = steps.get_mut(slot) {
                *step = line.to_string();
            }
        }
        // The music's bars, as long as the music's: under the steady pulse a slot lasts its
        // beats in cycles, and a cue is weighted exactly as the music's bar is
        // (`strudel::sequence`), so it fires on that bar's first beat.
        // Grouped as the music's bars are (`strudel::nested`), so a long piece's cues cost the
        // scheduler as little as its music does.
        let sequence = match &self.weights {
            // A piece of no slots still has its one rest, unweighted.
            Some(weights) if weights.len() == steps.len() => {
                nested(steps.into_iter().zip(weights.iter().copied()).collect())
            }
            Some(_) => steps.join(" "),
            None => one_a_cycle(steps),
        };
        format!("n(\"<{sequence}>\").cpm({})", fmt_param(self.cpm))
    }
}

/// The header lines that say which lines a cue pattern leaves unread, and why.
fn say_unread(out: &mut String, unread: &[Unread], read: usize) {
    if unread.is_empty() {
        return;
    }
    let _ = writeln!(
        out,
        "// {} of {} lines not read:",
        unread.len(),
        unread.len() + read,
    );
    for line in unread {
        let _ = writeln!(
            out,
            "//   line {} (bytes {}..{}) is not read, {}: {}",
            line.line + 1,
            line.span.start(),
            line.span.end(),
            line.why.says(),
            comment_safe(&line.text)
        );
    }
}

/// Plans the reading of `poem` from its lowering, the palette's tempo, and the variant
/// `mappings` chooses for [`Mapping::Reading`].
///
/// # Errors
///
/// [`ReadingError::NotThisPoem`] if `score` was not lowered from `poem`.
pub fn plan(
    poem: &Poem,
    score: &Score,
    style: &Style,
    mappings: &Mappings,
) -> Result<Reading, ReadingError> {
    let lines: Vec<_> = poem
        .stanzas()
        .iter()
        .flat_map(prosody_core::parse::Stanza::lines)
        .collect();
    if lines.len() != score.bars() {
        return Err(ReadingError::NotThisPoem {
            bars: score.bars(),
            lines: lines.len(),
        });
    }
    let cpm = cycles_per_minute(score, style);
    // How long each slot sounds, on the scheduler: its cycles over the cycles a second. Under the
    // steady pulse a slot of four beats lasts a third longer than one of three, and a line is
    // fitted to the bar it is spoken in, not to an average one.
    let all_weights = weights(score);
    let bar_seconds = |slot: usize| {
        let cycles = all_weights.get(slot).copied().unwrap_or(1);
        let cycles = f64::from(u32::try_from(cycles).unwrap_or(u32::MAX));
        cycles * 60.0 / cpm / SCHEDULER_CPS
    };
    let slots = score.arrangement().slots().len();
    let variant = mappings.get(Mapping::Reading);
    let kind = Kind::of(variant);

    let arrivals = arrivals(score);
    let placed: Vec<(usize, usize)> = match kind {
        Kind::Off => Vec::new(),
        Kind::Stanza => by_stanza(poem, &arrivals, slots),
        Kind::Breath => by_breath(poem, &arrivals, slots),
        Kind::Arrives => arrivals.clone(),
    };
    let spoken = |bar: usize| said(poem, score, &lines, bar);

    let cues: Vec<Cue> = placed
        .into_iter()
        .filter_map(|(slot, bar)| {
            let spoken = spoken(bar)?;
            Some(Cue {
                slot,
                rate: fitted(spoken.syllables, bar_seconds(slot)),
                spoken,
            })
        })
        .collect();
    let natural: Vec<Natural> = by_nature(kind, poem, &arrivals)
        .into_iter()
        .filter_map(|(onset, bar)| {
            Some(Natural {
                onset,
                spoken: spoken(bar)?,
            })
        })
        .collect();
    let (unread, natural_unread) = if kind == Kind::Off {
        (Vec::new(), Vec::new())
    } else {
        let fitted: BTreeSet<usize> = cues.iter().map(Cue::line).collect();
        let natural: BTreeSet<usize> = natural.iter().map(|n| n.spoken.line).collect();
        (
            unread(poem, &lines, &fitted, &arrivals),
            unread(poem, &lines, &natural, &arrivals),
        )
    };
    Ok(Reading {
        cues,
        natural,
        unread,
        natural_unread,
        slots,
        cpm,
        variant,
        kind,
        weights: (score.pulse() != Variant::Elastic).then_some(all_weights),
        starts: starts(score),
    })
}

/// Line `bar` of the poem as the reading speaks it, or `None` if its span is not in the source.
fn said(
    poem: &Poem,
    score: &Score,
    lines: &[&prosody_core::parse::Line],
    bar: usize,
) -> Option<Spoken> {
    let line = lines.get(bar)?;
    let text = line.span().text(poem.source())?;
    let mut first = 0;
    let (mut stanza, mut closes) = (0, false);
    for (index, each) in poem.stanzas().iter().enumerate() {
        let count = each.lines().len();
        if bar < first + count {
            stanza = index;
            closes = bar + 1 == first + count;
            break;
        }
        first += count;
    }
    let events: Vec<_> = score
        .events()
        .iter()
        .filter(|e| e.bar().get() == bar)
        .collect();
    // What the line sounds is what the rate fits; a line of words nobody could pronounce still
    // has words, and a synthesiser will say them, so it counts one each.
    let syllables = if events.is_empty() {
        line.words().count().max(1)
    } else {
        events.len()
    };
    let words = line
        .words()
        .map(|word| Word {
            span: word.span(),
            syllables: events
                .iter()
                .filter(|e| e.span() == word.span())
                .count()
                .max(1),
        })
        .collect();
    Some(Spoken {
        line: bar,
        stanza,
        span: line.span(),
        text: text.to_owned(),
        syllables,
        words,
        pace: Pace::of(line.span(), poem.source()).unwrap_or(Pace::NEUTRAL),
        pause: Pause::after(text, closes),
    })
}

/// The lines of `lines` not in `read`, and why: a line whose stanza is read in part, or which
/// arrives, lost its turn to the end of the piece; any other never arrives at all.
fn unread(
    poem: &Poem,
    lines: &[&prosody_core::parse::Line],
    read: &BTreeSet<usize>,
    arrivals: &[(usize, usize)],
) -> Vec<Unread> {
    let arrived: BTreeSet<usize> = arrivals.iter().map(|(_, line)| *line).collect();
    let mut out = Vec::new();
    let mut first_line = 0;
    for stanza in poem.stanzas() {
        let stanza_lines = first_line..first_line + stanza.lines().len();
        first_line = stanza_lines.end;
        let started = stanza_lines.clone().any(|line| read.contains(&line));
        for index in stanza_lines.filter(|line| !read.contains(line)) {
            let Some(line) = lines.get(index) else {
                continue;
            };
            let why = if started || arrived.contains(&index) {
                Why::PieceEnds
            } else {
                Why::NeverArrives
            };
            out.push(Unread {
                line: index,
                span: line.span(),
                text: line
                    .span()
                    .text(poem.source())
                    .unwrap_or_default()
                    .to_owned(),
                why,
            });
        }
    }
    out
}

/// The natural pace (`SPEC.md` §4.7): which lines are read, in reading order, and when each may
/// begin. The variants keep their meaning — `breath` and `stanza` read a stanza through from
/// where they start it, `arrives` reads each line where it is first heard — but a line is no
/// longer given a bar: it follows the line before it, after that line's pause.
///
/// Under `breath` a stanza starts where its first line first arrives (or, if it never does,
/// where its earliest line does), *not* the bar after the previous stanza's last cue: the breath
/// is measured on the page against the previous stanza's spoken end, which only the voice
/// decides. Under `stanza` it starts at the earliest arrival of any of its lines, as at the
/// fitted pace. A stanza none of whose lines arrives is not read, and is said
/// ([`Reading::natural_unread`]). Nothing here is cut at the piece's end: whether the reading
/// reaches it depends on the voice's length, so the page cuts and says.
fn by_nature(kind: Kind, poem: &Poem, arrivals: &[(usize, usize)]) -> Vec<(Onset, usize)> {
    let first_slot: BTreeMap<usize, usize> =
        arrivals.iter().map(|(slot, line)| (*line, *slot)).collect();
    let mut out = Vec::new();
    match kind {
        Kind::Off => {}
        Kind::Arrives => {
            let mut lines: Vec<(usize, usize)> = arrivals.to_vec();
            // Reading order; arrivals are in slot order, which a line arriving before an earlier
            // one would break, and the page chains in reading order.
            lines.sort_by_key(|(_, line)| *line);
            out.extend(
                lines
                    .into_iter()
                    .map(|(slot, line)| (Onset::At { slot }, line)),
            );
        }
        Kind::Stanza | Kind::Breath => {
            let mut first_line = 0;
            for stanza in poem.stanzas() {
                let lines = first_line..first_line + stanza.lines().len();
                first_line = lines.end;
                let earliest = || {
                    lines
                        .clone()
                        .filter_map(|line| first_slot.get(&line).copied())
                        .min()
                };
                let start = if kind == Kind::Breath {
                    first_slot.get(&lines.start).copied().or_else(earliest)
                } else {
                    earliest()
                };
                let Some(slot) = start else {
                    continue;
                };
                for line in lines.clone() {
                    let onset = match (line == lines.start, kind) {
                        (true, Kind::Breath) => Onset::Breath { slot },
                        (true, _) => Onset::At { slot },
                        (false, _) => Onset::Follows,
                    };
                    out.push((onset, line));
                }
            }
        }
    }
    out
}

/// Each line's first arrival, as `(slot, line)` in slot order: where `arrives` reads it.
fn arrivals(score: &Score) -> Vec<(usize, usize)> {
    let mut spoken = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (index, slot) in score.arrangement().slots().iter().enumerate() {
        let Some(host) = slot.host() else {
            continue;
        };
        // SPEC.md §4.7: only where a line *arrives* — its stanza stated, overruling, or replacing
        // the old loop's bar. Everywhere else the line is material already heard, and reading it
        // again would read the loop, not the poem.
        let arrives = matches!(
            host.cause(),
            Cause::Stated | Cause::Arriving { .. } | Cause::Overruling
        );
        let bar = host.bar().get();
        if arrives && matches!(host.lane(), Lane::Layer(_)) && spoken.insert(bar) {
            out.push((index, bar));
        }
    }
    out
}

/// The `stanza` variant: each stanza's lines one a slot, in order, from the earliest slot any of
/// them arrives in (SPEC.md §4.7).
///
/// Reading never runs backwards or doubles up: a line is never put in a slot at or before the
/// previous cue's, so if a stanza would start before the last one has finished being read, it
/// waits. A line that would fall past the piece's last slot is not read — the piece has ended.
/// Only a stanza none of whose lines arrives (none does, in today's arrangement) is skipped.
fn by_stanza(poem: &Poem, arrivals: &[(usize, usize)], slots: usize) -> Vec<(usize, usize)> {
    let first_slot: std::collections::BTreeMap<usize, usize> =
        arrivals.iter().map(|(slot, line)| (*line, *slot)).collect();
    let mut out = Vec::new();
    let mut next_free = 0;
    let mut first_line = 0;
    for stanza in poem.stanzas() {
        let count = stanza.lines().len();
        let lines = first_line..first_line + count;
        first_line += count;
        let Some(start) = lines
            .clone()
            .filter_map(|line| first_slot.get(&line).copied())
            .min()
        else {
            continue;
        };
        for (offset, line) in lines.enumerate() {
            let slot = (start + offset).max(next_free);
            if slot >= slots {
                break;
            }
            out.push((slot, line));
            next_free = slot + 1;
        }
    }
    out
}

/// The `breath` variant, the default: each stanza read through, a line a slot, in order, with a
/// silent slot between two stanzas' readings and never before the stanza's first line is heard
/// (SPEC.md §4.7).
///
/// Stanza *k* starts at the later of its first line's first arrival and the second slot after
/// the previous stanza's last cue. Its later lines follow one a slot, whether or not they have
/// arrived yet — a transforming stanza brings them in one a pass (§4.6), and the point is to read
/// the stanza as written, not as the loop reveals it. A stanza whose first line never arrives
/// starts at the earliest arrival of any of its lines; one none of whose lines arrives (none
/// does, in today's arrangement) is skipped and leaves no breath behind.
///
/// **Past the end** (decided, `SPEC.md` §4.7): a line that would fall after the piece's last slot
/// is not read, so a stanza that runs out of piece reads the lines that fit and stops, as under
/// `stanza`. The cue pattern loops with the music, so a cue after the last slot would land in the
/// next round's first bars, on top of the first stanza; and starting the stanza earlier would read
/// it before the music has begun it or without its breath. The outro (§4.6) plays the last stanza
/// once more after the poem, which is what usually gives the last stanza room.
fn by_breath(poem: &Poem, arrivals: &[(usize, usize)], slots: usize) -> Vec<(usize, usize)> {
    let first_slot: std::collections::BTreeMap<usize, usize> =
        arrivals.iter().map(|(slot, line)| (*line, *slot)).collect();
    let mut out = Vec::new();
    // The earliest slot the next stanza may start in: after the last cue and one silent slot.
    let mut next_free = 0;
    let mut first_line = 0;
    for stanza in poem.stanzas() {
        let count = stanza.lines().len();
        let lines = first_line..first_line + count;
        first_line += count;
        let Some(arrival) = first_slot.get(&lines.start).copied().or_else(|| {
            lines
                .clone()
                .filter_map(|line| first_slot.get(&line).copied())
                .min()
        }) else {
            continue;
        };
        let start = arrival.max(next_free);
        let mut last = None;
        for (offset, line) in lines.enumerate() {
            let slot = start + offset;
            if slot >= slots {
                break;
            }
            out.push((slot, line));
            last = Some(slot);
        }
        if let Some(last) = last {
            next_free = last + 2;
        }
    }
    out
}

/// `text` made safe to sit inside a `//` comment the page evaluates as JavaScript.
///
/// A line's span ends at a newline, but not every line terminator is a newline: JavaScript also
/// ends a comment at U+2028 and U+2029, which text pasted from the web can carry. Left raw, the
/// rest of the poem line would run as code and the page would refuse to play at all (CLAUDE.md
/// §6: user input never stops the program). Those and every control character are written as
/// their `\u{…}` escape instead, so the comment still shows exactly which character is there.
/// The spoken text itself travels in the cue, not in the comment, and is never escaped.
fn comment_safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') {
            out.extend(c.escape_unicode());
        } else {
            out.push(c);
        }
    }
    out
}

/// The rate at which `syllables` take [`FILL`] of a bar of `bar_seconds`, within
/// [`SLOWEST`]`..=`[`FASTEST`].
fn fitted(syllables: usize, bar_seconds: f64) -> f64 {
    let at_one = f64::from(u32::try_from(syllables).unwrap_or(u32::MAX)) / SYLLABLES_PER_SECOND;
    let room = bar_seconds * FILL;
    if room > 0.0 {
        (at_one / room).clamp(SLOWEST, FASTEST)
    } else {
        FASTEST
    }
}

#[cfg(test)]
mod tests {
    use prosody_core::lower::{self, Analyses};
    use prosody_core::phoneme::arpabet::ARPABET;
    use prosody_core::phoneme::{
        Inventory, Phonemiser, Pronunciation, Stress, Unpronounceable, syllabify,
    };
    use prosody_core::surprisal::{Lexicon, Rarity};
    use prosody_core::voice::Score;
    use prosody_core::{Cause, Poem, meter, repetition, rhyme, role, sound, surprisal};

    use prosody_core::mapping::{Mapping, Mappings, Variant};

    use super::{FASTEST, SLOWEST, Why, by_breath, by_stanza, comment_safe, fitted, plan};
    use crate::fmt_param;
    use crate::style::Style;

    /// Spelling is pronunciation: `A` a stressed vowel, `e` an unstressed one.
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
        // No closed class: every word keeps its dictionary stress under either variant.
        fn is_function_word(&self, _: &str) -> bool {
            false
        }

        fn rarity(&self, _word: &str) -> Rarity {
            Rarity::band(1)
        }
    }

    fn lowered(source: &str) -> (Poem, Score) {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let meter = meter::scan(&poem, &Spelled);
        let rhyme = rhyme::analyse(&poem, &Spelled);
        let repetition = repetition::analyse(&poem);
        let sound = sound::analyse(&poem, &Spelled);
        let surprisal = surprisal::analyse(&poem, &Spelled, &Flat);
        let roles = role::assign(&meter, &repetition).unwrap();
        let score = lower::run(Analyses {
            poem: &poem,
            meter: &meter,
            rhyme: &rhyme,
            repetition: &repetition,
            sound: &sound,
            surprisal: &surprisal,
            roles: &roles,
        })
        .unwrap();
        (poem, score)
    }

    /// 120 bpm exactly, so the clock can be worked by hand: over two-beat bars, 60 cycles a
    /// minute, and on a scheduler at one cycle a second, a bar of one second.
    fn at_120() -> Style {
        Style::parse("+++\ntempo_range = [120, 120]\n+++").unwrap()
    }

    /// Two stanzas of two two-foot lines, of one layer: the second transforms the first's loop.
    const TRANSFORMING: &str = "eA eA\nteA teA\n\nbeA beA\ndeA deA\n";

    /// A refrain stanza, a ragged one, and the refrain stanza again.
    const REFRAIN: &str = "eA eA\nteA teA\n\nbA\nbA bA bA bA bA\n\neA eA\nteA teA\n";

    #[test]
    fn a_line_is_read_where_it_arrives_and_nowhere_it_only_loops() {
        let (poem, score) = lowered(TRANSFORMING);
        // The arrangement this is about, by hand (SPEC.md §4.6): lines 0 and 1 stated; then the
        // second stanza transforms the loop — pass 1 is |2 1|, pass 2 is |2 3| — then the outro
        // plays the last stanza once more, |2 3|. Asserted first, so a change to the arrangement
        // fails here rather than reading as a change to the reading.
        let hosts: Vec<(usize, Cause)> = score
            .arrangement()
            .slots()
            .iter()
            .map(|slot| {
                let host = slot.host().unwrap();
                (host.bar().get(), host.cause())
            })
            .collect();
        assert_eq!(
            hosts,
            vec![
                (0, Cause::Stated),
                (1, Cause::Stated),
                (2, Cause::Arriving { pass: 1 }),
                (1, Cause::Yielding { pass: 1 }),
                (2, Cause::Arriving { pass: 2 }),
                (3, Cause::Arriving { pass: 2 }),
                (2, Cause::Repeating),
                (3, Cause::Repeating),
            ]
        );

        let reading = plan(&poem, &score, &at_120(), &reading(Variant::Arrives)).unwrap();
        // Each line once, in the slot it is first heard: line 2 in slot 2 (its first arrival, not
        // its second in slot 4), line 3 in slot 5. Slot 3 yields, slot 4 re-arrives, 6 and 7 are
        // the outro: silent.
        let spoken: Vec<(usize, usize, &str)> = reading
            .cues()
            .iter()
            .map(|cue| (cue.slot(), cue.line(), cue.text()))
            .collect();
        assert_eq!(
            spoken,
            vec![
                (0, 0, "eA eA"),
                (1, 1, "teA teA"),
                (2, 2, "beA beA"),
                (5, 3, "deA deA"),
            ]
        );
        assert!(
            reading
                .pattern()
                // Every line of two beats, so every bar two cycles — a cycle is a beat at 120.
                .ends_with("n(\"<0@2 1@2 2@2 ~@2 ~@2 3@2 ~@2 ~@2>\").cpm(120.000)\n"),
            "{}",
            reading.pattern()
        );
    }

    /// The default choices with the reading set to `variant`.
    fn reading(variant: Variant) -> Mappings {
        Mappings::all().with(Mapping::Reading.choose(variant).unwrap())
    }

    #[test]
    fn the_stanza_variant_reads_a_transforming_stanza_through_from_its_first_arrival() {
        // The same arrangement as above. Stanza 1's lines first arrive in slots 0 and 1, so it is
        // read there as before. Stanza 2's first arrival is line 2 in slot 2; under `stanza` its
        // lines are read one a bar from there — line 2 in slot 2, line 3 in slot 3 — instead of
        // line 3 waiting for its own arrival in slot 5, while slot 3 is still the old loop's
        // yielding line 1.
        let (poem, score) = lowered(TRANSFORMING);
        let reading = plan(&poem, &score, &at_120(), &reading(Variant::Stanza)).unwrap();
        let spoken: Vec<(usize, usize, &str)> = reading
            .cues()
            .iter()
            .map(|cue| (cue.slot(), cue.line(), cue.text()))
            .collect();
        assert_eq!(
            spoken,
            vec![
                (0, 0, "eA eA"),
                (1, 1, "teA teA"),
                (2, 2, "beA beA"),
                (3, 3, "deA deA"),
            ]
        );
        let pattern = reading.pattern();
        assert!(
            pattern.ends_with("n(\"<0@2 1@2 2@2 3@2 ~@2 ~@2 ~@2 ~@2>\").cpm(120.000)\n"),
            "{pattern}"
        );
        assert!(
            pattern.starts_with("// the reading: each stanza read a line a bar"),
            "{pattern}"
        );
        assert!(pattern.contains("reading = stanza"), "{pattern}");
        // The rate is fitted to the bar exactly as under `arrives`: four syllables, 1.176.
        assert_eq!(fmt_param(reading.cues()[3].rate()), "1.176");
    }

    #[test]
    fn the_breath_variant_is_the_default() {
        let (poem, score) = lowered(TRANSFORMING);
        let default = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let breath = plan(&poem, &score, &at_120(), &reading(Variant::Breath)).unwrap();
        assert_eq!(default, breath);
        assert!(default.pattern().contains("reading = breath"));
        let arrives = plan(&poem, &score, &at_120(), &reading(Variant::Arrives)).unwrap();
        assert_ne!(default, arrives);
        assert!(arrives.pattern().contains("reading = arrives"));
    }

    /// `(slot, line, text)` of every cue.
    fn spoken(reading: &super::Reading) -> Vec<(usize, usize, &str)> {
        reading
            .cues()
            .iter()
            .map(|cue| (cue.slot(), cue.line(), cue.text()))
            .collect()
    }

    #[test]
    fn the_breath_variant_reads_a_stanza_whole_with_a_silent_bar_before_the_next() {
        // The arrangement asserted in the first test: line 0 stated in slot 0, line 1 in 1, line
        // 2 arriving in 2 (pass 1, over the old loop's yielding line 1 in 3), line 3 in 5.
        //
        // By hand: stanza 1 starts at its first line's arrival, slot 0, and reads lines 0 and 1
        // in slots 0 and 1. Stanza 2's first line arrives in slot 2, but slot 2 is the breath
        // after slot 1, so it starts at the later of 2 and 1 + 2 = 3: line 2 in slot 3, line 3 in
        // slot 4 — a bar apart, not the three of `arrives`, and after the music began it in 2.
        let (poem, score) = lowered(TRANSFORMING);
        let reading = plan(&poem, &score, &at_120(), &reading(Variant::Breath)).unwrap();
        assert_eq!(
            spoken(&reading),
            vec![
                (0, 0, "eA eA"),
                (1, 1, "teA teA"),
                (3, 2, "beA beA"),
                (4, 3, "deA deA"),
            ]
        );
        let pattern = reading.pattern();
        // Every bar two beats, two cycles under the steady pulse: slot 2 is the breath.
        assert!(
            pattern.ends_with("n(\"<0@2 1@2 ~@2 2@2 3@2 ~@2 ~@2 ~@2>\").cpm(120.000)\n"),
            "{pattern}"
        );
        assert!(
            pattern.starts_with(
                "// the reading: each stanza read through a line a bar, a silent bar after the \
                 last, never before the music begins it (SPEC.md §4.7, candidate; reading = breath)"
            ),
            "{pattern}"
        );
        assert!(
            pattern.contains("//   bar 4 speaks line 3 (bytes 15..22), 4 syllables at rate 1.176"),
            "{pattern}"
        );
        // Each rate is fitted to the cue's own bar: two beats at 120, one second, four
        // syllables — 1 / 0.85 = 1.176.
        for cue in reading.cues() {
            assert_eq!(fmt_param(cue.rate()), "1.176");
        }
    }

    /// Three stanzas of three iambic lines, of one layer: the second transforms the first's loop,
    /// the third the second's.
    const THREE: &str =
        "eA eA\nteA teA\nneA neA\n\nbeA beA\ndeA deA\neAn eAn\n\neAt eAt\neAd eAd\nbeAt beAt\n";

    #[test]
    fn the_three_stanza_arrangement_by_hand() {
        let (_, score) = lowered(THREE);
        // The arrangement by hand (SPEC.md §4.6): lines 0–2 stated in slots 0–2. Stanza 2
        // transforms the loop a bar a pass: |3 1 2| in 3–5, |3 4 2| in 6–8, |3 4 5| in 9–11.
        // Stanza 3 the same over stanza 2's loop: |6 4 5| in 12–14, |6 7 5| in 15–17, |6 7 8| in
        // 18–20. The outro plays the last loop once more, |6 7 8| in 21–23.
        let hosts: Vec<(usize, Cause)> = score
            .arrangement()
            .slots()
            .iter()
            .map(|slot| {
                let host = slot.host().unwrap();
                (host.bar().get(), host.cause())
            })
            .collect();
        let arriving = |pass| Cause::Arriving { pass };
        let yielding = |pass| Cause::Yielding { pass };
        assert_eq!(
            hosts,
            vec![
                (0, Cause::Stated),
                (1, Cause::Stated),
                (2, Cause::Stated),
                (3, arriving(1)),
                (1, yielding(1)),
                (2, yielding(1)),
                (3, arriving(2)),
                (4, arriving(2)),
                (2, yielding(2)),
                (3, arriving(3)),
                (4, arriving(3)),
                (5, arriving(3)),
                (6, arriving(1)),
                (4, yielding(1)),
                (5, yielding(1)),
                (6, arriving(2)),
                (7, arriving(2)),
                (5, yielding(2)),
                (6, arriving(3)),
                (7, arriving(3)),
                (8, arriving(3)),
                (6, Cause::Repeating),
                (7, Cause::Repeating),
                (8, Cause::Repeating),
            ]
        );
    }

    #[test]
    fn three_stanzas_under_every_variant_by_hand() {
        // The arrangement is the one asserted just above.
        let (poem, score) = lowered(THREE);
        let slots = |variant| -> Vec<(usize, usize)> {
            plan(&poem, &score, &at_120(), &reading(variant))
                .unwrap()
                .cues()
                .iter()
                .map(|cue| (cue.slot(), cue.line()))
                .collect()
        };
        // `arrives`, unchanged: each line in the first slot it arrives in.
        assert_eq!(
            slots(Variant::Arrives),
            vec![
                (0, 0),
                (1, 1),
                (2, 2),
                (3, 3),
                (7, 4),
                (11, 5),
                (12, 6),
                (16, 7),
                (20, 8)
            ]
        );
        // `stanza`, unchanged: each stanza a line a slot from its first arrival — stanza 2 from 3,
        // straight after stanza 1, stanza 3 from 12.
        assert_eq!(
            slots(Variant::Stanza),
            vec![
                (0, 0),
                (1, 1),
                (2, 2),
                (3, 3),
                (4, 4),
                (5, 5),
                (12, 6),
                (13, 7),
                (14, 8)
            ]
        );
        // `breath`: stanza 1 in 0–2. Stanza 2 arrives in 3, which is the breath after 2, so it
        // starts at max(3, 2 + 2) = 4: slots 4–6. Stanza 3 arrives in 12, long after the breath
        // after 6 (slot 8), so it starts at max(12, 6 + 2) = 12: slots 12–14.
        assert_eq!(
            slots(Variant::Breath),
            vec![
                (0, 0),
                (1, 1),
                (2, 2),
                (4, 3),
                (5, 4),
                (6, 5),
                (12, 6),
                (13, 7),
                (14, 8)
            ]
        );
        // 24 slots of 2 beats, more than `strudel::GROUP`, so the rests between the cues are
        // joined (`strudel::nested`): five after slot 6 (7–11, ten beats), nine after slot 14
        // (15–23, eighteen). Twelve elements remain, so nothing is grouped.
        let pattern = plan(&poem, &score, &at_120(), &Mappings::all())
            .unwrap()
            .pattern();
        assert!(
            pattern.ends_with(
                "n(\"<0@2 1@2 2@2 ~@2 3@2 4@2 5@2 ~@10 6@2 7@2 8@2 ~@18>\").cpm(120.000)\n"
            ),
            "{pattern}"
        );
    }

    #[test]
    fn a_breath_starts_at_the_later_of_the_arrival_and_the_silent_bar_and_stops_with_the_piece() {
        // Hand-made arrivals, to reach the edges the arrangement does not reach today. Lines 0–1
        // are stanza 1, 2–3 stanza 2, 4–5 stanza 3.
        let poem =
            prosody_core::Poem::parse("eA\nteA\n\nbeA\ndeA\n\nneA\ntA\n".to_owned()).unwrap();
        // Stanza 1 from slot 0: 0, 1. Stanza 2's first line arrives in 6, after the breath (3):
        // it waits for the music, 6, 7. Stanza 3's first line arrives in 7, before the breath
        // after 7 (9): it waits for the breath, 9, 10.
        let arrivals = [(0, 0), (1, 1), (6, 2), (7, 4), (8, 3), (9, 5)];
        assert_eq!(
            by_breath(&poem, &arrivals, 12),
            vec![(0, 0), (1, 1), (6, 2), (7, 3), (9, 4), (10, 5)]
        );
        // Past the end: with ten slots (0–9) stanza 3 starts in 9 and only its first line fits;
        // its second is not read rather than wrapping round onto the next pass's first bars.
        assert_eq!(
            by_breath(&poem, &arrivals, 10),
            vec![(0, 0), (1, 1), (6, 2), (7, 3), (9, 4)]
        );
        // With nine, stanza 3 would start after the piece: not read at all.
        assert_eq!(
            by_breath(&poem, &arrivals, 9),
            vec![(0, 0), (1, 1), (6, 2), (7, 3)]
        );
    }

    #[test]
    fn a_breath_starts_from_the_first_line_and_skips_a_stanza_never_heard() {
        let poem =
            prosody_core::Poem::parse("eA\nteA\n\nbeA\ndeA\n\nneA\ntA\n".to_owned()).unwrap();
        // Stanza 2's second line (3) arrives in slot 4, before its first (2) in slot 6: the
        // stanza waits for its first line, 6 and 7, never reading it before the music has.
        // Stanza 3 never arrives: skipped, and it leaves no breath — nothing follows anyway.
        let arrivals = [(0, 0), (1, 1), (4, 3), (6, 2)];
        assert_eq!(
            by_breath(&poem, &arrivals, 12),
            vec![(0, 0), (1, 1), (6, 2), (7, 3)]
        );
        // A stanza whose first line never arrives as a host starts at the earliest of its lines'
        // arrivals: line 3 in slot 4, so 4 and 5 — after the breath, slot 3.
        let arrivals = [(0, 0), (1, 1), (4, 3), (9, 4), (10, 5)];
        assert_eq!(
            by_breath(&poem, &arrivals, 12),
            vec![(0, 0), (1, 1), (4, 2), (5, 3), (9, 4), (10, 5)]
        );
    }

    #[test]
    fn a_line_left_unread_says_why() {
        let poem =
            prosody_core::Poem::parse("eA\nteA\n\nbeA\ndeA\n\nneA\ntA\n".to_owned()).unwrap();
        let lines: Vec<_> = poem
            .stanzas()
            .iter()
            .flat_map(prosody_core::parse::Stanza::lines)
            .collect();
        // Lines 0 and 2 read. Line 1 arrived and its stanza was read: the piece ended first.
        // Line 3 never arrived, but its stanza was read, so it too lost its turn to the end. Stanza
        // 3 (lines 4 and 5) has no cue and no arrival: it never arrives.
        let read = std::collections::BTreeSet::from([0, 2]);
        let left = super::unread(&poem, &lines, &read, &[(0, 0), (1, 1), (4, 2)]);
        let said: Vec<(usize, &str, Why)> = left
            .iter()
            .map(|line| (line.line(), line.text(), line.why()))
            .collect();
        assert_eq!(
            said,
            vec![
                (1, "teA", Why::PieceEnds),
                (3, "deA", Why::PieceEnds),
                (4, "neA", Why::NeverArrives),
                (5, "tA", Why::NeverArrives),
            ]
        );
        // Bytes: "eA\nteA\n\nbeA\ndeA\n\n" is 3 + 4 + 1 + 4 + 4 + 1 = 17 bytes, so line 4
        // is 17..20 (I4).
        assert_eq!((left[2].span().start(), left[2].span().end()), (17, 20));
    }

    #[test]
    fn the_reading_switched_off_cues_nothing() {
        let (poem, score) = lowered(TRANSFORMING);
        let reading = plan(&poem, &score, &at_120(), &reading(Variant::Off)).unwrap();
        assert!(reading.cues().is_empty());
        // Eight slots, all rests.
        assert!(
            reading
                .pattern()
                .ends_with("n(\"<~@2 ~@2 ~@2 ~@2 ~@2 ~@2 ~@2 ~@2>\").cpm(120.000)\n"),
            "{}",
            reading.pattern()
        );
        assert!(reading.pattern().contains("reading = off"));
    }

    #[test]
    fn a_stanza_waits_for_the_last_to_be_read_and_stops_with_the_piece() {
        // Hand-made arrivals, to reach the two edges the arrangement does not reach today. Stanza
        // 1 (lines 0, 1) first arrives in slot 0, so it takes slots 0 and 1. Stanza 2 (lines 2,
        // 3) first arrives in slot 1 — already taken — so it waits: line 2 in slot 2, line 3 in
        // slot 3. With only three slots, line 3 would fall after the piece and is not read.
        let poem = prosody_core::Poem::parse("eA\nteA\n\nbeA\ndeA\n".to_owned()).unwrap();
        let arrivals = [(0, 0), (1, 2), (3, 1), (5, 3)];
        assert_eq!(
            by_stanza(&poem, &arrivals, 8),
            vec![(0, 0), (1, 1), (2, 2), (3, 3)]
        );
        assert_eq!(by_stanza(&poem, &arrivals, 3), vec![(0, 0), (1, 1), (2, 2)]);
    }

    #[test]
    fn under_every_variant_a_line_is_read_at_most_once_and_in_order() {
        // I4 and the reading's order, for both fixtures and every variant.
        for source in [TRANSFORMING, REFRAIN] {
            let (poem, score) = lowered(source);
            for variant in Mapping::Reading.variants() {
                let reading = plan(&poem, &score, &at_120(), &reading(*variant)).unwrap();
                let lines: Vec<usize> = reading.cues().iter().map(super::Cue::line).collect();
                let slots: Vec<usize> = reading.cues().iter().map(super::Cue::slot).collect();
                assert!(
                    lines.windows(2).all(|w| w[0] < w[1]),
                    "{variant:?}: {lines:?}"
                );
                assert!(
                    slots.windows(2).all(|w| w[0] < w[1]),
                    "{variant:?}: {slots:?}"
                );
                for cue in reading.cues() {
                    assert_eq!(cue.span().text(poem.source()), Some(cue.text()));
                }
            }
        }
    }

    #[test]
    fn every_cue_is_its_lines_span() {
        // I4: the spoken text is exactly the source under the cue's span.
        let (poem, score) = lowered(TRANSFORMING);
        for cue in plan(&poem, &score, &at_120(), &Mappings::all())
            .unwrap()
            .cues()
        {
            assert_eq!(cue.span().text(poem.source()), Some(cue.text()));
        }
    }

    #[test]
    fn a_refrain_stanza_is_read_again_but_the_lanes_are_not() {
        // The third stanza repeats the first, so both are refrain stanzas and both overrule
        // (SPEC.md §4.6): slots 0 and 1. The ragged second stanza is stated in slots 2 and 3, and
        // the third drops in at 4 and 5 — new lines of the poem, so read. The outro after them,
        // and the refrain lane that loops the returning lines, are not.
        let (poem, score) = lowered(REFRAIN);
        let causes: Vec<(usize, Cause)> = score
            .arrangement()
            .slots()
            .iter()
            .map(|slot| {
                (
                    slot.host().unwrap().bar().get(),
                    slot.host().unwrap().cause(),
                )
            })
            .collect();
        assert_eq!(
            causes[..6],
            [
                (0, Cause::Overruling),
                (1, Cause::Overruling),
                (2, Cause::Stated),
                (3, Cause::Stated),
                (4, Cause::Overruling),
                (5, Cause::Overruling),
            ]
        );
        assert!(causes.len() > 6, "an outro follows: {causes:?}");

        let reading = plan(&poem, &score, &at_120(), &reading(Variant::Arrives)).unwrap();
        let spoken: Vec<(usize, usize)> = reading
            .cues()
            .iter()
            .map(|cue| (cue.slot(), cue.line()))
            .collect();
        assert_eq!(spoken, vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4), (5, 5)]);
        assert_eq!(
            reading.cues()[4].text(),
            "eA eA",
            "the returning line, read again"
        );
        // Under `breath`, the default, each stanza waits a silent bar after the last: stanza 2
        // arrives in 2 but starts at 1 + 2 = 3, so 3 and 4; stanza 3 arrives in 4 but starts at
        // 4 + 2 = 6, so 6 and 7 — over the outro, which plays it again (eight slots in all at
        // least, asserted next). The returning stanza is still read.
        assert!(causes.len() >= 8, "{causes:?}");
        let breath = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let spoken: Vec<(usize, usize)> = breath
            .cues()
            .iter()
            .map(|cue| (cue.slot(), cue.line()))
            .collect();
        assert_eq!(spoken, vec![(0, 0), (1, 1), (3, 2), (4, 3), (6, 4), (7, 5)]);
    }

    #[test]
    fn the_rate_fits_the_line_to_its_bar() {
        // Hand-worked. 120 bpm over two-beat bars is `.cpm(60)`, which Strudel plays as a bar of
        // 60 / 60 / 1 = one second (SCHEDULER_CPS); 85% of it is 0.85 s. "eA eA" sounds four syllables, 1 s at four a second, so the rate is
        // 1 / 0.85 = 1.176. "teA teA" is the same four syllables.
        let (poem, score) = lowered(TRANSFORMING);
        let reading = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        assert_eq!(fmt_param(reading.cues()[0].rate()), "1.176");
        assert_eq!(reading.cues()[0].syllables(), 4);

        // Twice the tempo halves the bar, so the same line would need 2.353 — bounded to FASTEST.
        let fast = Style::parse("+++\ntempo_range = [240, 240]\n+++").unwrap();
        let reading = plan(&poem, &score, &fast, &Mappings::all()).unwrap();
        assert_eq!(fmt_param(reading.cues()[0].rate()), fmt_param(FASTEST));
    }

    #[test]
    fn the_rate_is_bounded_both_ways() {
        // One syllable in a one-second bar would be 0.294: too slow to be speech.
        assert_eq!(fmt_param(fitted(1, 1.0)), fmt_param(SLOWEST));
        assert_eq!(fmt_param(fitted(40, 1.0)), fmt_param(FASTEST));
        // Two seconds, 0.85 of it 1.7 s, six syllables 1.5 s: 0.882.
        assert_eq!(fmt_param(fitted(6, 2.0)), "0.882");
    }

    #[test]
    fn the_palette_moves_the_rate_never_the_cues() {
        // I3: which line is read in which bar is structure. A different palette may read faster,
        // not elsewhere.
        let (poem, score) = lowered(TRANSFORMING);
        let brisk = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        // 90 bpm: a bar of 4/3 s, 85% of it 1.133 s, so four syllables at 0.882.
        let other = Style::parse("+++\ntempo_range = [90, 90]\nscale = \"lydian\"\n+++").unwrap();
        let steady = plan(&poem, &score, &other, &Mappings::all()).unwrap();
        let shape = |reading: &super::Reading| -> Vec<(usize, usize)> {
            reading
                .cues()
                .iter()
                .map(|cue| (cue.slot(), cue.line()))
                .collect()
        };
        assert_eq!(shape(&brisk), shape(&steady));
        assert_eq!(fmt_param(brisk.cues()[0].rate()), "1.176");
        assert_eq!(fmt_param(steady.cues()[0].rate()), "0.882");
    }

    #[test]
    fn the_same_poem_reads_the_same_bytes() {
        // I1.
        let (poem, score) = lowered(TRANSFORMING);
        let once = plan(&poem, &score, &at_120(), &Mappings::all())
            .unwrap()
            .pattern();
        let (poem, score) = lowered(TRANSFORMING);
        assert_eq!(
            once,
            plan(&poem, &score, &at_120(), &Mappings::all())
                .unwrap()
                .pattern()
        );
    }

    #[test]
    fn a_line_terminator_inside_a_line_cannot_end_its_comment() {
        // Worked by hand: U+2028 and U+2029 end a JavaScript comment as a newline does; a tab is
        // a control character. Each becomes its escape; ordinary text, accents included, stays.
        assert_eq!(
            comment_safe("the cat sat\u{2028}on the mat"),
            "the cat sat\\u{2028}on the mat"
        );
        assert_eq!(
            comment_safe("a\u{2029}b\tc\rd"),
            "a\\u{2029}b\\u{9}c\\u{d}d"
        );
        assert_eq!(comment_safe("Tyger, Tyger — žar"), "Tyger, Tyger — žar");
    }

    #[test]
    fn a_pasted_line_separator_stays_in_the_comment_and_in_the_spoken_text() {
        let source = "bAt\u{2028}dAt\nnAd\n";
        let (poem, score) = lowered(source);
        let reading = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let pattern = reading.pattern();
        // Every evaluated line is a comment or the one `n(...)` line: nothing after a line
        // terminator escapes its comment.
        let terminators = |c: char| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
        let lines: Vec<&str> = pattern
            .split(terminators)
            .filter(|l| !l.is_empty())
            .collect();
        assert!(
            lines.iter().rev().skip(1).all(|l| l.starts_with("//")),
            "{pattern}"
        );
        assert!(lines.last().unwrap().starts_with("n(\""), "{pattern}");
        assert!(pattern.contains("bAt\\u{2028}dAt"), "{pattern}");
        // The cue keeps the line's exact source, separator and all, for the page to speak (I4).
        assert_eq!(reading.cues()[0].text(), "bAt\u{2028}dAt");
    }

    #[test]
    fn a_pause_is_read_from_how_the_line_ends() {
        use super::Pause;
        // Hand-worked from SPEC.md §4.7's table: the last character that is not space or a
        // closing quote or bracket.
        assert_eq!(
            Pause::after("Tyger Tyger, burning bright,", false),
            Pause::Comma
        );
        assert_eq!(
            Pause::after("In the forests of the night;", false),
            Pause::Comma
        );
        assert_eq!(
            Pause::after("What the hand dare seize the fire?", false),
            Pause::Stop
        );
        assert_eq!(Pause::after("Prijat'lji!", false), Pause::Stop);
        assert_eq!(Pause::after("And so on...", false), Pause::Stop);
        assert_eq!(Pause::after("and then \u{2014}", false), Pause::Comma);
        assert_eq!(
            Pause::after("\u{201c}burning bright,\u{201d}  ", false),
            Pause::Comma
        );
        assert_eq!(Pause::after("(as if it were.)", false), Pause::Stop);
        assert_eq!(
            Pause::after("And what shoulder, & what art", false),
            Pause::RunOn
        );
        assert_eq!(Pause::after("", false), Pause::RunOn);
        // A stanza's last line pauses longest, however it ends.
        assert_eq!(
            Pause::after("Could frame thy fearful symmetry?", true),
            Pause::StanzaEnd
        );
        assert_eq!(Pause::after("and on", true), Pause::StanzaEnd);
        // The values, ascending: a run-on line the least, a stanza's end the most.
        let millis: Vec<u32> = [Pause::RunOn, Pause::Comma, Pause::Stop, Pause::StanzaEnd]
            .iter()
            .map(|pause| pause.millis())
            .collect();
        assert_eq!(millis, vec![250, 450, 800, 1500]);
        assert_eq!(fmt_param(Pause::Comma.seconds()), "0.450");
    }

    #[test]
    fn a_pace_is_one_of_seventeen_hundredths_from_its_own_lines_seed() {
        use super::Pace;
        use prosody_core::span::Span;
        // The draw onto the steps, by hand: seventeen steps from 92 to 108, the draw modulo 17.
        // 2^32 = (2^8)^4 and 2^8 = 256 = 15 * 17 + 1, so 2^32 - 1 is a multiple of 17: step 0.
        assert_eq!(Pace::from_draw(0).hundredths(), 92);
        assert_eq!(Pace::from_draw(16).hundredths(), 108);
        assert_eq!(Pace::from_draw(17).hundredths(), 92);
        assert_eq!(Pace::from_draw(25).hundredths(), 100);
        assert_eq!(Pace::from_draw(u32::MAX).hundredths(), 92);
        assert_eq!(fmt_param(Pace::from_draw(5).speed()), "0.970");
        // The line's own text alone: the same line at another place in another poem paces the
        // same — a refrain returns at the pace it left at — and a span outside the source has none.
        let here = "eA eA\nteA teA\n";
        let there = "bAt\n\nteA teA\n";
        assert_eq!(
            Pace::of(Span::new(6, 13), here),
            Pace::of(Span::new(5, 12), there)
        );
        assert_eq!(Pace::of(Span::new(6, 99), here), None);
        // Within bounds for many lines.
        for n in 0..200 {
            let text = format!("line number {n}");
            let pace = Pace::of(Span::new(0, text.len()), &text).unwrap();
            assert!((Pace::SLOWEST..=Pace::FASTEST).contains(&pace.hundredths()));
        }
    }

    /// `(onset, line)` of every line at the natural pace.
    fn natural(reading: &super::Reading) -> Vec<(super::Onset, usize)> {
        reading
            .natural()
            .iter()
            .map(|n| (n.onset(), n.spoken().line()))
            .collect()
    }

    #[test]
    fn at_the_natural_pace_a_stanza_starts_where_the_music_begins_it_and_its_lines_follow() {
        use super::Onset::{At, Breath, Follows};
        use super::Pause;
        // The arrangement asserted in the first test: lines 0, 1 stated in slots 0, 1; line 2
        // first arrives in slot 2, line 3 in slot 5.
        let (poem, score) = lowered(TRANSFORMING);
        let plan = |variant| plan(&poem, &score, &at_120(), &reading(variant)).unwrap();
        // `breath`: stanza 2 waits for the music, slot 2 — not the fitted pace's slot 3, a bar
        // after stanza 1's last cue: at the natural pace the breath is measured on the page,
        // from where stanza 1's voice actually stopped.
        let breath = plan(Variant::Breath);
        assert_eq!(
            natural(&breath),
            vec![
                (Breath { slot: 0 }, 0),
                (Follows, 1),
                (Breath { slot: 2 }, 2),
                (Follows, 3)
            ]
        );
        // `stanza`: the same starts, waiting only for the line before.
        assert_eq!(
            natural(&plan(Variant::Stanza)),
            vec![
                (At { slot: 0 }, 0),
                (Follows, 1),
                (At { slot: 2 }, 2),
                (Follows, 3)
            ]
        );
        // `arrives`: every line where it is first heard.
        assert_eq!(
            natural(&plan(Variant::Arrives)),
            vec![
                (At { slot: 0 }, 0),
                (At { slot: 1 }, 1),
                (At { slot: 2 }, 2),
                (At { slot: 5 }, 3)
            ]
        );
        assert!(natural(&plan(Variant::Off)).is_empty());
        // Each stanza's last line pauses longest; the others end on a word, so run on.
        let pauses: Vec<Pause> = breath
            .natural()
            .iter()
            .map(|n| n.spoken().pause())
            .collect();
        assert_eq!(
            pauses,
            vec![
                Pause::RunOn,
                Pause::StanzaEnd,
                Pause::RunOn,
                Pause::StanzaEnd
            ]
        );
        let stanzas: Vec<usize> = breath
            .natural()
            .iter()
            .map(|n| n.spoken().stanza())
            .collect();
        assert_eq!(stanzas, vec![0, 0, 1, 1]);
        // The cue pattern carries the onsets: slots 0 and 2, two cycles each at 120.
        let pattern = breath.natural_pattern();
        assert!(
            pattern.ends_with("n(\"<0@2 ~@2 2@2 ~@2 ~@2 ~@2 ~@2 ~@2>\").cpm(120.000)\n"),
            "{pattern}"
        );
        assert!(
            pattern.starts_with(
                "// the reading at the natural pace: a stanza starts in the bar the music begins \
                 it, or a silent bar after the previous stanza's spoken end, and its lines follow \
                 each other (SPEC.md §4.7, candidate; reading = breath)\n"
            ),
            "{pattern}"
        );
        assert!(
            pattern.contains(
                "pace (0.920..1.080, from the line's own seed), then a pause: 0.250 s after a \
                 run-on line, 0.450 after a comma, 0.800 after a full stop, 1.500 at a stanza's end"
            ),
            "{pattern}"
        );
        // Line 3 of the poem, "beA beA", opens stanza 2 in bar 3. It starts at byte 15 —
        // "eA eA\n" is 6 bytes, "teA teA\n" 8 and the blank line 1 — and is 7 long. Its pace is
        // its own seed's, so it is read back rather than worked by hand.
        let pace = super::Pace::of(prosody_core::span::Span::new(15, 22), poem.source()).unwrap();
        assert!(
            pattern.contains(&format!(
                "//   bar 3, a stanza, after a silent bar, line 3 (bytes 15..22): 4 syllables, \
                 pace {}, then 0.250 s (run-on): beA beA\n",
                fmt_param(pace.speed())
            )),
            "{pattern}"
        );
        assert!(
            pattern.contains("//   then line 4 (bytes 23..30): 4 syllables, pace "),
            "{pattern}"
        );
    }

    #[test]
    fn a_spoken_line_counts_the_syllables_of_each_word() {
        // "eA" is two syllables (an unstressed IH, a stressed AA), "teA" two, "beAn" two: the
        // consonants carry none.
        let (poem, score) = lowered("eA teA\nbeAn\n");
        let reading = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let words: Vec<(usize, usize, usize)> = reading.natural()[0]
            .spoken()
            .words()
            .iter()
            .map(|w| (w.span().start(), w.span().end(), w.syllables()))
            .collect();
        assert_eq!(words, vec![(0, 2, 2), (3, 6, 2)]);
        assert_eq!(reading.natural()[0].spoken().syllables(), 4);
        // A word nobody can pronounce still counts one: a voice will say it.
        let (poem, score) = lowered("eA xyz\n");
        let reading = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let counts: Vec<usize> = reading.natural()[0]
            .spoken()
            .words()
            .iter()
            .map(|w| w.syllables())
            .collect();
        assert_eq!(counts, vec![2, 1]);
    }

    #[test]
    fn three_stanzas_at_the_natural_pace_by_hand() {
        use super::Onset::{Breath, Follows};
        // The arrangement asserted above: stanza 1 stated in slots 0-2, stanza 2's first line
        // arrives in 3, stanza 3's in 12. The fitted pace starts stanza 2 at 4 (a bar after
        // slot 2); the natural pace hands slot 3 to the page, which waits as long as stanza 1's
        // voice needs.
        let (poem, score) = lowered(THREE);
        let reading = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        assert_eq!(
            natural(&reading),
            vec![
                (Breath { slot: 0 }, 0),
                (Follows, 1),
                (Follows, 2),
                (Breath { slot: 3 }, 3),
                (Follows, 4),
                (Follows, 5),
                (Breath { slot: 12 }, 6),
                (Follows, 7),
                (Follows, 8)
            ]
        );
        // 24 slots: rests joined as the fitted pattern's are — two after slot 0, eight after 3,
        // eleven after 12.
        assert!(
            reading
                .natural_pattern()
                .ends_with("n(\"<0@2 ~@4 3@2 ~@16 6@2 ~@22>\").cpm(120.000)\n"),
            "{}",
            reading.natural_pattern()
        );
        // The bars on the clock: 24 slots of two cycles, and the piece's length last.
        assert_eq!(reading.bars().len(), 25);
        assert_eq!(reading.bars()[3], 6);
        assert_eq!(reading.bars()[24], 48);
    }

    #[test]
    fn the_palette_moves_no_onset_pace_or_pause() {
        // I3, at the natural pace: only the clock's speed is palette.
        let (poem, score) = lowered(THREE);
        let a = plan(&poem, &score, &at_120(), &Mappings::all()).unwrap();
        let other = Style::parse("+++\ntempo_range = [90, 90]\nscale = \"lydian\"\n+++").unwrap();
        let b = plan(&poem, &score, &other, &Mappings::all()).unwrap();
        assert_eq!(a.natural(), b.natural());
        assert_eq!(a.bars(), b.bars());
        assert_ne!(fmt_param(a.cpm()), fmt_param(b.cpm()));
    }

    #[test]
    fn a_score_from_another_poem_is_refused() {
        let (poem, _) = lowered(TRANSFORMING);
        let (_, other) = lowered("eA eA\n");
        assert!(plan(&poem, &other, &at_120(), &Mappings::all()).is_err());
    }
}
