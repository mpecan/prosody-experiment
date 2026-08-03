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
| M2b | Letter-to-sound fallback for out-of-vocabulary words | **done** | §7.2, §9; iteration 4 |
| M3a | Meter: stress vectors, foot segmentation, metrical variance | **done** | §4.2, §7.3; iteration 5 |
| M3b | Rhyme: continuous scoring, equivalence classes | **done** | §4.5, §7.3; iteration 6 |
| M4a | Repetition: refrain, anaphora, epistrophe, and `VoiceId` | **done** | §4.3, §7.4; iteration 7 |
| M4b | Repetition: alliteration, assonance, morphological near-repetition | **done** | §4.3; iteration 8 |
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

- **Word families are order-dependent, like rhyme classes.** A word joins the nearest family
  *built so far*, so editing an early stanza can change a later word's `WordVoiceId`. Depth is now
  order-independent — measured from each family's shortest form — but membership is not. Same
  shape as the rhyme and role-locality questions; **M5 must settle all three together.**
- **A token can be both a family member and a rhyme.** "burning" may be a variation on "burn"
  (§4.3, timbre and modulation) *and* rhyme with "turning" (§4.5, pitch). The two passes stay
  structurally separate — verified, no shared state — but nothing says how a mapping should
  reconcile both signals on one word. **Decide at M5.**
- **Irregular morphology is entirely missed** — `sing`/`sang`, `go`/`went`, `be`/`was`. No prefix
  rule can reach it, and these are disproportionately the oldest, commonest, most repeated verbs
  in English. A small closed table of principal parts would fix it, in the same shape as the
  endings table. **Decide at M5 or defer explicitly.**
- **Alliteration's window is one line.** Editions that print each Anglo-Saxon half-line as its own
  line would lose the form's structural device. A stanza-wide window would flood a Hopkins-density
  poem instead; a bounded sliding word-window is the likely answer. **Decide when a real
  alliterative text is in the corpus.**

- **`VoiceId` is line-scoped; the morphological row is word-scoped** (`SPEC.md` §4.3). A line can
  be part of a verbatim refrain *and* contain a word whose family lives in an unrelated line —
  "burn" here, "burnt" there. `LineRepetition` holds one voice and has nowhere to put a second
  identity. **M4b needs a parallel word-scoped structure**, not a refinement of this one. Iteration
  1 charged M4a with getting `VoiceId` right: the seed-avalanche half is solved, the granularity
  half is not.
- **A sestina's rotating end-words are invisible.** One occurrence per stanza is far too sparse for
  the density rule that admits Whitman's "I" and Ginsberg's "who". The form's whole mechanism is
  single-word end-repetition rotated across stanzas, and it needs its own detector. **Decide at
  M4b or defer explicitly.**
- **A refrain line in otherwise-unique stanzas gives §4.1 no signal.** Per-stanza metrical variance
  reflects the *other* lines, so the poem's most periodic, most loop-worthy line is invisible to
  role assignment. Every real refrain form checked — villanelle, Psalm 136, The Three Ravens —
  places one recurring line per stanza. **M5 needs line-grain repetition data alongside variance.**

- **Rhyme classes are poem-wide and order-dependent** (`SPEC.md` §4.5, §6 I2). Editing stanza 1 to
  an unrhymed line shifts stanza 3's `RhymeClassId` even though stanza 3 is byte-identical — and
  §4.5 hands each class a scale degree, so a pitch moves. Not a literal I2 violation: I2 is about
  PRNG seeding, and voices do not exist until M5. But it is the same shape as the role-locality
  question already recorded for meter, and **M5 must decide both together.**
- **Rhyme scoring ranks consonance above assonance** (`SPEC.md` §4.3, §4.5). Because the score is
  a suffix match, "worth"/"breath" (shared coda) scores 0.5 while "lake"/"fate" (shared stressed
  vowel) scores 0. Most prosody treats the vowel as rhyme's primary ingredient. Left alone because
  §4.3 gives assonance its own pass and a threshold sweep puts the current metric near its
  optimum — but **M4 must decide** whether the assonance pass double-signals a pair that rhyme
  has already connected.

- **Which variance does role assignment use?** (`SPEC.md` §4.1). Raw `Variance::of` grows with
  line length even when raggedness does not: `[4,4,4,6]` scores 0.75 and `[10,10,10,15]` scores
  4.69, though they are equally ragged in proportion. `Variance::relative` (variance ÷ mean²,
  still exact) is scale-free. **Decide at M5.** Raw is right for comparing stanzas of similar line
  length; relative is right across a poem that mixes hexameter and trimeter stanzas.
