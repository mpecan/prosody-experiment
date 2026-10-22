//! Scoring a compiled poem: how interesting it is likely to sound, and whether it still listens to
//! the poem.
//!
//! This is the measuring half of a feedback loop that tunes the generator. It reads a
//! [`Score`] — the structure, before any palette — and reports two kinds of number:
//!
//! - **Interest** ([`measure`]): plain counts over the piece's timeline (layering, recurrence,
//!   melody, syncopation), each judged against a [`Band`]. Bands, not maxima: what listeners find
//!   interesting peaks at *moderate* complexity (Berlyne's inverted U; Witek et al. 2014 for
//!   syncopation specifically), so a metric pushed past its band scores worse, not better.
//! - **Legibility** ([`sensitivity`]): one-word edits, and whether each was heard where it was
//!   made and nowhere else. This is the guard against `SPEC.md` §1's failure mode. An optimiser
//!   that raised interest by ignoring the text would raise one number and sink this one.
//!
//! [`Report::overall`] is their product, so neither can be bought with the other.
//! [`Report::interest`] is itself a geometric mean, for the same reason one level down.
//!
//! # The bands are provisional
//!
//! Each band's edges are a starting guess with a stated reason, not a measurement. The plan is to
//! calibrate them against a listener's pairwise preferences; until then, read the raw values
//! alongside the fits and distrust a fit that disagrees with your ears.
//!
//! # Determinism
//!
//! Every metric is a count over `BTreeMap`s, or a float sum in a fixed order, so the same score
//! always measures the same. Floats appear only as reported values (`CLAUDE.md` §4).

// Tests may unwrap and index freely; production code may not. See CLAUDE.md §3.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)
)]

mod metrics;
pub mod sensitivity;
mod timeline;

use std::fmt::Write as _;

use prosody_core::voice::Score;

pub use sensitivity::{DEFAULT_PROBES, Sensitivity};

use crate::timeline::Timeline;

/// The range a metric should fall in, inclusive.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    low: f64,
    high: f64,
}

impl Band {
    /// A band from `low` to `high`. Crate-internal so that every band is one of the documented
    /// ones below, where `low < high` holds by inspection.
    const fn new(low: f64, high: f64) -> Self {
        Self { low, high }
    }

    /// The lower edge.
    #[must_use]
    pub const fn low(self) -> f64 {
        self.low
    }

    /// The upper edge.
    #[must_use]
    pub const fn high(self) -> f64 {
        self.high
    }

    /// How well `value` sits in the band: 1.0 anywhere inside, and `1 / (1 + d)²` outside, where
    /// `d` is the distance in band-widths. One width out scores ¼, two widths ⅑.
    ///
    /// Quadratic rather than `1 / (1 + d)` because the gentler curve scored a piece that is a
    /// single line of sound — half a width below the layering band, and the defect this crate was
    /// written to catch — at ⅔. It never reaches zero, so an optimiser always has a slope to climb.
    ///
    /// Branch-free: the distance is the sum of how far it falls short and how far it overshoots,
    /// at most one of which is non-zero.
    #[must_use]
    pub fn fit(self, value: f64) -> f64 {
        let outside = (self.low - value).max(0.0) + (value - self.high).max(0.0);
        let near = 1.0 + outside / (self.high - self.low);
        1.0 / (near * near)
    }
}

/// One measurement of a piece, and how it sits in its band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metric {
    name: &'static str,
    value: f64,
    band: Band,
}

impl Metric {
    /// A short, stable name, safe to use as a key in a log.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// The raw value.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.value
    }

    /// The band it is judged against.
    #[must_use]
    pub const fn band(self) -> Band {
        self.band
    }

    /// How well the value sits in the band, from 0.0 to 1.0.
    #[must_use]
    pub fn fit(self) -> f64 {
        self.band.fit(self.value)
    }
}

// The bands. Each says what a value outside it sounds like.

