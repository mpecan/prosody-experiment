//! Property tests for the invariants in `SPEC.md` §6.
//!
//! These are written against the invariants themselves rather than against any one pass, and
//! they grow as the pipeline does. They currently cover I1 (determinism of seeding and of
//! incremental reparse), I2 (per-unit seeding, and locality of a real edit across stanzas) and
//! I4 (spans resolve, nest and are attributable). I3 arrives with the style layer; see `PLAN.md`.
//!
//! Every property here must be able to fail. A property that holds by virtue of a function
//! signature rather than by virtue of the implementation is not a test, and two such properties
//! were removed from this file after review rather than left in to inflate the count.

// Property-test bodies unwrap and index freely, as test code may. See CLAUDE.md §3.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use proptest::prelude::{
    Strategy, prop_assert, prop_assert_eq, prop_assert_ne, prop_assume, proptest,
};
use prosody_core::phoneme::arpabet::ARPABET;
use prosody_core::phoneme::{Inventory, Phonemiser, Pronunciation, Unpronounceable};
use prosody_core::{Domain, Phoneme, Poem, Seed, Span, StanzaIdx, Stress, meter, phoneme};

/// A phonemiser that gives every word one stressed syllable, so meter properties depend on the
/// segmentation rather than on a dictionary.
struct EverySyllableStressed;

impl Phonemiser for EverySyllableStressed {
    fn inventory(&self) -> &'static Inventory {
        &ARPABET
    }

    fn pronounce(&self, word: &str) -> Result<Pronunciation, Unpronounceable> {
        // One syllable per vowel letter, alternating stress, so lines get varied stress vectors.
        let count = word.chars().filter(|c| "aeiou".contains(*c)).count();
        if count == 0 {
            return Err(Unpronounceable::NotKnown);
        }
        let nucleus = ARPABET.phoneme("AH").unwrap();
        let syllables = (0..count)
            .map(|i| {
                let stress = if i % 2 == 0 {
                    Stress::Primary
                } else {
                    Stress::Unstressed
                };
                prosody_core::Syllable::new(vec![], nucleus, vec![], stress).unwrap()
            })
            .collect();
        Ok(Pronunciation::from_dictionary(syllables))
    }
}
use rand_core::Rng;

/// A well-formed poem: stanzas of lines of words, blank-line separated.
fn poem_source() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        proptest::collection::vec(proptest::collection::vec("[a-zšč]{1,6}", 1..4), 1..4),
        1..4,
    )
    .prop_map(|stanzas| {
        let rendered: Vec<String> = stanzas
            .iter()
            .map(|lines| {
                lines
                    .iter()
                    .map(|words| words.join(" "))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect();
        format!("{}\n", rendered.join("\n\n"))
    })
}

/// Every stanza's seed, in order.
fn stanza_seeds(poem: &Poem) -> Vec<Seed> {
    (0..poem.stanzas().len())
        .filter_map(|i| poem.stanza_seed(StanzaIdx(i)))
        .collect()
}

/// Every token span in the poem, in source order.
fn token_spans(poem: &Poem) -> Vec<Span> {
    poem.stanzas()
        .iter()
        .flat_map(prosody_core::Stanza::lines)
        .flat_map(prosody_core::Line::tokens)
        .copied()
        .map(prosody_core::Token::span)
        .collect()
}

/// Replacement texts an edit might use.
///
/// The point of the awkward ones is coverage the earlier version of this file lacked: an empty
/// string deletes, `a\nb` inserts a line break, `\n\n` splits a stanza, and `gorči` is
/// multi-byte. Without those, every generated edit stayed inside one line, and the row/column
/// arithmetic in `Poem::apply` was never required to be right — a deliberately broken
/// `point_of` passed the whole property suite.
fn replacement() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        "", "x", "quenched", "gorči", "a\nb", "\n\n", "  ", "ó",
    ])
}

/// The largest byte offset the span strategies draw. Chosen to sit somewhat beyond the longest
/// source `poem_text()` can produce, so that in-bounds, boundary and out-of-bounds spans all
/// occur at usable rates.
const MAX_OFFSET: usize = 200;

