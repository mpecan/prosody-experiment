//! Drum kit articulation: `SPEC.md` §4.1 and §4.2, across the kit rather than one piece per layer.
//!
//! Under [`Variant::Flat`] (the default), every syllable of a [`Layer::Foundation`] voice plays the
//! foundation's steady pulse ([`Drum::Pulse`] — kick in the machine kit, ride in the jazz kit) and
//! every syllable of a [`Layer::Flourish`] voice plays the flourish's flat ornament
//! ([`Drum::Ornament`] — closed hat in the machine kit, snare in the jazz kit). A trochee (`'x x`)
//! and an iamb (`x 'x`) in a foundation bar therefore play the same two kicks (`[bd bd]`),
//! differing only in gain (`0.50` against `0.21`).
//!
//! Under [`Variant::Articulated`], each syllable's drum piece is chosen from its voice's
//! [`Layer`], its beat's parity (`beat % 2`, downbeat against backbeat) and the stress distribution
//! within its foot ([`Event::velocity`]):
//!
//! - **In [`Layer::Foundation`]:**
//!   - Each beat (`(bar, beat)`) has one main accent, which plays [`Drum::Pulse`] on even beats
//!     (`beat % 2 == 0` — kick in the machine kit, ride in the jazz kit) and [`Drum::Backbeat`] on
//!     odd beats (`beat % 2 == 1` — snare in both kits):
//!     - When the beat has at least one [`Velocity::PRIMARY`] syllable, the first primary-stressed
//!       syllable takes that main hit, and any second primary-stressed syllable in the same beat
//!       (a spondee) takes the complementary accent ([`Drum::Backbeat`] on even beats,
//!       [`Drum::Pulse`] on odd beats), so a spondee plays `[bd sd]` or `[sd bd]` rather than two
//!       identical hits.
//!     - When the beat has no primary stress but has a [`Velocity::SECONDARY`] syllable (for
//!       instance after [`crate::accent`] has lightened a function word), the first secondary
//!       syllable takes the beat's main hit.
//!     - When every syllable of the beat is [`Velocity::UNSTRESSED`] (a pyrrhic), the first
//!       syllable on the beat takes [`Drum::Pulse`] so the pulse is not lost.
//!   - Every other syllable in the foundation beat — an iamb's or anapest's unstressed pickup, a
//!     trochee's or dactyl's unstressed tail, or a weak secondary stress alongside a primary one —
//!     plays [`Drum::Subdivision`] (closed hat in both kits).
//!   - Trochaic tetrameter (*The Tyger*) therefore plays `[[bd hh] [sd hh] [bd hh] sd]` and iambic
//!     pentameter (*Sonnet 18*) plays `[[hh bd] [hh sd] [hh bd] [hh sd] [hh bd]]`.
//! - **In [`Layer::Flourish`]:**
//!   - The first [`Velocity::PRIMARY`] syllable of a beat plays [`Drum::HighTom`] on even beats
//!     and [`Drum::Backbeat`] (snare) on odd beats; a second primary stress in the same beat plays
//!     [`Drum::MidTom`] on even beats and [`Drum::LowTom`] on odd beats.
//!   - A [`Velocity::SECONDARY`] syllable plays [`Drum::MidTom`] on even beats and [`Drum::LowTom`]
//!     on odd beats.
//!   - A [`Velocity::UNSTRESSED`] syllable plays [`Drum::Subdivision`] (closed hat in both kits).
//!
//! # Invariants
//!
//! The decision for a syllable reads only its voice's layer and the events of its own beat in its
//! own bar — never another line or stanza — so an edit to stanza *n* leaves stanza *m*'s drum
//! pieces as they were (I2). Which sample name a [`Drum`] becomes (`"bd"`, `"rd"`, `"oh"`, `"sd"`,
//! `"hh"`, `"ht"`, `"mt"`, `"lt"`) belongs to the backend's palette; the role here is structural
//! and unitless, and no event moves (I3). Every event keeps its span (I4).

use std::collections::BTreeMap;

use crate::mapping::Variant;
use crate::repetition::VoiceId;
use crate::role::Layer;
use crate::voice::{Event, Velocity, Voice};

/// Which piece of the drum kit an event plays (`SPEC.md` §4.1, §4.2).
///
/// Structural and unitless, like [`crate::voice::ScaleDegree`]: *which* sample bank and *which*
/// kit (machine or jazz ride) translate this into a sample name belong to the backend's style.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Drum {
    /// The foundation's main pulse: kick in the machine kit, ride (or its stand-in) in the jazz
    /// kit. Under [`Variant::Flat`], every foundation syllable plays this.
    Pulse,
    /// The snare backbeat (`"sd"` in both kits): odd-beat stresses on the foundation and odd-beat
    /// primary stresses on the flourish under [`Variant::Articulated`].
    Backbeat,
    /// An unstressed subdivision (`"hh"` in both kits): pickups and weak tails under
    /// [`Variant::Articulated`].
    Subdivision,
    /// The flourish layer's flat ornament (`"hh"` in the machine kit, `"sd"` in the jazz kit):
    /// played by every flourish syllable under [`Variant::Flat`].
    Ornament,
    /// High tom (`"ht"` in both kits): an even-beat primary stress on the flourish under
    /// [`Variant::Articulated`].
    HighTom,
    /// Mid tom (`"mt"` in both kits): an even-beat secondary stress (or second spondee stress) on
    /// the flourish under [`Variant::Articulated`].
    MidTom,
    /// Low tom (`"lt"` in both kits): an odd-beat secondary stress (or second spondee stress) on
    /// the flourish under [`Variant::Articulated`].
    LowTom,
}