/// Two to four things at once: below two is a single line of sound, the state the README's
/// "percussion with sparse melodic punctuation" describes; above four, at this density of events,
/// the layers stop being separable.
const LAYERING: Band = Band::new(2.0, 4.0);
/// Most bars should bring something back, since a loop is the unit of electronic music; every bar
/// returning means nothing new ever arrives.
const RECURRENCE: Band = Band::new(0.5, 0.9);
/// From a third of events pitched (a bassline under drums) to most of them; all of them leaves no
/// percussion to carry the meter.
const PITCHED: Band = Band::new(0.3, 0.8);
/// Roughly three to eight equally used degrees: fewer is a drone, more stops sounding like a
/// scale being used.
const PITCH_ENTROPY: Band = Band::new(1.5, 3.0);
/// Moderate, per Witek et al. (2014). In these units a line of pure iambs scores about 1.0 and of
/// pure trochees 0.0; the band asks for a mixture.
const SYNCOPATION: Band = Band::new(0.3, 0.7);

// The machine-music profile: Kraftwerk, Daft Punk, Laibach's electronic songs, psytrance. Like
// the bands above these are reasoned guesses, not measurements of those records.

/// A kick, a bass, a riff and something on top; psytrance stacks more, but past five at this
/// density the parts blur.
const MACHINE_LAYERING: Band = Band::new(3.0, 5.0);
/// A loop held for minutes, with a part arriving every eight or sixteen bars: one new (bar, voice)
/// pair in ten to thirty.
const MACHINE_RECURRENCE: Band = Band::new(0.85, 0.97);
/// A bassline and a riff are pitched, the drums are not.
const MACHINE_PITCHED: Band = Band::new(0.35, 0.7);
/// Riffs of two to five notes; a psytrance bass is often one.
const MACHINE_PITCH_ENTROPY: Band = Band::new(1.0, 2.3);
/// A straight machine grid: accents mostly on the beat, the offbeat carried by the bass.
const MACHINE_SYNCOPATION: Band = Band::new(0.1, 0.4);
/// Four on the floor: something lands on nearly every beat.
const MACHINE_PULSE: Band = Band::new(0.9, 1.0);
/// The meter holds; a bar of another length is an event, not the norm.
const MACHINE_STEADINESS: Band = Band::new(0.8, 1.0);

// The jazz profile: a small-group swing or modal record — a piano trio, a quartet. Reasoned
// guesses again, each against the machine profile it most differs from, and not measurements.
//
// Swing is not measured and cannot be: the scorer reads the score, before any palette, and swing is
// palette (`SPEC.md` §5) — every onset is still where the score put it. A swung piece therefore
// scores exactly as its straight twin does, which is what "tolerated" means here: nothing in this
// profile penalises it, and nothing rewards it either.

/// A trio is three players and a quartet four; comping, bass, ride and a line on top. Fewer than
/// two at once is a soloist, more than four at this density muddies.
const JAZZ_LAYERING: Band = Band::new(2.0, 4.0);
/// Less looped than machine music: a head returns, the comping returns, the bass walks new ground
/// each bar. Most bars bring something back, but a fifth to a third of what sounds is new.
const JAZZ_RECURRENCE: Band = Band::new(0.65, 0.85);
/// Bass and piano are pitched and only the drums are not, so more of the piece is pitched than in
/// machine music; all of it would leave nothing to carry the time.
const JAZZ_PITCHED: Band = Band::new(0.4, 0.85);
/// Lines that move through a mode: four to ten degrees in use, against the machine's riffs of two
/// to five. Below two bits is a vamp on one chord.
const JAZZ_PITCH_ENTROPY: Band = Band::new(2.0, 3.3);
/// Accents off the beat — anticipations, the offbeat comp, the "and" of a swung eighth — are the
/// idiom, so the band sits above the generic one; the top is where accents stop implying a beat at
/// all (Witek et al.'s inverted U again, shifted).
const JAZZ_SYNCOPATION: Band = Band::new(0.45, 0.9);
/// The ride and the walking bass mark nearly every beat; less strictly than four on the floor,
/// since a comping figure or a break leaves one now and then.
const JAZZ_PULSE: Band = Band::new(0.75, 1.0);
/// A song form keeps its meter, but a turnaround or a tag can stretch a bar.
const JAZZ_STEADINESS: Band = Band::new(0.7, 1.0);
/// Three against two — a comping figure or a line in triplets over the ride's straight or swung
/// pairs, a hemiola across the bar — is a device a small group uses, not the ground it stands on:
/// the ride and the bass keep one shared subdivision and the cross-rhythm is heard against it. So
/// from one beat in ten crossed (below it the piece has one grid throughout) to two in five
/// (above it no subdivision is shared for long enough to be the grid). A reasoned guess, like the
/// rest: no study we know of counts the share of crossed beats in recorded jazz.
const JAZZ_CROSS_RHYTHM: Band = Band::new(0.1, 0.4);