- **`dominant_foot` is unreliable for iambic pentameter** — measured 50–57% agreement on
  Shakespeare, Milton and Pope. Not a segmentation bug: `CMUdict` marks monosyllabic function
  words (`of`, `to`, `as`, `shall`, `but`) with primary stress, so a run of them reads as
  spondees. This is `SPEC.md` §9's lexical-vs-phrasal stress limitation, and its practical reach
  is much larger than that bullet suggests. Anything downstream that leans on `dominant_foot`
  gets the wrong answer about half the time on the commonest English metre. **Decide at M5**
  whether the Voice IR should use it at all.

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

**Iteration 9 — M5: surprisal, role assignment, and the Voice IR.** Not yet planned.

M5 inherits four recorded decisions that have to be made together, because they are all the same
question — how much may an analysis depend on the rest of the poem?
- which variance the role assignment uses (raw or relative)
- whether `dominant_foot` is trustworthy enough to lower from
- rhyme-class and word-family order-dependence
- how a token that is both a family variation and a rhyme reconciles timbre against pitch

---

---

## Log

### Iteration 8 — M4b phonetic repetition

Built: `prosody-core::sound` — word families, alliteration, assonance. 297 tests.

Reviewed by two parallel agents. The linguistics reviewer graded the family rule against the whole
dictionary rather than sampling it, which is what made this iteration worth having.

**The family rule was 45% precision.** It grouped `for`, `fort`, `forty`, `force`, `forest`,
`foreign` and `ford` into one voice, and produced 4,742 families of four or more words across the
dictionary, some fifty wide. That is `SPEC.md` §1's failure mode exactly: a modulation landing on
"forest" traceable to an accident in a table.

| Finding | Resolution |
|---|---|
| **Any word plus a plausible suffix counted.** Two endings alone — bare `IY` at 7.8% precision and `AH N` at 8.3% — produced 34% of all false joins; `T` sat at 27.3% and made `for`/`fort`, `car`/`cart`, `star`/`start` | English picks its ending *by the sound before it*: `S` after voiceless, `Z` after voiced, `T` after voiceless or the irregular nasal class. Checking the allomorph is what separates `walked` from `cart`. The two worst endings are gone |
| **Depth degenerated on word order.** "burning burn burnt" gave `burn` and `burnt` the same depth, collapsing no-suffix and shallow-suffix into one modulation | Measured from each family's *shortest* form, after every word is placed |
| **Alliteration read the first syllable, not the stressed one.** `collide` and `believe` alliterate on a stressed `L`; the code reported a spurious assonance on the schwa they both open with and missed the alliteration | Reads the stressed syllable, as the rhyme pass already does |
| **The ending check had no coverage in its own crate** — removing it passed all 17 unit and 38 property tests, caught only by a real-word test in another crate | `sing`/`sink` and `for`/`fort` are now hand-worked cases in `sound.rs` |
| The property `word_families_share_a_root` allowed a slack the implementation does not, which is *why* two mutations were invisible to it | Tightened |
| Comparing every occurrence rather than every distinct sound made refrain-heavy verse quadratic — 188ms on a 1000-line poem against 3.4ms for the text pass | Families hold distinct sounds |

**Now measured and tracked**: `prosody-lts/tests/family_accuracy.rs` grades 1200 real inflection
pairs and 1200 merely-similar pairs from the dictionary, with ground truth taken from *spelling*
so it is independent of the phonetic rule under test. **80.6% recall at a 5.0% false-join rate**,
with floors that fail the build on regression.

A bug in iteration 4's work surfaced too: the letter-to-sound `-le` rule looked at the whole word,
so "tables" came out `T EY B L AH S` instead of `T EY B AH L S` — the same suffix-blindness fixed
for silent-`e` in that very iteration, in a rule written at the same time.

### Iteration 7 — M4a exact repetition and the voice

Built: `prosody-core::repetition` — refrain, anaphora, epistrophe, and `VoiceId`. No phonemiser
needed; all three are facts about text. 291 tests.

Reviewed by two parallel agents (spec fidelity, Rust-and-tests). The spec reviewer ran real poems
through the pass; the other mutation-tested it.

