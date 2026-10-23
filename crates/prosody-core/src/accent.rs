//! Light function words: `SPEC.md` §4.2's "stress → accent", where the stress is the dictionary's
//! and the meter disagrees.
//!
//! A pronunciation dictionary gives most one-syllable words primary stress, function words
//! included: `CMUdict` has "or" `AO1 R`, "thy" `DH AY1`, "what" `W AH1 T`. Lowered as they stand,
//! they are full-force accents, often off the beat — in The Tyger's "What immortal hand or eye"
//! the "or" is struck as hard as "hand" and "eye". Stress in a line is not lexical stress: a
//! closed-class monosyllable is stressed in verse when the metre needs it there, and is otherwise
//! weak. This step lets the meter's scansion decide.
//!
//! It is a selector, [`Mapping::Function`](crate::mapping::Mapping::Function) — *decided by the
//! user as a variant to compare, `PLAN.md`*. Under [`Variant::Full`] (the default) nothing here
//! runs and every syllable keeps its dictionary stress, as it always has.
//!
//! # The rule, under [`Variant::Light`]
//!
//! A syllable is struck lightly — [`Velocity::SECONDARY`], the middle rung, "a light accent" —
//! when all of these hold:
//!
//! - its word is a **function word**, as the lexicon says
//!   ([`Lexicon::is_function_word`](crate::surprisal::Lexicon::is_function_word));
//! - the word has **one syllable** in the line (a two-syllable "upon" has a stress pattern of its
//!   own, which the dictionary gets right);
//! - it carries **primary** stress (secondary and unstressed are already light);
//! - the meter does **not** put it on a strong position.
//!
//! A *strong position* is read from the meter's own scansion, never re-scanned: the foot it fell
//! in ([`Event::foot`]) and where in that foot it sits ([`Event::subdivision`]). A position is
//! strong where the foot's template ([`FootKind::template`]) is stressed — the second syllable of
//! an iamb, the first of a trochee or dactyl, the third of an anapest, a single's only syllable.
//! So "of" in "In the forests of the night", which the meter scans as the strong half of an iamb
//! ("-ests OF"), keeps its accent: the line needs it there, and the poet put it there.
//!
//! **The one case read further is the spondee.** The meter scans two adjacent stresses as one
//! spondee, one beat (§4.2's "foot → beat") with both positions stressed — and a dictionary
//! stress on a function word next to a content word is exactly what makes one: "hand or" is a
//! spondee only because `CMUdict` stresses "or". A beat carries one accent, so a function word in
//! a spondee keeps it only if the spondee's strong position is its own, and that is read from the
//! rest of the meter's scansion of the same line:
//!
//! - partnered by a syllable without primary stress, it is the beat's only stress, and keeps it;
//! - in a **falling** line — more trochees and dactyls than iambs and anapests among its feet —
//!   the strong position is the first, on the beat: "What immortal hand or eye" scans trochee,
//!   trochee, spondee, single, so "hand" keeps the accent and "or" is light;
//! - in a **rising** line — more iambs and anapests — it is the second: "Did he who made the Lamb
//!   make thee?" has an iamb and no falling foot, so the rhyme word "thee" keeps its accent and
//!   it is "Did" that is light;
//! - in a line with no direction (as many of each, or neither), the function word yields to a
//!   partner that is not one to lighten; when both are, the one on the beat keeps it.
//!
//! No foot is changed, so no event moves: only gains change.
//!
//! # Every consumer reads it
//!
//! It runs right after the rhythm and before the bass, changing [`Event::velocity`] on the score
//! itself: the Strudel gains, the OSC velocities, the height bass (which plays primary-stressed
//! foundation syllables only, so a lightened "or" loses its bass note), W2's strongest syllable,
//! and `prosody-score`'s accents (primary force) all read that one value.
//!
//! # Invariants
//!
//! The decision for a syllable reads only its own word and its own foot — never another line — so
//! an edit to stanza *n* leaves stanza *m*'s velocities as they were (I2). Nothing about it is
//! palette (I3): the style never sees it. Every lightened event is the same event, with the same
//! span (I4), and says so ([`Event::is_light`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::mapping::Variant;
use crate::meter::FootKind;
use crate::span::Span;
use crate::surprisal::PoemSurprisal;
use crate::voice::{Event, Velocity};

