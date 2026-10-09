//! Switching a mapping off, to hear what it does.
//!
//! Some mappings are still candidates: added by a design iteration and marked *pending review* in
//! `SPEC.md`, they change what a poem becomes, and whether they belong is judged by listening. A
//! [`Mappings`] set says which of them a lowering applies, so the same poem can be heard with and
//! without each one, side by side.
//!
//! This is structure, not palette, so it is deliberately **not** part of the style (`SPEC.md` §6,
//! I3: the style may change only parameters). It is a separate argument to
//! [`lower::run_with`](crate::lower::run_with), and every invariant holds within one set: the same
//! poem under the same set is bit-identical, and an edit to one stanza leaves the others' voices
//! alone. [`Mappings::all`] is the piece as specified, and the default everywhere.

use std::collections::BTreeSet;

/// One mapping that can be switched off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mapping {
    /// §4.6's held lane: a line whose rhyme is answered later in its stanza sounds until it is.
    HeldLane,
    /// §4.6's refrain lane: a refrain line loops until the next refrain line takes over.
    RefrainLane,
    /// §4.1's bass: a stressed foundation syllable is pitched by its vowel's height.
    Bass,
    /// §4.6's bar lock to the stanza: a bar lasts its stanza's longest line. Off, a bar lasts its
    /// own line, and whatever sounds under it is stretched or squeezed to fit.
    StanzaPeriod,
    /// §4.6's outro: the loops play one more pass when the poem runs out.
    Outro,
}

impl Mapping {
    /// Every mapping, in the order a page lists them.
    pub const ALL: [Self; 5] = [
        Self::HeldLane,
        Self::RefrainLane,
        Self::Bass,
        Self::StanzaPeriod,
        Self::Outro,
    ];

    /// Its name on a command line or in a query string.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HeldLane => "held",
            Self::RefrainLane => "refrain",
            Self::Bass => "bass",
            Self::StanzaPeriod => "period",
            Self::Outro => "outro",
        }
    }

    /// The mapping called `name`, if there is one.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mapping| mapping.name() == name)
    }

    /// What it does, in a phrase.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::HeldLane => "an open rhyme sounds until its answer",
            Self::RefrainLane => "a refrain line loops until the next one",
            Self::Bass => "stressed foundation vowels play a bass note by height",
            Self::StanzaPeriod => "every bar of a stanza lasts its longest line",
            Self::Outro => "the loops play one more pass at the end",
        }
    }
}

/// Which mappings a lowering applies.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mappings {
    off: BTreeSet<Mapping>,
}

impl Mappings {
    /// Every mapping on: the piece as specified.
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// The same set with `mapping` switched off.
    #[must_use]
    pub fn without(mut self, mapping: Mapping) -> Self {
        self.off.insert(mapping);
        self
    }

    /// Whether `mapping` applies.
    #[must_use]
    pub fn has(&self, mapping: Mapping) -> bool {
        !self.off.contains(&mapping)
    }

    /// The mappings switched off, in [`Mapping::ALL`]'s order.
    pub fn off(&self) -> impl Iterator<Item = Mapping> + '_ {
        self.off.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{Mapping, Mappings};

    #[test]
    fn every_mapping_is_found_by_its_own_name() {
        for mapping in Mapping::ALL {
            assert_eq!(Mapping::named(mapping.name()), Some(mapping));
        }
        assert_eq!(Mapping::named("nonsense"), None);
    }

    #[test]
    fn a_set_has_everything_until_something_is_switched_off() {
        let all = Mappings::all();
        assert!(Mapping::ALL.into_iter().all(|mapping| all.has(mapping)));
        let some = all.without(Mapping::Outro).without(Mapping::Bass);
        assert!(!some.has(Mapping::Outro));
        assert!(!some.has(Mapping::Bass));
        assert!(some.has(Mapping::HeldLane));
        assert_eq!(
            some.off().collect::<Vec<_>>(),
            vec![Mapping::Bass, Mapping::Outro]
        );
    }
}