/// Text shaped roughly like a stanza: words, spaces and newlines, plus multi-byte characters at
/// two, three and four bytes wide, so the non-English case from `SPEC.md` §9 is exercised rather
/// than assumed away.
fn poem_text() -> impl Strategy<Value = String> {
    proptest::string::string_regex("[a-zA-Zčšžæø€漢🔥' \n]{0,40}").unwrap()
}

fn domain() -> impl Strategy<Value = Domain> {
    proptest::sample::select(vec![Domain::Stanza, Domain::Line])
}

/// Sub-stream labels must be `'static`, so the strategy selects from a fixed set.
fn label() -> impl Strategy<Value = &'static str> {
    proptest::sample::select(vec![
        "detune",
        "sample-bank",
        "ornament",
        "velocity",
        "filter",
    ])
}

/// Seeds a whole string as one unit.
fn seed_of(domain: Domain, text: &str) -> Seed {
    Seed::for_unit(domain, Span::new(0, text.len()), text).unwrap()
}

fn draw(seed: &Seed, count: usize) -> Vec<u32> {
    let mut rng = seed.rng();
    (0..count).map(|_| rng.next_u32()).collect()
}

/// An arbitrary phone sequence, drawn from the whole `ARPAbet` inventory.
///
/// Arbitrary sequences reach shapes no real word has — clusters English never uses, runs of
/// vowels, consonant-only words — which is the point: the dictionary corpus only exercises
/// sequences that happen to occur in English.
fn phone_sequence() -> impl Strategy<Value = Vec<(Phoneme, Stress)>> {
    let symbols: Vec<&'static str> = ARPABET.symbols.iter().map(|s| s.text).collect();
    proptest::collection::vec(
        (
            proptest::sample::select(symbols),
            proptest::sample::select(vec![Stress::Unstressed, Stress::Secondary, Stress::Primary]),
        ),
        0..12,
    )
    .prop_map(|drawn| {
        drawn
            .into_iter()
            .map(|(symbol, stress)| {
                let phoneme = ARPABET.phoneme(symbol).unwrap();
                let stress = if phoneme.is_vowel() {
                    stress
                } else {
                    Stress::Unstressed
                };
                (phoneme, stress)
            })
            .collect()
    })
}

