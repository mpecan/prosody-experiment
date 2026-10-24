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
//! parameter: the rate is fitted so the line fits its bar at the piece's tempo, and the tempo is
//! palette, so a faster palette reads faster. Which voice speaks is the page's.

use std::fmt::Write as _;

use prosody_core::mapping::{Mapping, Mappings, Variant};
use prosody_core::parse::Poem;
use prosody_core::span::Span;
use prosody_core::voice::Score;
use prosody_core::{Cause, Lane};

use crate::fmt_param;
use crate::strudel::{cycles_per_minute, weights};
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

/// One line, spoken in one slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Cue {
    slot: usize,
    line: usize,
    span: Span,
    text: String,
    syllables: usize,
    rate: f64,
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
        self.line
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

    /// How many syllables the line sounds — what the rate is fitted to.
    #[must_use]
    pub const fn syllables(&self) -> usize {
        self.syllables
    }

    /// The speaking rate, `SLOWEST..=FASTEST`, for a speech synthesiser's `rate`. A value only;
    /// print it through [`fmt_param`].
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
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

/// The poem's reading: every line at most once, in the slot the chosen variant puts it in.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    cues: Vec<Cue>,
    unread: Vec<Unread>,
    slots: usize,
    cpm: f64,
    variant: Variant,
    /// How many cycles each slot lasts — the music's own ([`weights`]) — or `None` under the
    /// elastic pulse, where every slot is one cycle and the cue pattern is written as it always
    /// was.
    weights: Option<Vec<u64>>,
}

impl Reading {
    /// Every cue, in playing order — which is reading order.
    #[must_use]
    pub fn cues(&self) -> &[Cue] {
        &self.cues
    }

    /// The lines left unread, in reading order, and why: empty when every line is read, and
    /// always empty under `off`, where nothing is read by choice.
    #[must_use]
    pub fn unread(&self) -> &[Unread] {
        &self.unread
    }