/// Strikes lightly every syllable the light variant lightens; under any other variant, nothing.
pub(crate) fn run(events: &mut [Event], surprisal: &PoemSurprisal, variant: Variant) {
    if variant != Variant::Light {
        return;
    }
    let function: BTreeSet<Span> = surprisal
        .words()
        .iter()
        .filter(|word| word.is_function_word())
        .map(crate::surprisal::WordSurprisal::span)
        .collect();
    let mut syllables: BTreeMap<Span, usize> = BTreeMap::new();
    for event in events.iter() {
        *syllables.entry(event.span()).or_insert(0) += 1;
    }
    let candidate =
        |event: &Event| candidate(event, &function, syllables.get(&event.span()).copied());

    // Decided on the unchanged events, then applied: a spondee's verdict reads its partner's
    // velocity, which must be the dictionary's whichever of the two is visited first.
    let light: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| candidate(event) && !strong(event, events, &candidate))
        .map(|(index, _)| index)
        .collect();
    for index in light {
        if let Some(event) = events.get_mut(index) {
            *event = event.lightened();
        }
    }
}

/// Whether `event` is a syllable the light variant may lighten: a primary-stressed, one-syllable
/// function word. Where it sits in the metre is [`strong`]'s question.
fn candidate(event: &Event, function: &BTreeSet<Span>, syllables: Option<usize>) -> bool {
    event.velocity() == Velocity::PRIMARY
        && syllables == Some(1)
        && function.contains(&event.span())
}

/// Which way a line's feet lean, from the meter's scansion: toward a stress at the end of the
/// foot, or at its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    /// More iambs and anapests than trochees and dactyls.
    Rising,
    /// More trochees and dactyls than iambs and anapests.
    Falling,
    /// As many of each, or none of either.
    Level,
}

/// The direction of the line `event` is in (a bar is a line), counting each of its feet once.
fn direction(event: &Event, events: &[Event]) -> Direction {
    let feet: BTreeMap<usize, FootKind> = events
        .iter()
        .filter(|other| other.bar() == event.bar())
        .map(|other| (other.beat().get(), other.foot()))
        .collect();
    let rising = feet
        .values()
        .filter(|foot| matches!(foot, FootKind::Iamb | FootKind::Anapest))
        .count();
    let falling = feet
        .values()
        .filter(|foot| matches!(foot, FootKind::Trochee | FootKind::Dactyl))
        .count();
    match rising.cmp(&falling) {
        core::cmp::Ordering::Greater => Direction::Rising,
        core::cmp::Ordering::Less => Direction::Falling,
        core::cmp::Ordering::Equal => Direction::Level,
    }
}

/// Whether the meter's scansion puts `event` on a strong position of its foot (see the module
/// documentation): its foot's template stressed there, and — in a spondee — the spondee's strong
/// position by its line's direction, or by its partner where the line has none.
fn strong(event: &Event, events: &[Event], candidate: &impl Fn(&Event) -> bool) -> bool {
    let within = event.subdivision().index();
    let stressed = event
        .foot()
        .template()
        .get(within)
        .copied()
        .unwrap_or(false);
    if !stressed || event.foot() != FootKind::Spondee {
        return stressed;
    }
    // The other syllable of the same beat of the same line (a bar is a line).
    let partner = events.iter().find(|other| {
        other.bar() == event.bar()
            && other.beat() == event.beat()
            && other.subdivision().index() != within
    });
    let Some(partner) = partner else {
        return true;
    };
    if partner.velocity() != Velocity::PRIMARY {
        return true;
    }
    match direction(event, events) {
        Direction::Rising => within > 0,
        Direction::Falling => within == 0,
        Direction::Level => candidate(partner) && within == 0,
    }
}