proptest! {
    /// I4 — every syllable of a line belongs to exactly one foot, and no foot reaches past the
    /// line. A segmentation that dropped or double-counted a syllable would change the rhythm
    /// with nothing to trace it to.
    #[test]
    fn i4_feet_partition_a_line_exactly(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let scanned = meter::scan(&poem, &EverySyllableStressed);
        for stanza in scanned.stanzas() {
            for line in stanza.lines() {
                let mut next = 0;
                for foot in line.feet() {
                    prop_assert_eq!(foot.beats().start, next, "feet must be contiguous");
                    prop_assert!(foot.beats().end > foot.beats().start, "a foot has syllables");
                    prop_assert_eq!(
                        foot.beats().len(),
                        foot.kind().syllables(),
                        "a foot's width must match its kind"
                    );
                    prop_assert!(line.span().contains(foot.span()));
                    next = foot.beats().end;
                }
                prop_assert_eq!(next, line.beats().len(), "feet must cover every syllable");
                prop_assert_eq!(line.syllable_count().get(), line.beats().len());
                prop_assert_eq!(line.foot_count().get(), line.feet().len());
            }
        }
    }

    /// I4 — every syllable names the word it came from, and that word lies inside its line.
    #[test]
    fn i4_every_syllable_traces_to_a_word(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        let scanned = meter::scan(&poem, &EverySyllableStressed);
        for stanza in scanned.stanzas() {
            for line in stanza.lines() {
                for beat in line.beats() {
                    prop_assert!(beat.word.text(poem.source()).is_some());
                    prop_assert!(line.span().contains(beat.word));
                }
                for span in line.unpronounced() {
                    prop_assert!(line.span().contains(*span));
                }
            }
        }
    }

    /// I1 — scanning is a pure function of the poem and the phonemiser.
    #[test]
    fn i1_scanning_is_reproducible(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        prop_assert_eq!(
            meter::scan(&poem, &EverySyllableStressed),
            meter::scan(&poem, &EverySyllableStressed)
        );
    }

    /// Variance is a total order, so role assignment can rank stanzas without a tie-break that
    /// depends on float rounding — and the order tracks the *value*, not the representation.
    ///
    /// The axioms alone are not enough: an `Ord` that compared numerators and ignored denominators
    /// is still a valid total order, and passed an earlier version of this test while defeating
    /// the whole point of the exact arithmetic. The duplication case below is what catches it.
    #[test]
    fn variance_is_a_total_order(
        a in proptest::collection::vec(0_usize..20, 0..8),
        b in proptest::collection::vec(0_usize..20, 0..8),
    ) {
        let (x, y) = (meter::Variance::of(&a), meter::Variance::of(&b));
        prop_assert_eq!(x.cmp(&y), y.cmp(&x).reverse());
        prop_assert_eq!(x.cmp(&x), core::cmp::Ordering::Equal);

        // Equality must agree with ordering, or `Ord`'s contract is broken.
        prop_assert_eq!(x == y, x.cmp(&y) == core::cmp::Ordering::Equal);

        // Repeating a set leaves its spread unchanged but scales the stored fraction, so this
        // fails for any comparison that reads the numerator without the denominator.
        let doubled: Vec<usize> = a.iter().chain(a.iter()).copied().collect();
        prop_assert_eq!(meter::Variance::of(&doubled), x);
        prop_assert_eq!(meter::Variance::of(&doubled).cmp(&y), x.cmp(&y));

        // A set of identical values has no spread at all, whatever the value is.
        if a.iter().all(|v| Some(v) == a.first()) {
            prop_assert!(x.is_zero());
        }
    }

    /// Relative variance is invariant under scaling every line's length, which raw variance is
    /// not — the property `SPEC.md` §4.1 needs when stanzas differ in line length.
    #[test]
    fn relative_variance_ignores_scale(
        counts in proptest::collection::vec(1_usize..12, 1..6),
        factor in 2_usize..5,
    ) {
        let scaled: Vec<usize> = counts.iter().map(|c| c * factor).collect();
        prop_assert_eq!(
            meter::Variance::relative(&scaled),
            meter::Variance::relative(&counts)
        );
    }

    /// I4, for sound rather than text — syllabification attributes every phoneme exactly once,
    /// in order. A pass that lost or reordered a phoneme would make every syllable count, stress
    /// vector and rhyme derived from it untraceable to what the poet wrote.
    #[test]
    fn i4_syllabification_conserves_every_phoneme(phones in phone_sequence()) {
        let expected: Vec<Phoneme> = phones.iter().map(|(p, _)| *p).collect();
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            // The only refusal for a single-inventory sequence is having no vowel.
            prop_assert!(!expected.iter().any(|p| p.is_vowel()));
            return Ok(());
        };

        let got: Vec<Phoneme> = syllables.iter().flat_map(prosody_core::Syllable::phonemes).collect();
        prop_assert_eq!(got, expected.clone());
        prop_assert_eq!(
            syllables.len(),
            expected.iter().filter(|p| p.is_vowel()).count()
        );
    }

    /// Every interior onset is legal *and* maximal, so the boundary is pinned rather than merely
    /// permitted. Stated over arbitrary sequences, not just the ones English happens to use.
    #[test]
    fn syllable_boundaries_are_legal_and_maximal(phones in phone_sequence()) {
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            return Ok(());
        };
        for index in 1..syllables.len() {
            let onset = syllables[index].onset();
            prop_assert!((ARPABET.legal_onset)(onset));

            if let Some(&borrowed) = syllables[index - 1].coda().last() {
                let mut longer = vec![borrowed];
                longer.extend_from_slice(onset);
                prop_assert!(
                    !(ARPABET.legal_onset)(&longer),
                    "onset is not maximal"
                );
            }
        }
    }

    /// Every syllable holds exactly one vowel, in its nucleus.
    #[test]
    fn syllables_hold_one_vowel_each(phones in phone_sequence()) {
        let Ok(syllables) = phoneme::syllabify(&phones) else {
            return Ok(());
        };
        for syllable in &syllables {
            prop_assert!(syllable.nucleus().is_vowel());
            prop_assert!(syllable.onset().iter().all(|p| !p.is_vowel()));
            prop_assert!(syllable.coda().iter().all(|p| !p.is_vowel()));
        }
    }

    /// I1 — syllabification is a pure function of its input.
    #[test]
    fn i1_syllabification_is_reproducible(phones in phone_sequence()) {
        prop_assert_eq!(phoneme::syllabify(&phones), phoneme::syllabify(&phones));
    }

    /// I1 — derivation is a pure function of its inputs, so repeated runs agree, and the
    /// generator built from a seed replays the same stream.
    #[test]
    fn i1_seed_derivation_is_reproducible(domain in domain(), content in poem_text()) {
        let first = seed_of(domain, &content);
        let second = seed_of(domain, &content);
        prop_assert_eq!(first, second);
        prop_assert_eq!(draw(&first, 32), draw(&second, 32));
    }

    /// I2 — a unit's seed depends on the unit's own bytes and on nothing about where those bytes
    /// sit. The same stanza text embedded at different offsets in different poems must seed
    /// identically; if it did not, an edit anywhere before a stanza would move that stanza's
    /// voice, which is precisely the locality failure I2 exists to prevent.
    ///
    /// This is the property that the `Span` parameter of `Seed::for_unit` makes testable: the
    /// call site has to name its bounds, so a test can vary everything outside them.
    #[test]
    fn i2_a_unit_seeds_the_same_wherever_it_sits(
        domain in domain(),
        unit in poem_text(),
        before in poem_text(),
        after in poem_text(),
    ) {
        let alone = seed_of(domain, &unit);

        let embedded = format!("{before}{unit}{after}");
        let span = Span::new(before.len(), before.len() + unit.len());
        // The surrounding text may end mid-character; only a span on real boundaries resolves.
        prop_assume!(span.text(&embedded) == Some(unit.as_str()));

        prop_assert_eq!(Seed::for_unit(domain, span, &embedded).unwrap(), alone);
    }

    /// I2 — widening a span past the unit changes the seed. This is the mistake the old `&str`
    /// API could not distinguish from correct use: passing the whole poem where a stanza was
    /// meant stays deterministic, so only a locality property like this one catches it.
    #[test]
    fn i2_widening_a_span_changes_the_seed(
        unit in poem_text(),
        extra in poem_text(),
    ) {
        prop_assume!(!extra.is_empty());
        let source = format!("{unit}{extra}");
        let narrow = Span::new(0, unit.len());
        let wide = Span::new(0, source.len());
        prop_assume!(narrow.text(&source).is_some());

        prop_assert_ne!(
            Seed::for_unit(Domain::Stanza, narrow, &source).unwrap(),
            Seed::for_unit(Domain::Stanza, wide, &source).unwrap()
        );
    }

    /// I2 — distinct unit content gives distinct seeds.
    #[test]
    fn i2_distinct_content_gives_distinct_seeds(
        domain in domain(),
        a in poem_text(),
        b in poem_text(),
    ) {
        // `prop_assume!` rather than an `if`, so proptest tracks how often the case is vacuous
        // instead of silently asserting nothing.
        prop_assume!(a != b);
        prop_assert_ne!(seed_of(domain, &a), seed_of(domain, &b));
    }

    /// I2 — the two domains never collide for the same text.
    #[test]
    fn i2_domains_stay_separated(content in poem_text()) {
        prop_assert_ne!(seed_of(Domain::Stanza, &content), seed_of(Domain::Line, &content));
    }

    /// I2 — named sub-streams within a unit are independent, and asking for one does not disturb
    /// another.
    #[test]
    fn i2_child_seeds_are_independent(
        content in poem_text(),
        left in label(),
        right in label(),
    ) {
        let parent = seed_of(Domain::Stanza, &content);
        prop_assert_ne!(parent.child(left), parent);

        if left == right {
            prop_assert_eq!(parent.child(left), parent.child(right));
        } else {
            prop_assert_ne!(parent.child(left), parent.child(right));
            prop_assert_ne!(draw(&parent.child(left), 8), draw(&parent.child(right), 8));
        }
    }

    /// I4 — a span resolves to exactly the bytes it claims, or declines for a reason.
    ///
    /// Both halves matter. Without the `None` arm, an implementation that never resolved
    /// anything would pass; without the offset check, one that returned a correctly-sized slice
    /// from the wrong position would pass. Offsets are drawn near the source length so that
    /// in-bounds, boundary and out-of-bounds cases all actually occur — drawing them from the
    /// whole `usize` range would make the resolving case vanishingly rare.
    #[test]
    fn i4_spans_resolve_to_their_claimed_bytes_or_decline(
        source in poem_text(),
        start in 0_usize..=MAX_OFFSET,
        end in 0_usize..=MAX_OFFSET,
    ) {
        let span = Span::new(start, end);
        match span.text(&source) {
            Some(text) => {
                prop_assert_eq!(text.len(), span.len());
                // The slice must begin at the offset the span claims, not merely occur somewhere
                // in the source.
                let offset = text.as_ptr().addr() - source.as_ptr().addr();
                prop_assert_eq!(offset, span.start());
            }
            None => {
                // Declining is only allowed for a span that genuinely cannot be sliced.
                prop_assert!(
                    span.end() > source.len()
                        || !source.is_char_boundary(span.start())
                        || !source.is_char_boundary(span.end()),
                    "declined a span that was in bounds and on character boundaries"
                );
            }
        }
    }

    /// I4 — a node's span covers its children's spans, which is what makes attribution
    /// transitive: if an event traces to a token, it traces to the line containing that token.
    #[test]
    fn i4_join_covers_both_operands(
        a_start in 0_usize..200,
        a_len in 0_usize..50,
        b_start in 0_usize..200,
        b_len in 0_usize..50,
    ) {
        let a = Span::new(a_start, a_start + a_len);
        let b = Span::new(b_start, b_start + b_len);
        let joined = a.join(b);

        prop_assert!(joined.contains(a));
        prop_assert!(joined.contains(b));
        prop_assert_eq!(joined, b.join(a));
        prop_assert!(joined.len() >= a.len().max(b.len()));
    }

    /// I2, the property this whole design exists for — editing one stanza must not disturb any
    /// other stanza's voice.
    ///
    /// A randomly chosen token in a randomly chosen line of stanza *i* is replaced. Every stanza
    /// *j ≠ i* keeps a byte-identical seed, even though the edit shifts their byte offsets: seeds
    /// follow text, not position. Stanza *i*'s own seed must change, or the edit would be
    /// inaudible and the test vacuous.
    #[test]
    fn i2_an_edit_moves_only_its_own_stanza(
        source in poem_source(),
        pick in 0_usize..4096,
        text in replacement(),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        let stanza_count = poem.stanzas().len();
        let target = pick % stanza_count;

        let lines = poem.stanzas()[target].lines();
        let line = (pick / 7) % lines.len();
        let tokens = lines[line].tokens();
        let token = (pick / 13) % tokens.len();
        let span = tokens[token].span();
        prop_assume!(span.text(poem.source()) != Some(text));

        let before = stanza_seeds(&poem);
        poem.apply(span, text).unwrap();

        // Edits that add or remove a stanza break change the stanza list itself; those are
        // covered by the reparse property below, where index-wise comparison still makes sense.
        prop_assume!(poem.stanzas().len() == stanza_count);

        let after = stanza_seeds(&poem);
        prop_assert_eq!(after.len(), before.len());
        for index in 0..before.len() {
            if index == target {
                prop_assert_ne!(before[index], after[index], "edited stanza kept its seed");
            } else {
                prop_assert_eq!(before[index], after[index], "stanza {} moved", index);
            }
        }
    }

    /// I2 — appending or removing material *after* a stanza must not change that stanza's seed.
    ///
    /// This is the case a raw tree-sitter node range gets wrong: the trailing newline of a
    /// stanza's last line belongs to the stanza when it is terminal and to the stanza break once
    /// something follows, so the extent — and the seed — would move without the text moving.
    #[test]
    fn i2_appending_a_stanza_leaves_earlier_stanzas_alone(
        source in poem_source(),
        addition in poem_source(),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        let before = stanza_seeds(&poem);
        let before_spans: Vec<Span> = poem.stanzas().iter().map(prosody_core::Stanza::span).collect();

        let end = Span::new(poem.source().len(), poem.source().len());
        poem.apply(end, &format!("\n{addition}")).unwrap();

        let after = stanza_seeds(&poem);
        prop_assert!(after.len() >= before.len());
        for index in 0..before.len() {
            prop_assert_eq!(before_spans[index], poem.stanzas()[index].span());
            prop_assert_eq!(before[index], after[index]);
        }
    }

    /// I4 — every node the parser produces resolves to real source text, and the tree nests:
    /// tokens inside lines inside stanzas.
    #[test]
    fn i4_every_parsed_node_resolves_and_nests(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        for stanza in poem.stanzas() {
            prop_assert!(stanza.span().text(poem.source()).is_some());
            for line in stanza.lines() {
                prop_assert!(line.span().text(poem.source()).is_some());
                prop_assert!(stanza.span().contains(line.span()));
                for token in line.tokens() {
                    prop_assert!(token.span().text(poem.source()).is_some());
                    prop_assert!(line.span().contains(token.span()));
                    prop_assert!(!token.span().is_empty());
                }
            }
        }
    }

    /// I4 — tokenisation loses nothing. Every non-whitespace byte of a line is covered by exactly
    /// one token.
    ///
    /// Nesting and resolution say only that the tokens which exist are well-formed; they would
    /// happily accept a tokeniser that dropped half its output. This says the text is *fully*
    /// attributed, which is what invariant I4 actually claims.
    #[test]
    fn i4_tokens_cover_every_non_whitespace_byte(source in poem_source()) {
        let poem = Poem::parse(source).unwrap();
        for stanza in poem.stanzas() {
            for line in stanza.lines() {
                let mut covered = vec![false; line.span().len()];
                for token in line.tokens() {
                    for offset in token.span().range() {
                        let local = offset - line.span().start();
                        prop_assert!(!covered[local], "byte {} covered twice", offset);
                        covered[local] = true;
                    }
                }
                let text = line.span().text(poem.source()).unwrap();
                for (local, byte) in text.bytes().enumerate() {
                    if !byte.is_ascii_whitespace() {
                        prop_assert!(covered[local], "byte {local} of {text:?} has no token");
                    }
                }
            }
        }
    }

    /// I1 — an incremental reparse lands in exactly the same place as parsing the edited text
    /// from scratch, for every shape of edit including ones that insert newlines and change the
    /// stanza count. If these disagreed, output would depend on a poem's edit history rather
    /// than on the poem.
    #[test]
    fn i1_incremental_reparse_matches_a_fresh_parse(
        source in poem_source(),
        pick in 0_usize..4096,
        text in replacement(),
    ) {
        let mut incremental = Poem::parse(source).unwrap();
        let spans = token_spans(&incremental);
        let span = spans[pick % spans.len()];

        incremental.apply(span, text).unwrap();

        let fresh = Poem::parse(incremental.source().to_owned()).unwrap();
        prop_assert_eq!(incremental.stanzas(), fresh.stanzas());
        prop_assert_eq!(stanza_seeds(&incremental), stanza_seeds(&fresh));
        prop_assert_eq!(incremental.diagnostics(), fresh.diagnostics());
    }

    /// I1 — a chain of incremental edits is equivalent to parsing the final text once. A single
    /// edit can hide a stale-tree bug that only shows up on the edit after it.
    #[test]
    fn i1_chained_edits_match_a_fresh_parse(
        source in poem_source(),
        picks in proptest::collection::vec((0_usize..4096, replacement()), 1..5),
    ) {
        let mut poem = Poem::parse(source).unwrap();
        for (pick, text) in picks {
            let spans = token_spans(&poem);
            if spans.is_empty() {
                break;
            }
            poem.apply(spans[pick % spans.len()], text).unwrap();
        }
        let fresh = Poem::parse(poem.source().to_owned()).unwrap();
        prop_assert_eq!(poem.stanzas(), fresh.stanzas());
        prop_assert_eq!(stanza_seeds(&poem), stanza_seeds(&fresh));
    }

    /// I4 — containment is reflexive and transitive, so a token inside a line inside a stanza is
    /// attributable to that stanza.
    #[test]
    fn i4_containment_is_transitive(
        outer_start in 0_usize..100,
        outer_len in 0_usize..100,
        inner_offset in 0_usize..100,
        inner_len in 0_usize..100,
        leaf_offset in 0_usize..100,
        leaf_len in 0_usize..100,
    ) {
        let outer = Span::new(outer_start, outer_start + outer_len);
        prop_assert!(outer.contains(outer));

        let inner = Span::new(outer_start + inner_offset, outer_start + inner_offset + inner_len);
        let leaf = Span::new(inner.start() + leaf_offset, inner.start() + leaf_offset + leaf_len);

        if outer.contains(inner) && inner.contains(leaf) {
            prop_assert!(outer.contains(leaf));
        }
    }
}
