//! How well `SPEC.md` §4.1's rule matches the forms a reader would name, measured on real verse.
//!
//! §4.1 calls role assignment "the load-bearing decision", and it is the one pass whose output a
//! literate reader can check without hearing anything: a ballad stanza is a loop, a page of
//! Whitman is not. So the threshold is set by measurement against verse of known form, not by
//! picking a number that makes the unit tests pass.
//!
//! Ground truth is the form, assigned by hand from the metrical tradition each poem belongs to —
//! independent of the variance the code computes. The regular set is deliberately awkward: it
//! includes common measure, whose lines alternate between four beats and three and which
//! therefore *does* vary. It is also the most loop-worthy stanza in English, so any threshold
//! that excludes it has misunderstood what §4.1 is asking for.
//!
//! # This is a floor and a ceiling, not a score
//!
//! The corpus is small and hand-picked, so "14/14" would be a claim about the corpus rather than
//! about the pass. What it actually establishes is the *window* the threshold may occupy: the
//! most ragged regular form must stay in and the least ragged free-verse one must stay out. That
//! window turns out to be narrow — Coleridge at 0.0364 against Eliot at 0.0661, under a factor of
//! two — and [`known_limitations`] records the forms that fall outside it in both directions.
//!
//! # What this cannot fix
//!
//! Relative variance is permutation-invariant, so it measures dispersion and not period. There
//! are rigidly fixed forms it calls ragged — a limerick, a Sapphic stanza — and no threshold
//! rescues them, because their spread exceeds free verse the pass correctly rejects. That is
//! pinned below rather than hidden, and recorded in `PLAN.md` as a limitation of `SPEC.md` §4.1's
//! rule rather than of this implementation.
//!
//! All texts are long out of copyright.

// Test code may unwrap and index freely. See CLAUDE.md §3.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use prosody_cmudict::CmuDict;
use prosody_core::role::{self, Layer, PERIODIC, StanzaBasis as Basis};
use prosody_core::{Fallback, Poem, meter, repetition};
use prosody_lts::LetterToSound;

/// A stanza of known form.
///
/// What each set expects is a property of the set, not of the case — `REGULAR` must classify on
/// its own regularity, `FREE` must not, and `KNOWN_LIMITATIONS` must not despite being regular
/// verse. Each of the three tests below asserts its own set, so a per-case expectation would only
/// be a second place for the same fact to live.
struct Case {
    poem: &'static str,
    form: &'static str,
}

/// Verse in a regular metre. Every one of these is something you could loop.
const REGULAR: &[Case] = &[
    Case {
        form: "Shakespeare, Sonnet 18 — iambic pentameter quatrain",
        poem: "shall i compare thee to a summers day\n\
               thou art more lovely and more temperate\n\
               rough winds do shake the darling buds of may\n\
               and summers lease hath all too short a date\n",
    },
    Case {
        // Common measure with an expanded third line (10 syllables against 8/7/6), which is why
        // this — not a plain ballad stanza — is the corpus's binding lower constraint.
        form: "Coleridge, The Ancient Mariner — common measure, expanded third line",
        poem: "it is an ancient mariner\n\
               and he stoppeth one of three\n\
               by thy long grey beard and glittering eye\n\
               now wherefore stoppst thou me\n",
    },
    Case {
        form: "Blake, The Tyger — trochaic tetrameter quatrain",
        poem: "tyger tyger burning bright\n\
               in the forests of the night\n\
               what immortal hand or eye\n\
               could frame thy fearful symmetry\n",
    },
    Case {
        form: "Dickinson — common measure",
        poem: "because i could not stop for death\n\
               he kindly stopped for me\n\
               the carriage held but just ourselves\n\
               and immortality\n",
    },
    Case {
        form: "Gray, Elegy — heroic quatrain",
        poem: "the curfew tolls the knell of parting day\n\
               the lowing herd wind slowly oer the lea\n\
               the plowman homeward plods his weary way\n\
               and leaves the world to darkness and to me\n",
    },
    Case {
        form: "Newton, Amazing Grace — common measure hymn",
        poem: "amazing grace how sweet the sound\n\
               that saved a wretch like me\n\
               i once was lost but now am found\n\
               was blind but now i see\n",
    },
    Case {
        // Iambic tetrameter throughout, rhyming AABB — *not* an alternating form. The last line
        // is headless, not trimeter. Mislabelled in an earlier version of this file, which
        // mattered: it looked like a second alternating datum justifying the threshold's lower
        // bound, and the genuinely alternating cases are only Coleridge, Dickinson and Newton.
        form: "Housman, Loveliest of Trees — iambic tetrameter quatrain",
        poem: "loveliest of trees the cherry now\n\
               is hung with bloom along the bough\n\
               and stands about the woodland ride\n\
               wearing white for eastertide\n",
    },
    Case {
        form: "Marlowe — iambic tetrameter quatrain",
        poem: "come live with me and be my love\n\
               and we will all the pleasures prove\n\
               that valleys groves hills and fields\n\
               woods or steepy mountain yields\n",
    },
];

