//! Whether the music listens to the poem: one-word edits, and what each one changed.
//!
//! `SPEC.md` §1's failure mode is "a random generator with a text-shaped seed: output that sounds
//! fine but where no specific audible feature can be traced to a specific word." The other metrics
//! in this crate measure *sounds fine*, and a loop that optimised them alone would drift straight
//! into that failure. This measures the other half — §8's acceptance test, run over every word:
//! swap one word, and ask three things of what changed.
//!
//! - **Audible**: did the edited word's own bar change at all?
//! - **Local**: did nothing outside its stanza change? Invariant I2, read strictly.
//! - **Explained**: did everything that changed outside its stanza have a *reason* to — a bar the
//!   edited bar shared an instrument, a rhyme, a word family or an anchored figure with, or one
//!   that sounds under it in the arrangement?
//!
//! A line "changes" if its own material does or if *where it sounds* does: the arrangement loops a
//! stanza until a later one of its layer takes over (`prosody_core::arrange`), so an edit that
//! moves a later stanza into another layer re-times every loop around it. That is poem-wide by
//! design, and it counts against *explained* unless a link accounts for it.
//!
//! The last is the one an optimiser is held to. Strict locality penalises §4.3's own figures:
//! editing one copy of a refrain *should* change the others, since they were one instrument, and
//! a loop that chased strict locality would learn to write poems without refrains. What it must not
//! do is move a bar that had nothing to do with the edit — `PLAN.md`'s scale degree numbered
//! poem-wide is that case, transposing an untouched stanza — and *explained* counts exactly those.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use prosody_core::voice::{
    Anchor, BeatIndex, Detune, Event, ScaleDegree, Score, Subdivision, Velocity,
};
use prosody_core::{
    Cause, Distance, Layer, Phoneme, Poem, RhymeClassId, Surprisal, TokenKind, VoiceId, WordVoiceId,
};

/// Words of different lengths and stress shapes, every one in `CMUdict`: a monosyllable, an iamb,
/// a trochee and a four-syllable word with its stress inside. A swap to one of these changes a
/// word's shape, which is what §8's synonym swap is meant to exercise.
pub const DEFAULT_PROBES: &[&str] = &["stone", "machine", "thunder", "incandescent"];

/// What a round of one-word edits found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sensitivity {
    edits: usize,
    audible: usize,
    local: usize,
    explained: usize,
    failed: usize,
}

impl Sensitivity {
    /// How many edits were tried: one per word.
    #[must_use]
    pub const fn edits(self) -> usize {
        self.edits
    }

    /// How many changed something in the edited word's own bar.
    #[must_use]
    pub const fn audible(self) -> usize {
        self.audible
    }

    /// How many left every bar outside the edited word's stanza exactly as it was.
    #[must_use]
    pub const fn local(self) -> usize {
        self.local
    }

    /// How many changed nothing outside their stanza except bars linked to the edited one. Every
    /// local edit is explained.
    #[must_use]
    pub const fn explained(self) -> usize {
        self.explained
    }

    /// How many edits produced a poem that would not compile at all.
    #[must_use]
    pub const fn failed(self) -> usize {
        self.failed
    }

    /// Share of edits that were heard where they were made.
    #[must_use]
    pub fn audible_share(self) -> f64 {
        share(self.audible, self.edits)
    }

    /// Share of edits that stayed inside their stanza.
    #[must_use]
    pub fn local_share(self) -> f64 {
        share(self.local, self.edits)
    }

    /// Share of edits whose every effect elsewhere had a reason.
    #[must_use]
    pub fn explained_share(self) -> f64 {
        share(self.explained, self.edits)
    }

    /// Heard where made, and nowhere without a reason: the share an optimiser may not trade away
    /// for a better-sounding piece.
    #[must_use]
    pub fn legibility(self) -> f64 {
        self.audible_share() * self.explained_share()
    }
}