/// What a piece is judged against: which metrics, and which bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Generic interest: the five metrics, each in a moderate band.
    Interest,
    /// Repetitive machine music — Kraftwerk, Daft Punk, Laibach's electronic songs, psytrance:
    /// denser, more repetitive, a steady pulse and short riffs.
    Machine,
    /// Small-group jazz — a piano trio, a modal quartet: fewer loops, more notes in use, more
    /// accents off the beat, some lines crossing the shared grid (three against two), and the time
    /// still kept. Swing is palette and so is not measured.
    Jazz,
}

impl Profile {
    /// Every profile, in the order a menu lists them.
    pub const ALL: [Self; 3] = [Self::Interest, Self::Machine, Self::Jazz];

    /// The profile called `name` on a command line: `interest`, `machine` or `jazz`.
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|profile| profile.name() == name)
    }

    /// Its name on a command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Interest => "interest",
            Self::Machine => "machine",
            Self::Jazz => "jazz",
        }
    }
}

/// Measures a compiled poem for [`Profile::Interest`].
///
/// The order is fixed and part of the output format, so a log of reports can be compared line by
/// line across runs.
#[must_use]
pub fn measure(score: &Score) -> Vec<Metric> {
    measure_as(score, Profile::Interest)
}

/// Measures a compiled poem against `profile`, in a fixed order: the five interest metrics, then
/// whatever the profile adds — pulse and steadiness for both genres, then cross-rhythm for jazz.
#[must_use]
pub fn measure_as(score: &Score, profile: Profile) -> Vec<Metric> {
    let timeline = Timeline::of(score);
    match profile {
        Profile::Interest => interest_metrics(&timeline),
        Profile::Machine => genre_metrics(
            &timeline,
            [
                MACHINE_LAYERING,
                MACHINE_RECURRENCE,
                MACHINE_PITCHED,
                MACHINE_PITCH_ENTROPY,
                MACHINE_SYNCOPATION,
            ],
            [MACHINE_PULSE, MACHINE_STEADINESS],
        ),
        Profile::Jazz => {
            let mut metrics = genre_metrics(
                &timeline,
                [
                    JAZZ_LAYERING,
                    JAZZ_RECURRENCE,
                    JAZZ_PITCHED,
                    JAZZ_PITCH_ENTROPY,
                    JAZZ_SYNCOPATION,
                ],
                [JAZZ_PULSE, JAZZ_STEADINESS],
            );
            // Jazz only: the machine profile's straight grid has no use for it, and adding it
            // there would move that cycle's numbers for a reason that is not its genre's.
            metrics.push(Metric {
                name: "cross_rhythm",
                value: metrics::cross_rhythm(&timeline),
                band: JAZZ_CROSS_RHYTHM,
            });
            metrics
        }
    }
}

/// A genre profile: the five interest metrics under the genre's own bands, then pulse and
/// steadiness under its bands for those.
fn genre_metrics(
    timeline: &Timeline,
    bands: [Band; 5],
    [pulse, steadiness]: [Band; 2],
) -> Vec<Metric> {
    let mut metrics: Vec<Metric> = interest_metrics(timeline)
        .into_iter()
        .zip(bands)
        .map(|(metric, band)| Metric { band, ..metric })
        .collect();
    metrics.push(Metric {
        name: "pulse",
        value: metrics::pulse(timeline),
        band: pulse,
    });
    metrics.push(Metric {
        name: "steadiness",
        value: metrics::steadiness(timeline),
        band: steadiness,
    });
    metrics
}