/// Free verse: no *stanza-grain* pulse for §4.1's measure to take.
///
/// Not the same as "no pulse". Whitman's anaphora and Crane's refrain are figures `SPEC.md` §4.3
/// turns into ostinati and loops — they are simply invisible to a measure of line-length spread,
/// which is what this set is about.
const FREE: &[Case] = &[
    Case {
        form: "Whitman, Song of Myself",
        poem: "i celebrate myself and sing myself\n\
               and what i assume you shall assume\n\
               for every atom belonging to me as good belongs to you\n",
    },
    Case {
        form: "Whitman, Out of the Cradle",
        poem: "out of the cradle endlessly rocking\n\
               out of the mockingbirds throat the musical shuttle\n\
               out of the ninth month midnight\n\
               over the sterile sands and the fields beyond where the child leaving his bed wandered alone bareheaded barefoot\n",
    },
    Case {
        form: "Sandburg, Chicago",
        poem: "hog butcher for the world\n\
               tool maker stacker of wheat\n\
               player with railroads and the nations freight handler\n\
               stormy husky brawling\n\
               city of the big shoulders\n",
    },
    Case {
        form: "Eliot, Prufrock",
        poem: "let us go then you and i\n\
               when the evening is spread out against the sky\n\
               like a patient etherised upon a table\n\
               let us go through certain half deserted streets\n\
               the muttering retreats\n",
    },
    Case {
        form: "Lawrence, Snake",
        poem: "a snake came to my water trough\n\
               on a hot hot day and i in pyjamas for the heat\n\
               to drink there\n",
    },
    Case {
        // Truncated to its first stanza deliberately: run whole, its refrains ("do not weep",
        // "war is kind") would be lifted to Foundation by the line-grain rule, which is a
        // different mechanism from the one measured here.
        form: "Crane, War Is Kind — opening stanza",
        poem: "do not weep maiden for war is kind\n\
               because your lover threw wild hands toward the sky\n\
               and the affrighted steed ran on alone\n\
               do not weep\n\
               war is kind\n",
    },
];

/// The layer and basis a case's single stanza gets, plus its measured spreads.
fn classify(case: &Case) -> (Layer, Basis, f64, f64) {
    let poem = Poem::parse(case.poem).unwrap();
    let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
    let meter = meter::scan(&poem, &phonemiser);
    let rep = repetition::analyse(&poem);
    let roles = role::assign(&meter, &rep).unwrap();
    let stanza = &roles.stanzas()[0];
    (
        stanza.layer(),
        stanza.basis(),
        stanza.relative_syllable_variance().as_f64(),
        stanza.relative_foot_variance().as_f64(),
    )
}

/// Regular forms that `SPEC.md` §4.1's rule gets wrong, and cannot be made to get right.
///
/// Each is a fixed, named, thoroughly loop-worthy stanza whose *spread* nonetheless exceeds free
/// verse the pass correctly rejects. They are here so the limitation is measured and visible
/// rather than discovered later: see the module docs and `PLAN.md`.
const KNOWN_LIMITATIONS: &[Case] = &[
    Case {
        form: "Lear — limerick, anapaestic 3/3/2/2/3",
        poem: "there was an old man with a beard\n               who said it is just as i feared\n               two owls and a hen\n               four larks and a wren\n               have all built their nests in my beard\n",
    },
    Case {
        form: "Sapphic stanza — three hendecasyllables and an adonic",
        poem: "all the night sleep came not upon my eyelids\n               shed not dew nor shook nor unclosed a feather\n               yet with lips shut close and with eyes of iron\n               stood and beheld me\n",
    },
];