/// Swaps each word of `source` in turn for a probe and compiles the result with `compile`.
///
/// The pipeline is passed in rather than built here: compiling needs a phonemiser and a lexicon,
/// which are concrete English crates this one should not depend on (`CLAUDE.md` §2), and tests
/// want a phonemiser whose every stress they chose.
///
/// The probe for the *n*th word is `probes[n % len]`, skipping one that is the word itself —
/// deterministic, so the same poem always gets the same edits. `None` if the poem does not parse,
/// the original does not compile, or there are no probes.
pub fn measure(
    source: &str,
    probes: &[&str],
    compile: impl Fn(&str) -> Option<Score>,
) -> Option<Sensitivity> {
    if probes.is_empty() {
        return None;
    }
    let poem = Poem::parse(source.to_owned()).ok()?;
    let score = compile(source)?;
    let original = by_bar(&score);
    let links = links(&score);
    let empty = BTreeSet::new();

    let mut result = Sensitivity {
        edits: 0,
        audible: 0,
        local: 0,
        explained: 0,
        failed: 0,
    };
    let mut bar = 0;
    let mut word_index = 0;
    for stanza in poem.stanzas() {
        let stanza_bars = bar..bar + stanza.lines().len();
        for line in stanza.lines() {
            for token in line.tokens().iter().filter(|t| t.kind() == TokenKind::Word) {
                let span = token.span();
                let probe = pick(probes, word_index, span.text(source).unwrap_or_default());
                word_index += 1;
                let (Some(before), Some(after)) =
                    (source.get(..span.start()), source.get(span.end()..))
                else {
                    continue;
                };
                result.edits += 1;
                let Some(edited) = compile(&format!("{before}{probe}{after}")) else {
                    result.failed += 1;
                    continue;
                };
                // A bar's links in the original, plus whatever sounds under it in either version:
                // a loop fitted under a line that grew has changed, and for an obvious reason.
                let mut linked = links.get(bar).unwrap_or(&empty).clone();
                linked.extend(hosted_by(&score, bar));
                linked.extend(hosted_by(&edited, bar));
                let verdict = judge(&original, &by_bar(&edited), bar, &stanza_bars, &linked);
                result.audible += usize::from(verdict.audible);
                result.local += usize::from(verdict.local);
                result.explained += usize::from(verdict.explained);
            }
            bar += 1;
        }
    }
    Some(result)
}

/// What one edit did.
struct Verdict {
    audible: bool,
    local: bool,
    explained: bool,
}

fn judge(
    original: &[Line],
    edited: &[Line],
    bar: usize,
    stanza: &Range<usize>,
    linked: &BTreeSet<usize>,
) -> Verdict {
    let audible = !same_bar(original, edited, bar);
    if original.len() != edited.len() {
        // A bar appeared or vanished: nothing downstream lines up, so nothing can be vouched for.
        return Verdict {
            audible,
            local: false,
            explained: false,
        };
    }
    let changed_elsewhere: Vec<usize> = (0..original.len())
        .filter(|other| !stanza.contains(other))
        .filter(|&other| !same_bar(original, edited, other))
        .collect();
    Verdict {
        audible,
        local: changed_elsewhere.is_empty(),
        explained: changed_elsewhere.iter().all(|other| linked.contains(other)),
    }
}

/// A relation a bar can share with others — the §4.3 and §4.5 figures that legitimately carry an
/// edit from one stanza into another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Link {
    /// The same instrument: a refrain (§4.3).
    Voice(VoiceId),
    /// The same rhyme class (§4.5).
    Rhyme(RhymeClassId),
    /// The same word family (§4.3, morphological row).
    Family(WordVoiceId),
    /// Both carry an anaphora's or an epistrophe's anchor (§4.3). Coarse: the IR does not say
    /// *which* figure, so two unrelated anaphoras link too. It errs towards explaining a change.
    Anchor(Anchor),
}

/// For each bar of `score`, every other bar it shares a [`Link`] with.
fn links(score: &Score) -> Vec<BTreeSet<usize>> {
    let mut bars_of: BTreeMap<Link, BTreeSet<usize>> = BTreeMap::new();
    let mut links_of: Vec<BTreeSet<Link>> = vec![BTreeSet::new(); score.bars()];
    for event in score.events() {
        let bar = event.bar().get();
        let found = [
            Some(Link::Voice(event.voice())),
            event.pitch().map(|pitch| Link::Rhyme(pitch.class())),
            event.family().map(|family| Link::Family(family.voice())),
            event.anchor().map(Link::Anchor),
        ];
        for link in found.into_iter().flatten() {
            bars_of.entry(link).or_default().insert(bar);
            if let Some(set) = links_of.get_mut(bar) {
                set.insert(link);
            }
        }
    }
    links_of
        .iter()
        .map(|links| {
            links
                .iter()
                .filter_map(|link| bars_of.get(link))
                .flatten()
                .copied()
                .collect()
        })
        .collect()
}

/// Every line that sounds in a slot hosted by `bar` — fitted to its length.
fn hosted_by(score: &Score, bar: usize) -> BTreeSet<usize> {
    score
        .arrangement()
        .slots()
        .iter()
        .filter(|slot| slot.host().is_some_and(|host| host.bar().get() == bar))
        .flat_map(|slot| slot.placements().iter().map(|p| p.bar().get()))
        .collect()
}