fn interest_metrics(timeline: &Timeline) -> Vec<Metric> {
    vec![
        Metric {
            name: "layering",
            value: metrics::layering(timeline),
            band: LAYERING,
        },
        Metric {
            name: "recurrence",
            value: metrics::recurrence(timeline),
            band: RECURRENCE,
        },
        Metric {
            name: "pitched",
            value: metrics::pitched(timeline),
            band: PITCHED,
        },
        Metric {
            name: "pitch_entropy",
            value: metrics::pitch_entropy(timeline),
            band: PITCH_ENTROPY,
        },
        Metric {
            name: "syncopation",
            value: metrics::syncopation(timeline),
            band: SYNCOPATION,
        },
    ]
}

/// Everything measured about one poem.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    metrics: Vec<Metric>,
    sensitivity: Option<Sensitivity>,
}

impl Report {
    /// A report from a score's metrics and, if it was run, the edit test.
    #[must_use]
    pub fn new(score: &Score, sensitivity: Option<Sensitivity>) -> Self {
        Self::profiled(score, sensitivity, Profile::Interest)
    }

    /// A report against `profile`.
    #[must_use]
    pub fn profiled(score: &Score, sensitivity: Option<Sensitivity>, profile: Profile) -> Self {
        Self {
            metrics: measure_as(score, profile),
            sensitivity,
        }
    }

    /// The metrics, in [`measure`]'s order.
    #[must_use]
    pub fn metrics(&self) -> &[Metric] {
        &self.metrics
    }

    /// The edit test, if it was run.
    #[must_use]
    pub const fn sensitivity(&self) -> Option<Sensitivity> {
        self.sensitivity
    }

    /// The geometric mean of the metrics' fits, from 0.0 to 1.0.
    ///
    /// Geometric, so a piece is only as interesting as its weakest dimension: an arithmetic mean
    /// let four passable metrics hide a piece with no layers at all. Summed in log space in
    /// [`measure`]'s fixed order (`CLAUDE.md` §4).
    #[must_use]
    pub fn interest(&self) -> f64 {
        let logs: f64 = self.metrics.iter().map(|metric| metric.fit().ln()).sum();
        let count = u32::try_from(self.metrics.len()).unwrap_or(u32::MAX).max(1);
        (logs / f64::from(count)).exp()
    }

    /// Interest times legibility; interest alone if the edit test was not run.
    #[must_use]
    pub fn overall(&self) -> f64 {
        self.interest() * self.sensitivity.map_or(1.0, Sensitivity::legibility)
    }

    /// The report as aligned text, one `name value` pair per line so a script can read it with
    /// `awk`. Every number is printed to three decimals, so the text is byte-stable.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "{:<16}{:>8}  {:<14}{:>6}",
            "metric", "value", "band", "fit"
        );
        for metric in &self.metrics {
            let band = format!("{}..{}", fixed(metric.band.low), fixed(metric.band.high));
            let _ = writeln!(
                out,
                "{:<16}{:>8}  {band:<14}{:>6}",
                metric.name,
                fixed(metric.value),
                fixed(metric.fit())
            );
        }
        let _ = writeln!(out, "{:<16}{:>8}", "interest", fixed(self.interest()));
        if let Some(sensitivity) = self.sensitivity {
            let _ = writeln!(
                out,
                "{:<16}{:>8}  {} of {} edits",
                "audible",
                fixed(sensitivity.audible_share()),
                sensitivity.audible(),
                sensitivity.edits()
            );
            let _ = writeln!(
                out,
                "{:<16}{:>8}  {} of {} edits",
                "local",
                fixed(sensitivity.local_share()),
                sensitivity.local(),
                sensitivity.edits()
            );
            let _ = writeln!(
                out,
                "{:<16}{:>8}  {} of {} edits",
                "explained",
                fixed(sensitivity.explained_share()),
                sensitivity.explained(),
                sensitivity.edits()
            );
            if sensitivity.failed() > 0 {
                let _ = writeln!(out, "{:<16}{:>8}", "failed_edits", sensitivity.failed());
            }
            let _ = writeln!(
                out,
                "{:<16}{:>8}",
                "legibility",
                fixed(sensitivity.legibility())
            );
        }
        let _ = writeln!(out, "{:<16}{:>8}", "overall", fixed(self.overall()));
        out
    }
}

