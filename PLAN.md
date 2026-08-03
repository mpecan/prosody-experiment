# Build plan & iteration ledger

Durable state for the build loop. `SPEC.md` is what we're building; `CLAUDE.md` is how.
This file is the single source of truth for *where we are* — it must survive context loss.

**Update this file at the end of every iteration.** An iteration that doesn't update the
ledger did not happen.

---

## Iteration protocol

Each iteration is strictly serial through these four phases. No parallelism except in REVIEW.

1. **PLAN** — read this file. Pick the next slice from the milestone list. Write the slice's
   goal and its acceptance criteria into "Current iteration" below *before* writing code.
   A slice is one milestone or a coherent part of one; it must end in a working, gated state.
2. **BUILD** — implement it. Serially. Then run all gates from `CLAUDE.md` §3:
   `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`. Gates must be green before REVIEW.
3. **REVIEW** — spawn parallel review agents, one per perspective, each reading the diff:
   - **invariants** — I1–I4 from `SPEC.md` §6; hunt for nondeterminism per `CLAUDE.md` §4
   - **spec-fidelity** — does the code implement what `SPEC.md` actually says, or a plausible
     neighbour of it? Watch for the §1 failure mode (untraceable output).
   - **rust-quality** — API design, type discipline (`CLAUDE.md` §5), error handling, idiom
   - **test-quality** — are the tests falsifiable? Hand-worked expectations, or values copied
     from the implementation's own output?
   Agents report findings only; they do not edit.
4. **REMEDIATE** — triage findings serially. Fix what's real, record what's rejected and why.
   Re-run gates. Commit. Update this file.

Stop the loop when every milestone below is `done`, or when a decision needs the user.

---

## Milestones

Order follows `SPEC.md` §7. Invariant property tests are added as soon as the machinery they
guard exists — they are not deferred to the end.

| # | Milestone | State | Notes |
|---|---|---|---|
| M0 | Workspace, lint gates, CI, `Span`, `Seed` | **done** | determinism + traceability primitives; iteration 1 |
| M1 | Tree-sitter grammar: frontmatter / stanza / line / caesura / token, incremental reparse | **done** | `SPEC.md` §7.1; iteration 2 |
| M2a | `Phonemiser` trait, phoneme model, CMUdict | **done** | §7.2, §9; iteration 3 |
| M2b | Letter-to-sound fallback for out-of-vocabulary words | todo | §7.2, §9 — must be marked `Provenance::LetterToSound` |
| M3 | Meter: stress vectors, foot segmentation. Rhyme: continuous scoring, equivalence classes | todo | §4.2, §4.5, §7.3 |
| M4 | Repetition passes: exact → anaphora/epistrophe → morphological | todo | §4.3, §7.4 — ship exact first |
| M5 | Surprisal (§4.4), role assignment (§4.1), Voice IR + lowering (several passes) | todo | §7.5 |
| M6 | `prosody-emit`: Strudel/Tidal text backend. `prosody-cli`: parse in, pattern out | todo | §7.6 — cheapest audition path |
| M6a | **§8 acceptance test: the couplet synonym swap** | todo | §8 — runs the moment M6 gives an audition path, not later |
| M7 | Debug annotation view | todo | §8 — the enforcement mechanism for I4 |
| M8 | Golden corpus (`insta`) + full I1–I4 property suite | todo | §8 — hardening, legitimately after M7 |
| M9 | OSC → SuperCollider, live-reload watcher | todo | §7.8 — only after mappings hold up |

`SPEC.md` §8 is titled "First test to write — before any of the fun parts," and `CLAUDE.md` §7
repeats that it precedes backend polish. M6a exists because an earlier draft of this plan buried
the synonym-swap test at the end, which would have let seven milestones of mapping work land
before anyone confirmed a single traceable, confined, audible change. It is split from M8 because
the golden corpus genuinely does belong later; the acceptance test does not.

### Open questions (raise, don't silently decide)