/// The probe for the `index`th word, never the word itself.
fn pick<'a>(probes: &[&'a str], index: usize, word: &str) -> &'a str {
    let len = probes.len();
    (0..len)
        .filter_map(|offset| probes.get((index + offset) % len))
        .find(|probe| !probe.eq_ignore_ascii_case(word))
        .or_else(|| probes.first())
        .copied()
        .unwrap_or_default()
}

/// One line as it would be heard: its own material, and where in the piece it sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    heard: Vec<Heard>,
    /// Every slot it sounds in, why, and how long that slot lasts — the arrangement's part.
    sounds: Vec<(usize, Cause, usize)>,
}

/// Every line, by its bar in the score. Lines with no events are present and empty.
fn by_bar(score: &Score) -> Vec<Line> {
    let mut plays_in: BTreeMap<VoiceId, BTreeSet<usize>> = BTreeMap::new();
    for event in score.events() {
        plays_in
            .entry(event.voice())
            .or_default()
            .insert(event.bar().get());
    }
    let mut lines = vec![
        Line {
            heard: Vec::new(),
            sounds: Vec::new(),
        };
        score.bars()
    ];
    for event in score.events() {
        if let Some(line) = lines.get_mut(event.bar().get()) {
            line.heard.push(Heard::of(score, event, &plays_in));
        }
    }
    for (index, slot) in score.arrangement().slots().iter().enumerate() {
        for placement in slot.placements() {
            if let Some(line) = lines.get_mut(placement.bar().get()) {
                line.sounds.push((index, placement.cause(), slot.beats()));
            }
        }
    }
    lines
}

fn same_bar(a: &[Line], b: &[Line], bar: usize) -> bool {
    a.get(bar) == b.get(bar)
}

/// What a backend can hear of an event: everything the Strudel or OSC backend renders, and
/// nothing else.
///
/// Two things an event carries are left out on purpose:
///
/// - **Where in the text it came from.** An edit moves every later byte offset, so comparing spans
///   would call every later event changed even when it sounds the same.
/// - **Identity numbers.** A [`VoiceId`], a family's [`WordVoiceId`] and a pitch's
///   [`RhymeClassId`] are all numbered poem-wide by first appearance (`PLAN.md`), so breaking a
///   refrain in stanza 1 renumbers every voice after it — and none of that is heard. The backends
///   play a family's *depth* and a pitch's *degree and detune*, which are compared; and what is
///   heard of a voice is *which bars share its instrument*, so that relation is compared in place
///   of the number. Counting a renumbering as a leak would call silence audible.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Heard {
    /// Every bar this event's instrument plays in, its own included.
    shares_with: Vec<usize>,
    layer: Option<Layer>,
    beat: BeatIndex,
    subdivision: Subdivision,
    velocity: Velocity,
    pitch: Option<(ScaleDegree, Detune)>,
    sharpness: Surprisal,
    depth: Option<Distance>,
    anchor: Option<Anchor>,
    bank: Option<Phoneme>,
}

impl Heard {
    fn of(score: &Score, event: &Event, plays_in: &BTreeMap<VoiceId, BTreeSet<usize>>) -> Self {
        Self {
            shares_with: plays_in
                .get(&event.voice())
                .map(|bars| bars.iter().copied().collect())
                .unwrap_or_default(),
            layer: score.voice(event.voice()).map(|voice| voice.layer()),
            beat: event.beat(),
            subdivision: event.subdivision(),
            velocity: event.velocity(),
            pitch: event.pitch().map(|pitch| (pitch.degree(), pitch.detune())),
            sharpness: event.sharpness(),
            depth: event.family().map(prosody_core::Family::depth),
            anchor: event.anchor(),
            bank: event.bank(),
        }
    }
}

fn share(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    let as_f64 = |count: usize| f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    as_f64(part) / as_f64(whole)
}

#[cfg(test)]
mod tests {
    use super::pick;

    #[test]
    fn probes_rotate_and_never_repeat_the_word() {
        let probes = ["stone", "machine"];
        assert_eq!(pick(&probes, 0, "night"), "stone");
        assert_eq!(pick(&probes, 1, "night"), "machine");
        assert_eq!(pick(&probes, 2, "night"), "stone");
        assert_eq!(
            pick(&probes, 0, "Stone"),
            "machine",
            "the word itself is skipped"
        );
    }
}