    /// The cue pattern, as Strudel text: one element per slot, the spoken line's index where a
    /// line is first heard and a rest elsewhere — weighted as the music's bars are under the
    /// steady pulse (`<0@3 ~@4>`), one a cycle under the elastic — at the pattern's own cycles per
    /// minute, so each cue falls on the first beat of its bar.
    ///
    /// The page evaluates this, attaches its speaking callback with `.onTrigger(…, true)` — the
    /// `true` keeps Strudel's default output from trying to sound the cue — and stacks it with the
    /// music. Comment lines first, so the pattern says where every cue came from.
    #[must_use]
    pub fn pattern(&self) -> String {
        let what = match self.variant {
            Variant::Breath => {
                "each stanza read through a line a bar, a silent bar after the last, never before \
                 the music begins it"
            }
            Variant::Stanza => "each stanza read a line a bar from where it is first heard",
            Variant::Off => "switched off, nothing spoken",
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
            | Variant::Light => "each line spoken in the bar it is first heard",
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
        if !self.unread.is_empty() {
            let _ = writeln!(
                out,
                "// {} of {} lines not read:",
                self.unread.len(),
                self.unread.len() + self.cues.len(),
            );
            for unread in &self.unread {
                let _ = writeln!(
                    out,
                    "//   line {} (bytes {}..{}) is not read, {}: {}",
                    unread.line + 1,
                    unread.span.start(),
                    unread.span.end(),
                    unread.why.says(),
                    comment_safe(&unread.text)
                );
            }
        }
        for cue in &self.cues {
            let _ = writeln!(
                out,
                "//   bar {} speaks line {} (bytes {}..{}), {} syllables at rate {}: {}",
                cue.slot + 1,
                cue.line + 1,
                cue.span.start(),
                cue.span.end(),
                cue.syllables,
                fmt_param(cue.rate),
                comment_safe(&cue.text)
            );
        }
        let mut steps = vec!["~".to_owned(); self.slots.max(1)];
        for cue in &self.cues {
            if let Some(step) = steps.get_mut(cue.slot) {
                *step = cue.line.to_string();
            }
        }
        // The music's bars, as long as the music's: under the steady pulse a slot lasts its
        // beats in cycles, and a cue is weighted exactly as the music's bar is
        // (`strudel::sequence`), so it fires on that bar's first beat.
        if let Some(weights) = &self.weights {
            for (step, weight) in steps.iter_mut().zip(weights) {
                let _ = write!(step, "@{weight}");
            }
        }
        let _ = writeln!(
            out,
            "n(\"<{}>\").cpm({})",
            steps.join(" "),
            fmt_param(self.cpm)
        );
        out
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

    let arrivals = arrivals(score);
    let placed: Vec<(usize, usize)> = match variant {
        Variant::Off => Vec::new(),
        Variant::Stanza => by_stanza(poem, &arrivals, slots),
        Variant::Breath => by_breath(poem, &arrivals, slots),
        // As above: only the reading's own variants can reach here.
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
        | Variant::Light => arrivals.clone(),
    };

    let mut cues = Vec::new();
    for (slot, bar) in placed {
        let Some(line) = lines.get(bar) else {
            continue;
        };
        let Some(text) = line.span().text(poem.source()) else {
            continue;
        };
        // What the line sounds is what the rate fits; a line of words nobody could pronounce still
        // has words, and a synthesiser will say them, so it counts one each.
        let sounded = score
            .events()
            .iter()
            .filter(|e| e.bar().get() == bar)
            .count();
        let syllables = if sounded == 0 {
            line.words().count().max(1)
        } else {
            sounded
        };
        cues.push(Cue {
            slot,
            line: bar,
            span: line.span(),
            text: text.to_owned(),
            syllables,
            rate: fitted(syllables, bar_seconds(slot)),
        });
    }
    let unread = if variant == Variant::Off {
        Vec::new()
    } else {
        unread(poem, &lines, &cues, &arrivals)
    };
    Ok(Reading {
        cues,
        unread,
        slots,
        cpm,
        variant,
        weights: (score.pulse() != Variant::Elastic).then_some(all_weights),
    })
}

/// The lines of `lines` no cue speaks, and why: a line whose stanza is read in part, or which
/// arrives, lost its turn to the end of the piece; any other never arrives at all.
fn unread(
    poem: &Poem,
    lines: &[&prosody_core::parse::Line],
    cues: &[Cue],
    arrivals: &[(usize, usize)],
) -> Vec<Unread> {
    let read: std::collections::BTreeSet<usize> = cues.iter().map(|cue| cue.line).collect();
    let arrived: std::collections::BTreeSet<usize> =
        arrivals.iter().map(|(_, line)| *line).collect();
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

    use super::{Cue, FASTEST, SLOWEST, Why, by_breath, by_stanza, comment_safe, fitted, plan};
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
        let pattern = plan(&poem, &score, &at_120(), &Mappings::all())
            .unwrap()
            .pattern();
        assert!(
            pattern.ends_with(
                "n(\"<0@2 1@2 2@2 ~@2 3@2 4@2 5@2 ~@2 ~@2 ~@2 ~@2 ~@2 6@2 7@2 8@2 ~@2 ~@2 ~@2 ~@2 \
                 ~@2 ~@2 ~@2 ~@2 ~@2>\").cpm(120.000)\n"
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
        let cue = |line: usize| {
            let span = lines[line].span();
            Cue {
                slot: line,
                line,
                span,
                text: span.text(poem.source()).unwrap().to_owned(),
                syllables: 1,
                rate: 1.0,
            }
        };
        // Lines 0 and 2 read. Line 1 arrived and its stanza was read: the piece ended first.
        // Line 3 never arrived, but its stanza was read, so it too lost its turn to the end. Stanza
        // 3 (lines 4 and 5) has no cue and no arrival: it never arrives.
        let left = super::unread(&poem, &lines, &[cue(0), cue(2)], &[(0, 0), (1, 1), (4, 2)]);
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
    fn a_score_from_another_poem_is_refused() {
        let (poem, _) = lowered(TRANSFORMING);
        let (_, other) = lowered("eA eA\n");
        assert!(plan(&poem, &other, &at_120(), &Mappings::all()).is_err());
    }
}