| Finding | Resolution |
|---|---|
| **Requiring two words made the canonical figure invisible.** Whitman's "I" (3 of 5 lines) and Ginsberg's "who" (4 of 4) both reported *zero* figures — the cases used to define anaphora. A content-word filter would not help: "I" and "who" are closed-class | A one-word figure now needs three occurrences covering at least half the lines it spans. Measured on real text: Ginsberg and Whitman qualify, Shakespeare's "And" (3 of 14) does not |
| **A figure's span could claim text nobody repeated.** The hull from first to last matched word absorbs whatever sits between: `"and, the tide"` and `"and the tide"` were reported as the same three-word figure | `Occurrence` holds each word's span. `hull()` is available for a debug view and documented as approximate |
| **A wordless line had no identity.** Two lines of `"..."` each got their own voice, so a repeated punctuation line could never be a refrain — and the test named for this used a blank line, which the grammar reads as a stanza break, so the branch had no coverage at all | Keyed on the raw line text when there are no words |
| Dead arithmetic in the empty-line branch — a push immediately overwritten with the same value, hiding an invariant behind `saturating_sub` | Simplified |
| `Figure::words()` meant "n" for an edge figure and "line length" for a refrain, via a fallible lookup whose fallback was unreachable | Derived from the occurrence: for a refrain the repeated part *is* the whole line, so the meaning is now uniform |
| **The property generator produced no anaphora or epistrophe in 5000 samples** — every repetition property held vacuously for two of the three figures. The same fixture problem as the rhyme properties an iteration earlier | Lines are now assembled from shared openings, middles and endings, so nested figures are common |
| Only single unit tests guarded the refrain-exclusion and the once-per-line-set rules | Properties for both. Three mutations that were previously caught by nothing in the property suite now fail it |

Judged differently from the reviewer, with reasoning: it read the `covered` check as double-reporting
when a fourth line dilutes a shorter n-gram — three lines sharing "and the tide" plus one sharing
only "and the" yields two figures over different line-sets. That is nested anaphora, which is real
and which a reader hears; Whitman has "I celebrate" inside "I". The rule is one figure per *set of
lines*, not one figure per line, and the module doc now says so. A property pins it.

Measured, not fixed: `analyse` runs in ~3.4ms on a realistic 1000-line poem, degrading
super-linearly only when "lines" grow prose-length. Not a concern for verse.

### Iteration 6 — M3b rhyme

Built: `prosody-core::rhyme` — continuous scoring, poem-wide classes, and `Phonemiser::variants`
so a heteronym's sense can be chosen where there is context to choose. 267 tests.

Real verse: Gray's Elegy reads ABAB, Pope's couplet AA, and Thomas's villanelle resolves to
exactly two classes with the refrain returning to its own across a stanza break — `SPEC.md` §4.5's
"two-note obsession, which is what a villanelle already is".

Reviewed by two parallel agents (prosody, invariants-and-Rust). The prosody reviewer swept the
threshold over 64 true rhyme pairs and ~3000 negatives, and brute-forced order-dependence.

| Finding | Resolution |
|---|---|
| **The reported score depended on which line founded the class.** "fade"/"shade" rhyme perfectly, but with "shed" read first the pair reported 0.5 — and §4.5 turns that number into exact pitch versus detune, so an unrelated third line swapped them | Scoring is a second pass, against the whole class, after every line is placed. Order-independent |
| **A heteronym's wrong sense won whenever its rival matched nothing.** `classify` returned `PERFECT` for founding a class, which variant choice could not tell from a genuine perfect match: `crow`/`bow` picked the bend, not the weapon | Variant choice uses the raw best match, which never reports a founding score |
| **`RhymeScore` overflowed `u32`** — wrong ordering in release, a panic in debug. `Variance` uses `u128` for exactly this reason; the Eq/Ord lesson was carried over from that incident but the width was not | Widened to `u64` |
| **All four new property tests verified shape, not content.** Five mutations passed every one, including two opposite and obviously-wrong classifiers — a singleton-per-line partition and a one-class-for-everything partition are both partitions | Added a property tying membership to scoring. Then found the *fixture* was the deeper problem: the fake phonemiser gave every word the same vowel, so every line rhymed. It now varies with spelling |
| `LineRhyme::score` reported `PERFECT` for a line that rhymes with nothing | `Option<RhymeScore>`, `None` for a class of one |
| `is_none` had no consumer; `score` and `score_tails` duplicated the same logic | Removed; unified |

The threshold of 1/2 was swept rather than assumed: 3.1% false negatives, 0.9% false positives,
and a shallow plateau across 1/3–1/2 rather than a sharp optimum. Recorded, not guessed.

A property also caught an over-claim in *itself*: it asserted a lone line rhymes with nothing
anywhere, but under the greedy rule a later line may rhyme with it and still belong elsewhere,
having found a better class. The honest invariant is that nothing placed *before* it rhymed.