- **Heteronym sense selection** (`SPEC.md` §4.5, §9): `CMUdict` lists `wind` as `W AY N D` (the
  verb) before the common noun `W IH N D`, and similarly for `bow`, `live`, `tear`, `lead`,
  `close`, `read`, `record`, `content`. Taking the first entry is deterministic but often the
  poetically wrong sense, which will produce both false rhymes and missed ones. `CmuDict::variants`
  now exposes the alternates so the choice can be made where there is information to make it.
  **Decide at M3**: most likely the rhyme pass picks the variant maximising rhyme within a stanza,
  with a deterministic tie-break. Do not add a part-of-speech tagger — §9 rules that out for now.
- **Maximal onset ignores morpheme boundaries** (`SPEC.md` §4.3): "bathroom" syllabifies as
  `B AE` + `TH R UW M` ("ba-throom") because `TH R` is a legal onset, as in "three". Syllable
  counts and rhyme tails are unaffected, but onset-based alliteration would credit that syllable
  with a `TH R` onset. Not fixable through the phonotactics table — `TH R` genuinely is legal.
  **Decide at M4**, when alliteration lands: either accept it, or add a morpheme-boundary hint.

- **Apostrophe ambiguity** (`SPEC.md` §4.3, §9): the tokeniser keeps `'` inside a word so that
  `don't`, `o'er`, `'tis` and `burn'd` stay single tokens. The cost is that scare-quotes are
  absorbed too — `'the` and `burns'` become tokens distinct from `the` and `burns`, which will
  break refrain matching and CMUdict lookup for any quoted word. English orthography does not
  distinguish the two cases, so the grammar cannot either. **Decide at M2**: most likely a
  normalisation step in the phonemiser that strips edge apostrophes when a lookup fails, rather
  than a grammar change.

- **Voice identity vs. seeds** (`SPEC.md` §4.3, morphological row): seeds avalanche by design, so
  they can express the refrain row ("identical text ⇒ identical voice") but not the near-repetition
  row ("burn / burning / burnt ⇒ *same* voice, varied parameters"). M4 must introduce a `VoiceId`
  assigned by the repetition pass, with phoneme edit distance driving modulation depth, and seeds
  demoted to residual jitter within a voice. **Decide the representation at M4, before M5 lowers
  anything.** This is the mapping the spec says earns the project; getting it wrong reproduces §1's
  failure mode exactly.
- **Role locality under a global threshold** (`SPEC.md` §4.1, §9): if the free-verse fallback
  compares each stanza's metrical variance against a poem-wide statistic, then editing stanza A can
  flip stanza B's *role* even though B's text and seed are untouched. Per-unit seeding protects
  randomness locality, not structural locality. M5 must decide whether that is acceptable.

- **Free verse degeneracy** (`SPEC.md` §9): absolute vs. global-variance-relative threshold for
  role assignment. Deferred to M5, must be decided there.
- **Short poems & looping-as-style** (`SPEC.md` §9): arguably violates I3. Unresolved. Do not
  add a looping style parameter without raising this.
- **Non-English input** (`SPEC.md` §9): only the trait boundary is in scope; no second
  phonemiser implementation planned.

---

## Current iteration

**Iteration 4 — M2b: letter-to-sound fallback.** Not yet planned; plan it at the start of the
iteration.

Carried into M2b from iteration 3's review:
- Guessed pronunciations must be built with `Pronunciation::guessed`, never `from_dictionary`.
- Edge-apostrophe normalisation belongs here, on lookup failure — but `CMUdict` genuinely contains
  `'tis`, `'bout` and `'cause`, so stripping must be a *fallback* after a direct lookup fails,
  never a pre-pass.
- `TokenKind::Number` still has no consumer: decide how numerals phonemise, or document that they
  do not.

---

---

## Log

### Iteration 3 — M2a phoneme model and CMUdict

Built: `prosody-core::phoneme` (inventory-agnostic phonemes, syllables, stress, the `Phonemiser`
trait, maximal-onset syllabification) and `crates/prosody-cmudict` (the dictionary vendored and
embedded, BSD-2-Clause). 141 tests. `prosody-core` still does no IO.

Reviewed by four parallel agents (invariants, linguistics, rust-quality, test-quality). Three
verified findings by running code; one scripted the whole dictionary. Findings accepted and fixed:

| Finding | Resolution |
|---|---|
| **The corpus test did not do its job.** Three mutations — never forming an onset, reversing to minimal onset, and deleting an entry from the phonotactics table — all passed every corpus test over 135k words. Conservation and syllable counts are invariant under all three, and asserting produced onsets are *legal* is circular: the splitter only returns splits the same table approved | Added **maximality**: at every interior boundary, moving one more consonant into the onset must be illegal. Non-circular, and pins the boundary exactly. Plus more hand-worked splits, which are the only thing that can catch a wrong *table*. All three mutations now fail |
| `pronounce` could not distinguish "not in the dictionary" from "in it but unsyllabifiable". 8 real vowel-less entries exist (`hm`, `shh`, `mm`, `fs`, `ths`), so `pronounce("hm")` was identical to `pronounce("krapplesnaffle")` | `Result<Pronunciation, Unpronounceable>` with `NotKnown` and `Unsyllabifiable(SyllabifyError)` |
| `Pronunciation::new` took any `Provenance`, so `new(guessed, Dictionary)` compiled — the one claim provenance exists to prevent. Flagged independently by two reviewers | Named constructors `from_dictionary` / `guessed`; the free constructor is gone |
| `Inventory::legal_onset` took `&[&'static str]`, allocating a `Vec` per candidate split, and made the one extension point every future inventory implements stringly-typed | Takes `&[Phoneme]`; one projection per word instead of one per candidate |
| `Phoneme::Ord` had no test: dropping the inventory comparison passed all 109 tests, despite the doc claiming it provides the total order `CLAUDE.md` §4 requires before emission | Ordering, hashing and cross-inventory tests added. Mutation now caught |
| Onset-table tests used one example per match *arm*: deleting `F` from stop+liquid (breaking "flee", "free") passed the whole suite | One example per listed *symbol*, each named with the word that attests it. Mutation now caught |
| `Inventory::name` uniqueness was unenforced, so two inventories sharing a name would compare equal | Debug assertion that equal names imply the same symbol table. Pointer identity was rejected as the fix: addresses vary per run and `Ord` must not |
| `syllabify` read the inventory from the first phoneme and applied it to all | `SyllabifyError::MixedInventories` |
| `trim_last` did `len() - 1`, a panic in debug builds for empty input — the subtraction, which no lint catches, not the slicing | `saturating_sub` |
| The "first entry wins" comment misdescribed where determinism came from; the `or_insert` path was dead on real data (verified: zero duplicate keys in 135k entries) | `parse_entries` split out and tested on synthetic input, including a genuine duplicate key |
| `Provenance::LetterToSound`, empty pronunciations, and `Phoneme` hashing were unexercised | All tested |
| No property test for this module | Four, over arbitrary phone sequences rather than only ones English happens to use |

Two of my own hand-worked expectations were wrong and the tests caught them, which is why they
are hand-worked: `CMUdict` lists `read` as `R EH D` (past tense) first, and "pentameter" is not in
the dictionary at all.

Deliberately **not** changed:
- **The phonotactics table stands.** The linguistics review checked it against real entries and
  confirmed `S F` (sphere), `S W` (swim), `TH W` (thwart), `G W` (penguin), `SH R` (shrimp) and the
  `Y`-onsets are all attested, and that `V R` is correctly absent — "average" splits as `AEV`-`RIHJH`
  through the single-consonant fallback, which is the right answer.
- **`rhyme_tail` counts secondary stress**, not only primary. Restricting to primary would break
  real rhymes: "cavalcade" carries primary stress on its first syllable and only secondary on the
  rhyming final one, yet rhymes with "brigade". Now a test.
- **Heteronyms and morpheme boundaries** are raised as open questions above rather than guessed at.

Five mutations were run against the finished code to confirm the tests bite: never-onset,
minimal-onset, a deleted table entry, `F` removed from stop+liquid, and `Ord` ignoring the
inventory. All five fail the suite.

### Iteration 2 — M1 tree-sitter grammar

Built: `crates/tree-sitter-prosody` (grammar, committed generated parser, 14 corpus tests) and
`prosody-core::parse` (`Poem`/`Stanza`/`Line`/`Token`, incremental reparse). 76 tests.