impl Drum {
    /// Its name in the debug view (`SPEC.md` §8).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Pulse => "pulse",
            Self::Backbeat => "backbeat",
            Self::Subdivision => "subdivision",
            Self::Ornament => "ornament",
            Self::HighTom => "high-tom",
            Self::MidTom => "mid-tom",
            Self::LowTom => "low-tom",
        }
    }

    /// The tag name used in explain views (`SPEC.md` §8).
    #[must_use]
    pub const fn annotation(self) -> &'static str {
        match self {
            Self::Pulse => "drums(pulse)",
            Self::Backbeat => "drums(backbeat)",
            Self::Subdivision => "drums(subdivision)",
            Self::Ornament => "drums(ornament)",
            Self::HighTom => "drums(high-tom)",
            Self::MidTom => "drums(mid-tom)",
            Self::LowTom => "drums(low-tom)",
        }
    }

    /// The piece a voice of `layer` plays on every syllable under [`Variant::Flat`].
    #[must_use]
    pub const fn flat_for(layer: Layer) -> Self {
        match layer {
            Layer::Foundation => Self::Pulse,
            Layer::Flourish => Self::Ornament,
        }
    }
}

/// Assigns each event's [`Drum`] from its voice's layer and — under [`Variant::Articulated`] — its
/// beat parity and stress within its foot.
pub(crate) fn run(events: &mut [Event], voices: &[Voice], variant: Variant) {
    if variant != Variant::Articulated {
        for event in events {
            let layer = voices
                .iter()
                .find(|voice| voice.id() == event.voice())
                .map_or(Layer::Flourish, |voice| voice.layer());
            *event = event.with_drum(Drum::flat_for(layer));
        }
        return;
    }

    let layers: BTreeMap<VoiceId, Layer> = voices
        .iter()
        .map(|voice| (voice.id(), voice.layer()))
        .collect();

    // Group contiguous events by `(voice, bar, beat)` so each foot's stress shape is read as a unit
    // without heap-allocating per-beat containers.
    for chunk in events
        .chunk_by_mut(|a, b| a.voice() == b.voice() && a.bar() == b.bar() && a.beat() == b.beat())
    {
        let Some(first) = chunk.first() else {
            continue;
        };
        let backbeat = first.beat().get() % 2 == 1;
        let layer = layers
            .get(&first.voice())
            .copied()
            .unwrap_or(Layer::Flourish);

        match layer {
            Layer::Foundation => articulate_foundation(chunk, backbeat),
            Layer::Flourish => articulate_flourish(chunk, backbeat),
        }
    }
}

/// Articulates one beat of a [`Layer::Foundation`] line.
fn articulate_foundation(events: &mut [Event], backbeat: bool) {
    let main = if backbeat {
        Drum::Backbeat
    } else {
        Drum::Pulse
    };
    let complement = if backbeat {
        Drum::Pulse
    } else {
        Drum::Backbeat
    };

    let has_primary = events
        .iter()
        .any(|event| event.velocity() == Velocity::PRIMARY);
    let has_secondary = events
        .iter()
        .any(|event| event.velocity() == Velocity::SECONDARY);

    if has_primary {
        let mut seen_primary = false;
        for event in events.iter_mut() {
            let drum = if event.velocity() == Velocity::PRIMARY {
                if seen_primary {
                    complement
                } else {
                    seen_primary = true;
                    main
                }
            } else {
                Drum::Subdivision
            };
            *event = event.with_drum(drum);
        }
    } else if has_secondary {
        let mut seen_secondary = false;
        for event in events.iter_mut() {
            let drum = if event.velocity() == Velocity::SECONDARY && !seen_secondary {
                seen_secondary = true;
                main
            } else {
                Drum::Subdivision
            };
            *event = event.with_drum(drum);
        }
    } else {
        for (position, event) in events.iter_mut().enumerate() {
            let drum = if position == 0 {
                Drum::Pulse
            } else {
                Drum::Subdivision
            };
            *event = event.with_drum(drum);
        }
    }
}

/// Articulates one beat of a [`Layer::Flourish`] line.
fn articulate_flourish(events: &mut [Event], backbeat: bool) {
    let mut seen_primary = false;
    for event in events.iter_mut() {
        let drum = match event.velocity() {
            Velocity::PRIMARY => {
                if seen_primary {
                    if backbeat { Drum::LowTom } else { Drum::MidTom }
                } else {
                    seen_primary = true;
                    if backbeat {
                        Drum::Backbeat
                    } else {
                        Drum::HighTom
                    }
                }
            }
            Velocity::SECONDARY => {
                if backbeat {
                    Drum::LowTom
                } else {
                    Drum::MidTom
                }
            }
            _ => Drum::Subdivision,
        };
        *event = event.with_drum(drum);
    }
}