/// Prints the measured table and pins the window the threshold may occupy.
///
/// Deliberately not reported as an accuracy score. The corpus is hand-picked, so a percentage
/// would grade the corpus; what is worth asserting is that the two sets stay separated and that
/// `PERIODIC` stays between them.
#[test]
fn regular_and_free_verse_stay_separated() {
    println!("{:<58} {:>10} {:>10}  verdict", "form", "syllables", "feet");

    let mut worst_regular = 0.0_f64;
    let mut best_free = f64::MAX;
    for case in REGULAR.iter().chain(FREE).chain(KNOWN_LIMITATIONS) {
        let (_, basis, syllables, feet) = classify(case);
        println!(
            "{:<58} {syllables:>10.4} {feet:>10.4}  {basis:?}",
            case.form
        );
    }
    // A stanza needs *both* spreads under the threshold, so the value that decides each case is
    // its larger spread. The regular set is bounded above by the worst of those, the free set
    // below by the best — comparing anything else would compare the wrong numbers.
    for case in REGULAR {
        let (_, _, syllables, feet) = classify(case);
        worst_regular = worst_regular.max(syllables.max(feet));
    }
    for case in FREE {
        let (_, _, syllables, feet) = classify(case);
        best_free = best_free.min(syllables.max(feet));
    }

    println!("\nregular forms reach up to {worst_regular:.4}");
    println!("free verse comes down to  {best_free:.4}");
    println!("threshold                 {:.4}", PERIODIC.as_f64());

    assert!(
        worst_regular < best_free,
        "the two sets overlap: regular reaches {worst_regular:.4}, free verse {best_free:.4} — \
         no threshold can separate them"
    );
    assert!(
        worst_regular < PERIODIC.as_f64() && PERIODIC.as_f64() < best_free,
        "PERIODIC {:.4} is outside the window ({worst_regular:.4}, {best_free:.4})",
        PERIODIC.as_f64()
    );
}

/// The forms §4.1's measure cannot reach, pinned so the gap stays visible.
///
/// If one of these starts passing, the pass has learned something about period and the module
/// docs, `PLAN.md` and this test must change together. If a *new* form joins them, that is worth
/// knowing too.
#[test]
fn known_limitations() {
    for case in KNOWN_LIMITATIONS {
        let (_, basis, syllables, feet) = classify(case);
        assert_ne!(
            basis,
            Basis::Regularity,
            "{} now classifies as regular ({syllables:.4}/{feet:.4}) — update the docs",
            case.form
        );
        // ...and no threshold would fix it. Admitting this form means setting the threshold at
        // or above its larger spread, and some free-verse case the pass correctly rejects has a
        // smaller one — so the same threshold would admit that too.
        let tightest_free = FREE
            .iter()
            .map(|c| {
                let (_, _, s, f) = classify(c);
                s.max(f)
            })
            .fold(f64::MAX, f64::min);
        assert!(
            syllables.max(feet) > tightest_free,
            "{} at {:.4} is tighter than the tightest free verse at {tightest_free:.4} — a \
             threshold could admit it after all",
            case.form,
            syllables.max(feet)
        );
    }
}

#[test]
fn common_measure_is_not_excluded_by_the_threshold() {
    // The awkward case that constrains the threshold from below. A ballad alternates four beats
    // and three, so it varies — and it is the most loop-worthy stanza in English.
    for case in REGULAR {
        let (_, basis, syllables, feet) = classify(case);
        assert_eq!(
            basis,
            Basis::Regularity,
            "{} measured {syllables:.4}/{feet:.4}, threshold {:.4}",
            case.form,
            PERIODIC.as_f64()
        );
    }
}

#[test]
fn free_verse_is_promoted_rather_than_left_without_a_pulse() {
    // SPEC.md §9: with no regular stanza the whole piece would be ornament. The most regular
    // stanza anchors, and the role says the comparison was poem-wide.
    for case in FREE {
        let (layer, basis, ..) = classify(case);
        assert_eq!(layer, Layer::Foundation, "{}", case.form);
        assert_eq!(basis, Basis::MostRegular, "{}", case.form);
    }
}

#[test]
fn editing_one_stanza_leaves_another_stanzas_role_alone() {
    // Structural locality, the analogue of invariant I2 for roles. Two regular stanzas: rewriting
    // the first into free verse must not disturb the second, which is byte-identical.
    let before = "shall i compare thee to a summers day\n\
                  thou art more lovely and more temperate\n\
                  \n\
                  the curfew tolls the knell of parting day\n\
                  the lowing herd wind slowly oer the lea\n";
    let after = "a\n\
                 and what i assume you shall assume for every atom belonging to me\n\
                 \n\
                 the curfew tolls the knell of parting day\n\
                 the lowing herd wind slowly oer the lea\n";

    let of = |source: &str| {
        let poem = Poem::parse(source.to_owned()).unwrap();
        let phonemiser = Fallback::new(CmuDict::new(), LetterToSound::new()).unwrap();
        let meter = meter::scan(&poem, &phonemiser);
        let rep = repetition::analyse(&poem);
        let roles = role::assign(&meter, &rep).unwrap();
        let stanza = &roles.stanzas()[1];
        (
            stanza.layer(),
            stanza.basis(),
            stanza.relative_syllable_variance(),
        )
    };

    assert_eq!(of(before), of(after));
    assert_eq!(of(before).0, Layer::Foundation);
}