Reviewed by four parallel agents (invariants, grammar correctness, rust-quality, test-quality),
three of which verified their findings by running code rather than reading it. Findings accepted
and fixed in the same iteration:

| Finding | Resolution |
|---|---|
| **I2 broken.** Tree-sitter gives a line's trailing newline to the *stanza break* when another stanza follows and to the *line* when the stanza is terminal, so a stanza's raw node range — and its seed — changed when a stanza was appended after it. Reproduced on a two-line poem | Spans are now **content spans**: a line's is the hull of its tokens, a stanza's the hull of its lines. Neither includes a delimiter, so a unit's identity depends only on the unit. Recorded as a rule in `CLAUDE.md` §5 |
| **I3 broken.** An unclosed style header, one not at byte 0, or trailing whitespace after `+++`, made the whole TOML block lex as poem text — with no frontmatter node *and no error node*, so nothing surfaced it | Grammar now accepts trailing whitespace after both delimiters; a `+++` line surviving into the body raises `DiagnosticKind::StrayStyleDelimiter` |
| **I4 gap.** Characters that are whitespace to the lexer but not skipped (form feed, vertical tab) produce an `ERROR` nested *inside* a line; the root-only scan reported a clean parse while the byte vanished from every token list | Diagnostics are collected by walking the whole tree, not just root children |
| `point_of` could be completely broken (row always 0) and all 12 property tests still passed — every generated edit stayed inside one line | Generators now pick a random line and token, and replacements include deletion, line breaks, stanza breaks and multi-byte text. Verified: the mutation is now caught |
| `line_seed` had zero coverage — returning `None` unconditionally passed all 57 tests | Four unit tests plus use in properties. Verified caught |
| `TokenKind::Punctuation` mapping untested — remapping it to `Word` passed everything, including the corpus test, which checks the grammar layer and never crosses the mapping seam | `every_token_kind_survives_the_mapping` |
| The I4 nesting property accepted a tokeniser that dropped half its tokens | Added `i4_tokens_cover_every_non_whitespace_byte`. Verified caught |
| `apply` edited `self.tree` in place before reparsing, so a failed reparse left a tree describing an edit that never happened — the next edit would build wrong spans from it | Edits a clone; the poem is untouched unless the reparse succeeds |
| `Option<Result<..>>` forced `.unwrap().unwrap()` at every call site | Flat `Result` with `ParseError::InvalidSpan(Span)`, which also gives the error a span per `CLAUDE.md` §6 |
| `line_seed(stanza: usize, line: usize)` — two same-typed indices, transposable silently | `StanzaIdx` / `LineIdx` newtypes |
| `,` and `:` inside numerals lexed as caesurae, so `1,000` and `3:14` produced false voice-split boundaries | `number` token |
| An ellipsis was three punctuation tokens | Single `...` token |
| `point_at`'s `unwrap_or(text)` silently substituted the whole document on an invalid offset | Replaced by `point_of(prefix)` / `advance(point, added)`, which take already-sliced text so there is no invalid input to handle |
| Corpus lacked the free-verse and refrain cases the milestone claimed | Both added, plus numerals and ellipsis (14 cases) |
| `prop_assume!(error_spans().is_empty())` filtered nothing — measured 5000/5000 | Removed |
| `CLAUDE.md` §3 claimed doc warnings were errors "via lints table"; they are only errors in CI via `RUSTDOCFLAGS` | Documented command corrected |

Five mutations were run against the finished code to confirm the tests bite: broken `point_of`,
a lossy tokeniser, `line_seed` returning `None`, punctuation mapped to `Word`, and stanza spans
taken from raw node ranges. All five fail the suite.

Deliberately **not** changed:
- **Scare-quote apostrophes** stay inside words. Raised as an open question above rather than
  guessed at — English does not distinguish `'tis` from `'the`, so the grammar cannot.
- **Caesura stays punctuation-only.** It cannot detect an unpunctuated rhythmic pause. That is a
  recorded M1 decision; whether a caesura splits a voice is an M5 question.