Deliberately **not** changed:
- **Suffix-only scoring**, which ranks consonance above assonance. Raised as an M4 question.
- **Greedy classification over transitive closure.** Union-find chains: *fade*~*shade*~*shed*
  would put *fade* and *shed* on one pitch.

### Iteration 5 — M3a meter

Built: `prosody-core::meter` — stress vectors, foot segmentation by dynamic programming, and
exact-rational metrical variance. 231 tests.

Reviewed by three parallel agents (prosody, invariants, rust-and-tests). The prosody reviewer
scanned Shakespeare, Milton, Pope, Blake, Dickinson and Hopkins, and brute-forced every stress
pattern up to length 14.

| Finding | Resolution |
|---|---|
| **`Variance` derived `PartialEq` but hand-wrote `Ord`**, so equal spreads at different stanza lengths compared unequal while `cmp` called them equal — Rust's contract broken on the exact comparison the type exists for | `PartialEq` and `Hash` now mirror the cross-multiplication |
| **The catalexis tie-break was added into the cost**, so it overrode strictly better tilings. Measured: 13–16% of odd-length stress patterns scanned worse than necessary — `S S w` took a one-mismatch dactyl over a perfect `Single` + trochee | Ranked below mismatch, where a tie-break belongs |
| **Amphibrach (w S w) was missing.** An amphibrachic line still tiles at equal cost out of iambs and pyrrhics, so the search never noticed — the feet were just named wrong | Added. Cretic deferred: rare as an English base metre |
| **A word could vanish entirely**: `Ok` with zero syllables produced no beats *and* no `unpronounced` entry, contradicting this slice's own acceptance criterion | Treated like any other failure |
| **Foot spans overlap** when a word straddles two feet, reproduced from the property generators in under 150 cases | Documented as the poem's own doing, with `beats_of` giving exact per-syllable attribution, and a test pinning the overlap so it is visible rather than surprising |
| Raw variance is scale-dependent, biasing §4.1 toward calling long-lined stanzas aperiodic | `Variance::relative` added, exact and sqrt-free; which one M5 uses is now an open question |
| `Beat::index` was dead — hardcoding it to 0 passed all 221 tests | Asserted |
| The catalexis tie-break and the primary-only stress rule had **no coverage in their own crate** — both were caught only by real-verse tests in `prosody-lts` | Both now have in-crate tests, and the `Fake` word table gained a secondary-stressed entry it had lacked |
| `variance_is_a_total_order` passed an `Ord` that ignored the denominator entirely — the axioms alone admit a wrong-but-valid order | Strengthened with a duplication case, which fails for any comparison that reads the numerator alone |
| `Foot`'s public fields let an inconsistent foot be built; `SyllableCount(pub usize)` let units be added together | Private fields with accessors; a property now checks `beats().len() == kind().syllables()` |
| `Foot::beats` could index a foreign line and panic | `LineMeter::beats_of` |

Five mutations were run against the finished code and all five now fail *in `prosody-core`*, not
merely somewhere in the workspace: dead `Beat::index`, removed catalexis tie-break, secondary
stress counted as a beat, `Ord` ignoring the denominator, and a dropped empty pronunciation.

Deliberately **not** changed:
- **`carries_the_beat` still counts primary stress only.** Verified correct for compounds
  (`blackbird`, `grandmother`) and Latinate words (`understand`, `intervene`, `comprehend`). It
  costs the "cavalcade" class, where a reader promotes secondary stress to carry the beat — the
  same word iteration 3 noted for rhyme, now known to affect metre too.
- **Cretic (S w S)** — no concrete English example justified the implementation effort.
- **Elision.** "Temperate" scans as three syllables where the verse wants two. A different failure
  from lexical-vs-phrasal stress — a syllable-*count* problem, not a stress-*placement* one — and
  worth naming separately when auditing accuracy.

### Iteration 4 — M2b letter-to-sound fallback

Built: `crates/prosody-lts` (rule-based English grapheme-to-phoneme), `Fallback` in
`prosody-core::phoneme`, and an edge-apostrophe retry in `CmuDict`. 192 tests.

Reviewed by four parallel agents (linguistics, invariants, rust-quality, test-quality). The
linguistics reviewer graded the guesser against all 124k dictionary entries rather than reading
it, which is what made the iteration worth having.

**Accuracy, before and after this iteration's fixes** — now a tracked test, not a claim:

| measure | before | after |
|---|---|---|
| exact whole word | 17.9% | **25.4%** |
| syllable count | not measured | **83.5%** |
| primary stress position | 67.7% | 67.7% |
| phoneme error rate | 0.29 | **0.24** |
| coverage | not measured | **99.9%** |

Findings accepted and fixed:

| Finding | Resolution |
|---|---|
| **No vowel reduction at all** — the dominant error source. `abandon` came out `AE B AE N D AA N` | Unstressed monophthongs reduce to schwa |
| **Silent-e detection was suffix-blind**, so every past tense and plural was wrong: `hoped` → `HH AA P EH D`. Measured `-ed` 2.7%, `-es` 0.2% | Looks through an inflectional `d`/`s` to the stem |
| **The `dge` rule was dead code** — the silent-e limit excluded any spelling ending in `e`, which is exactly `dge`. `judge` gained a spurious `D` | Limit applies per-spelling; `judge` is now `JH AH JH` |
| **R-controlled digraphs fired before a vowel**: `hero` → `HH ER AA`, `very` → `V ER IY` | Only when the `r` closes the syllable |
| **Initial silent letters absent**: `knight` → `K N AY T`. Measured 0% for `wr`/`ps`/`gn` | `kn`, `gn`, `ps`, `pn`, `wr` handled |
| **No `gh` rule**, so `though` leaked `G HH` | Silent after a vowel, hard word-initially |
| **`-le` had an exclusion but no replacement**: `table` → `T AE B L EH` | Its own syllable, and the preceding vowel lengthens |
| **German `ß` missing from the fold table**, so `straße` returned silence — contradicting the module's own goal | Folding now maps to *spellings*, not single letters: `ß`→`ss`, `č`→`ch`, `š`→`sh`, `ž`→`zh`, `œ`→`oe` |
| `th` was always unvoiced, so `the` was `TH EH` — wrong English enshrined in a test | Voiced in a closed list of function words and between vowels |
| `cc` before a front vowel collapsed as a geminate: `accent` → `AE S EH N T`, losing the /k/ | Doubled `c`/`g` split before a front vowel |
| First-syllable stress is right for 6% of `-tion` words against 94% for penultimate | Penultimate for `-tion`/`-sion`/`-ic`/`-ity` and friends |
| **Grapheme matching took the first rule, not the longest** — a rule appended at the natural place was silently shadowed (`eigh` by `ei`) | Longest match wins; declaration order no longer matters, and the misleading comment is gone |
| `NotKnown` was returned for a word with rules but no vowel — the very distinction M2a introduced, applied inconsistently | `brrr` is now `Unsyllabifiable(NoNucleus)`, like `CMUdict`'s `hm` |
| `fallback_refuses_to_mix_inventories` only asserted the compatible case; deleting the check entirely passed the suite | A second inventory exists in the tests, and both refusal directions are asserted |
| `yod_drops` and `y_sound`'s mid-word branch were entirely untested — gutting either passed everything | Hand-worked cases for `rule`/`dune`/`chute`/`huge` and `gym`/`myth` |
| The three `any_word` properties passed vacuously if the guesser returned `Err` for everything | Anchored by a fixed-word test and a vowel-bearing generator |
| `LetterToSound` was a concrete English implementation living in the inventory-agnostic core | Moved to `crates/prosody-lts`, with the boundary rule written into `CLAUDE.md` §2 |
| `Fallback` is generic but lived in the guesser's module | Moved beside the `Phonemiser` trait it composes |
| `is_guess` was a free function with no consumer | `Provenance::is_guess`, mirroring `Stress::is_stressed` |
| `_private: ()` marker fields | `#[non_exhaustive]` |

A property test found a bug of its own: `has_silent_final_e` counted a leading `y` as a vowel
while `transcribe` treats it as a consonant, so "ybe" silenced its only nucleus.

Deliberately **not** changed:
- **Vowel quality in stressed open syllables.** `sofa` is `S AA F AH`, not `S OW F AH`; `ghost` is
  `G AA S T`. Open-syllable lengthening would fix those and break `city` and `very` — English is
  genuinely split, and the tests now state the actual behaviour rather than an aspiration.
- **`ou`, `ea`, `ow` each pick one value.** `though`, `bread` and `cow` are wrong as a result.
  Spelling does not determine these; a closed list is the only fix and is not worth it yet.
- **`ch` is never /k/**, so `chorus` and `chaos` are wrong. Needs a lexicon, which is what the
  dictionary already is — these words only reach the guesser if they are absent from it.
- **Stress accuracy did not move** (67.7%). The suffix rules help the families they cover, which
  are a small share of the corpus. Recorded rather than papered over.

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