/// Three decimals, with negative zero normalised — the same rule as `prosody_emit::fmt_param`,
/// which this crate does not depend on because it scores structure, not a realisation.
fn fixed(value: f64) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    format!("{value:.3}")
}

#[cfg(test)]
mod tests {
    use super::{Band, Metric, Profile, Report, fixed};

    #[test]
    fn inside_the_band_fits_perfectly() {
        let band = Band::new(2.0, 4.0);
        assert_eq!(fixed(band.fit(2.0)), "1.000");
        assert_eq!(fixed(band.fit(3.0)), "1.000");
        assert_eq!(fixed(band.fit(4.0)), "1.000");
    }

    #[test]
    fn fit_falls_by_band_widths_on_either_side() {
        // Width 2. One width below (0.0) or above (6.0): 1/2² = ¼. Two widths above: 1/3² = ⅑.
        let band = Band::new(2.0, 4.0);
        assert_eq!(fixed(band.fit(0.0)), "0.250");
        assert_eq!(fixed(band.fit(6.0)), "0.250");
        assert_eq!(fixed(band.fit(8.0)), "0.111");
        // A single line of sound against the layering band: half a width below. 1/1.5² = 4/9.
        assert_eq!(fixed(band.fit(1.0)), "0.444");
    }

    #[test]
    fn interest_is_only_as_good_as_the_weakest_metric() {
        // Fits 1 and ¼: the geometric mean is √¼ = ½, where an arithmetic mean would say ⅝.
        let report = Report {
            metrics: vec![
                Metric {
                    name: "inside",
                    value: 3.0,
                    band: Band::new(2.0, 4.0),
                },
                Metric {
                    name: "one_width_out",
                    value: 0.0,
                    band: Band::new(2.0, 4.0),
                },
            ],
            sensitivity: None,
        };
        assert_eq!(fixed(report.interest()), "0.500");
        assert_eq!(fixed(report.overall()), "0.500", "no edit test, no penalty");
    }

    #[test]
    fn every_profile_is_found_by_its_own_name() {
        for profile in Profile::ALL {
            assert_eq!(Profile::named(profile.name()), Some(profile));
        }
        assert_eq!(Profile::named("jazz"), Some(Profile::Jazz));
        assert_eq!(Profile::named("polka"), None);
    }

    #[test]
    fn jazz_wants_less_recurrence_and_more_syncopation_than_machine_music() {
        // The point of the profile, as fits of the same values: a loop held almost unbroken
        // (recurrence 0.95) is ideal machine music and half a band-width too repetitive for jazz —
        // (0.95 − 0.85) / (0.85 − 0.65) = ½, so 1 / 1.5² = 0.444; and accents mostly off
        // the beat (syncopation 0.6) are ideal jazz and, against machine's 0.1..0.4, two thirds of
        // a width too many: 1 / (1 + ⅔)² = 0.36.
        assert_eq!(fixed(super::MACHINE_RECURRENCE.fit(0.95)), "1.000");
        assert_eq!(fixed(super::JAZZ_RECURRENCE.fit(0.95)), "0.444");
        assert_eq!(fixed(super::JAZZ_SYNCOPATION.fit(0.6)), "1.000");
        assert_eq!(fixed(super::MACHINE_SYNCOPATION.fit(0.6)), "0.360");
    }

    #[test]
    fn a_piece_on_one_grid_falls_short_of_jazz_cross_rhythm() {
        // A piece almost all on one grid (The Tyger measures 0.031) is about a quarter of a
        // band-width short of jazz's 0.1..0.4: (0.1 − 0.031) / 0.3 = 0.23, so 1 / 1.23² = 0.661.
        assert_eq!(fixed(super::JAZZ_CROSS_RHYTHM.fit(0.031)), "0.661");
        assert_eq!(fixed(super::JAZZ_CROSS_RHYTHM.fit(0.3)), "1.000");
    }

    #[test]
    fn negative_zero_prints_as_zero() {
        assert_eq!(fixed(-0.0), "0.000");
    }
}