- **`from_tree` rebuilds the whole view on every reparse**, discarding node reuse above the parse
  itself. Poems are small, and the reason for incremental reparse is I2 locality, not speed.

A tree-sitter bug worth remembering: `{2,}` in a token regex is lowered into repeated *pairs*, so
`(\r?\n[ \t]*){2,}` left an odd trailing newline unconsumed and any poem ending in 3 or 5 blank
lines produced an ERROR while 2 or 4 parsed cleanly. Written as "one, then one-or-more" instead.
Independently reproduced by the grammar reviewer on a sibling grammar.

### Iteration 1 — M0 foundation

Built: workspace (`crates/prosody-core`), workspace lint table, toolchain pin, `Span`, `Seed`,
CI running the four gates with `--locked`. 40 tests (31 unit, 9 property). All gates green.

Reviewed by four parallel agents (invariants, spec-fidelity, rust-quality, test-quality).
Findings accepted and fixed in the same iteration:

| Finding | Resolution |
|---|---|
| `Seed::derive(&str)` could not distinguish a stanza from the whole poem — misuse stays deterministic, so I1 holds while I2 breaks undetectably | Replaced with `Seed::for_unit(Domain, Span, &str)`, returning `Option`. Ties I2 to I4 through one argument |
| `i4_spans_resolve_or_decline` drew offsets from `any::<u16>()` against a ≤120-byte source — 0 successes in 100 000 measured trials; an always-`None` implementation passed it | Offsets drawn near source length; added the `None` arm asserting declining is *justified*, and a pointer-offset check that the slice starts where the span claims |
| `i2_seeds_ignore_surrounding_context` was unfalsifiable — no channel existed for context to leak into a one-argument pure function | Deleted; replaced with `i2_a_unit_seeds_the_same_wherever_it_sits`, which the `Span` parameter makes genuinely testable, plus `i2_widening_a_span_changes_the_seed` |
| `i1_generator_output_is_independent_of_construction_order` tested `rand_chacha`, not this crate | Deleted |
| Freeze test pinned 1 of 3 derivation contexts | All three pinned (`Stanza`, `Line`, child) |
| Seed module doc claimed content-hashing gives §4.3 "for free" — true only for the refrain row | Doc corrected; morphological row raised as an open question above and as a rule in `CLAUDE.md` §5 |
| §8 acceptance test scheduled at M8, though spec says it comes first | Split out as M6a |
| `child(&str)` label could be poem-derived, breaking I2 | Now `&'static str` — poem text is never `'static`, so the misuse does not compile |
| `Span::new` clamped inverted ranges silently | Added `Span::try_new`; documented which constructor belongs where |
| Gates enforced only by memory; no `--locked` anywhere | CI workflow added, `--locked` on every gate |
| `Cargo.lock` untracked despite an exact toolchain pin | Committed |
| `CLAUDE.md` cited a `Seed::for_unit` that did not exist | Reconciled — the method now exists under that name |
| `rand_core` duplicated in `[dependencies]` and `[dev-dependencies]` | Removed from dev-deps |
| `child` allocated a `Vec` per call | Uses `blake3::Hasher::new_derive_key` + two `update`s |
| 2-byte UTF-8 only in boundary tests | Added 3-byte (`€`) and 4-byte (`🔥`) cases, and to the proptest alphabet |
| Vacuous proptest cases hidden by `if a != b` | `prop_assume!`, so proptest reports rejection rates |

Deliberately **not** changed, with reasons:
- **`Span::new`'s clamping kept** rather than made a `debug_assert`. The crate forbids panicking
  on malformed input, and a panic in the live-reload path is exactly what `CLAUDE.md` §6 rules
  out. `try_new` gives callers the visible-failure option instead.
- **`Seed::child` kept despite having no consumer yet**, in tension with M0's own "no speculative
  code" criterion. It is a primitive of the seeding design, it is tested, and M5 will use it —
  but this is a real exception to a rule this plan set, recorded rather than glossed.
- **Freeze tests pin values copied from the implementation.** Legitimate here: a KDF output has
  no hand-derivable expected value, and the test's purpose is to detect change. This is not the
  hazard `CLAUDE.md` §7 warns about, which concerns mapping-correctness tests.
