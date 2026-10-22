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
| M5a | Surprisal (§4.4) and the lexicon | **done** | §4.4; iteration 9 |
| M5b | Role assignment (§4.1) | **done** | §4.1, §9 free-verse fallback; iteration 10 |
| M5c | Voice IR + lowering | **done** | §7.5, §10; iteration 11 |
| M6 | `prosody-emit`: Strudel/Tidal text backend. `prosody-cli`: parse in, pattern out | **done** | §7.6; iteration 12. `Style` lives in `prosody-emit`, so I3 is a compiler-enforced edge |
| M6a | **§8 acceptance test: the couplet synonym swap** | **done** | §8; landed at M5c — the IR *is* the audition path, no backend needed |
| M7 | Debug annotation view | **done** | §8; iteration 13 |
| M8 | Golden corpus (`insta`) + full I1–I4 property suite | **done** | §8; iteration 14 |
| M9 | OSC → `SuperCollider`, live-reload watcher | **done** | §7.8; iteration 15 |

`SPEC.md` §8 is titled "First test to write — before any of the fun parts," and `CLAUDE.md` §7
repeats that it precedes backend polish. M6a exists because an earlier draft of this plan buried
the synonym-swap test at the end, which would have let seven milestones of mapping work land
before anyone confirmed a single traceable, confined, audible change. It is split from M8 because
the golden corpus genuinely does belong later; the acceptance test does not.

### Open questions (raise, don't silently decide)

- **The rhyme tail starts at the last *primary-stressed* vowel, which is wrong for polysyllabic
  feminine rhyme.** Found by running `examples/wilde-theocritus.poem`, a real villanelle, which has
  two rhymes and which the pipeline splits into **four** classes: `Persephone` alone, `Sicily`
  alone, `bee`/`sea` together, and the `-ate` group. Every reader hears the first three as one
  rhyme; the pass does not, because in `Persephone` the last primary stress is three syllables
  before the sound the rhyme turns on. The rule is right for monosyllables and masculine rhyme, and
  wrong for exactly the rhyme a villanelle is built from — so `SPEC.md` §4.5's "villanelle → a
  two-note obsession" does not hold on an actual villanelle.

  The synthetic villanelle in `tests/corpus/` *does* collapse to two notes, because its rhyme words
  are monosyllables. That is the difference between a fixture and a poem, and the argument for
  `examples/` existing. A fix probably means scoring from the last *syllable* boundary as well as
  the last stress and taking the better of the two, which is a change to what §4.5 maps — so:
  **raised, not decided.**

- **Every word carries a `Family`, including words that are alone in one.** §4.3's morphological
  row is about *near-repetition* — "burn / burning / burnt" — so a word sharing its family with
  nothing has no variation to modulate. The debug view suppresses singletons, because otherwise
  every row reads `f0+0` and the real families are invisible; but the IR still records them, and
  the emitter still emits a crush depth for them (a no-op at depth zero, so harmless today). Either
  `Event::family` should be `None` for a family of one, or the view's suppression is the right
  place for it. **Decide at M8**, when the golden corpus makes the noise visible in a snapshot.

- **A cycle is a fixed span, so mapping a line to a bar makes the *foot* elastic.** A two-foot line
  and a five-foot line occupy the same time, and the pulse stretches to fit. §4.2 says "line → bar
  (or phrase)", which admits this, and warns against quantising to 4/4, which this does not do —
  but it quantises the *line* instead, which is arguably worse. Strudel can express the alternative
  (`.slow`, steps-per-cycle), so this is a mapping decision rather than a backend limit. **Decide at
  M7 or M9, when the debug view can show the difference.**
- **The poem picks the tempo inside the style's window, and `SPEC.md` §4 has no such mapping.**
  `TempoRange::at_density` reads §5's "palette **and** constraint" as "the style gives the range,
  the poem gives the position in it", which is defensible and arguably the only reading under which
  a *range* means anything. But it is a §4-class mapping introduced in a §5 module, it is a
  poem-wide aggregate so no token span explains it (I4), and adding one syllable anywhere retunes
  the whole piece (I2-shaped, like the rhyme-degree question above). **Raised late — it should have
  been raised when it was written.**
- ~~**`Event::anchor` is deliberately not rendered.**~~ **Answered at M9, and it was real.** The
  OSC backend schedules in absolute beats, so an anaphora's words are pulled *onto* the pulse and
  an epistrophe rings on where its neighbours are clipped. Both are still unexpressible in
  mini-notation, so the two backends genuinely differ in what they can say — which is the argument
  for the `Backend` trait having an associated `Output` rather than returning text.
- **Scale degrees climb out of hearing.** `Scale::degree` keeps ascending so two rhyme classes
  never collide, which is right for I4 — but a poem with ~45 distinct rhyme classes reaches MIDI
  127, and past that the notes are inaudible, which defeats the I4 argument for climbing. Needs
  octave folding with some other parameter distinguishing the classes. **Decide when a long poem
  is in the corpus.**
- **Error types do not carry spans, against `CLAUDE.md` §6.** `StyleError` is the first error a
  *user* meets from their own file — a malformed header — and it reports no position, though
  `Poem::frontmatter()` has the span and `toml` reports its own offset. `prosody-core`'s errors
  (`DifferentPoems`, `Unpronounceable`, `SyllabifyError`) carry none either, so the rule as written
  is out of step with the whole codebase. **Resolve in one direction at M7**, when the debug view
  makes spans visible.

- **A scale degree is a poem-wide number, so an edit can transpose an untouched stanza.** `SPEC.md`
  §4.5 assigns degrees "in order of first appearance", which is inherently a fact about the whole
  poem: introduce a rhyme in stanza 1 and every later class shifts up one. Invariant I2's own words
  are "changing one word reshuffles the whole piece". The two requirements genuinely conflict, and
  §4.5's is the explicit one, so the behaviour stands and is pinned by
  `a_new_rhyme_class_transposes_the_classes_after_it` rather than left implicit. Same shape as the
  role-locality question M5b settled, and the same answer is available — a basis-style disclosure
  saying *why* a degree is what it is. **Decide at M6, when it can be heard.**
- **Free verse produces no melody at all.** A rhyme class of one expresses no melodic relation, so
  it gets no pitch — and in unrhymed free verse that means every event is unpitched. §4.1's
  equivalent degeneracy got a named `OnlyCandidate` fallback; §4.5's gets silence. Consistent with
  the mapping, but it means a whole class of poems reaches the backend with no pitch information
  whatsoever. **Decide at M6.**
- **Pitch reaches about 8% of events**, since only rhyme words are pitched. Whatever a backend
  chooses for the other 92% will dominate what is heard and none of it traces to the poem — §1's
  failure mode arriving through omission. §4.5's own consequences ("ABAB → alternating cadences",
  "villanelle → a two-note obsession") are claims about the *piece*, not about one note per line.
  The middle path worth considering: keep the per-event pitch and additionally carry the line's
  rhyme class on the `Voice` as a tonal centre — one field, every pitch still traceable, and it
  gives §4.3's anchors something to resolve to. **Raise at M6.**
- **The IR has no duration, tie, or rest**, and no per-bar beat count. A consumer infers a bar's
  length by scanning its events. The equal-subdivision convention is now documented on
  `Subdivision`, which is enough for a Tidal-style emitter but not for a held note. **Revisit at
  M6 if the emitter needs it.**

- **The rarity corpus stops at 1929.** Every source is public domain, so `phone`, `jazz`,
  `explode` and `plastic` are absent and score as maximally rare. For a poem about a mobile phone
  that is simply wrong, and no public-domain corpus can fix it. Live with it, or accept a
  non-redistributable table behind a feature flag. **Not urgent; revisit if a modern poem misfires.**
- **α and β are both 1 and unmeasured.** `SPEC.md` §4.4 gives the formula's shape and no
  magnitudes. Nothing has been auditioned, so an unequal split would be a guess dressed as a
  decision. **Tune at M6, when there is something to listen to** — §9 expects exactly this.

- **§4.1's measure cannot see period, only spread.** Relative variance is computed from `n`, `Σx`
  and `Σx²`, so it is permutation-invariant: `[8,6,8,6]`, `[8,6,6,8]` and `[6,8,8,6]` are one
  stanza to it. A ballad passes for the same reason its shuffle does. Measured in both directions
  and pinned by `known_limitations` in `prosody-lts/tests/roles.rs`: a limerick (0.0938) and a
  Sapphic stanza (0.1191) are rigidly fixed forms the pass calls ragged, and **no threshold fixes
  them**, because Whitman's "Song of Myself" — correctly rejected — measures 0.0880. §4.1 asks for
  dispersion and *names* it periodicity; those are different things. An autocorrelation term would
  change what a poetic feature maps to, which `CLAUDE.md` §10 says is a spec change. **Raised, not
  decided.** It also means the Voice IR has nowhere to record loop *length*: a foundation stanza of
  `[8,6,8,6]` wants a two-bar loop and `[7,7,7,7]` a one-bar one. **M5c must decide whether to
  carry it.**
- **§4.1's caesura tier** — **resolved** as the call lane (`SPEC.md` §4.6). Only a line's *first*
  caesura is read; a line with several gives one call.
- **The role corpus is 16 hand-picked stanzas.** The threshold's window is (0.0364, 0.0880) and the
  test asserts the window rather than an accuracy score, precisely because a percentage over a
  self-chosen corpus grades the corpus. Two canonical free-verse pieces (H.D.'s *Oread*, a short
  Whitman passage) do classify as regular — defensible, since §4.1 asks about line-length evenness,
  but it means agreement with "the forms a reader would name" is weaker than 16/16 suggests.
  **Revisit when the golden corpus lands at M8.**
- **Foot count has no measured accuracy floor.** It is half of §4.1's rule and derives from the
  same lexical-stress vector that makes `dominant_foot` 50–57% accurate. `scansion.rs` has
  hand-worked cases but no floor of the `accuracy.rs` kind. **Add one at M8.**

- **Word families are order-dependent, like rhyme classes.** A word joins the nearest family
  *built so far*, so editing an early stanza can change a later word's `WordVoiceId`. Depth is now
  order-independent — measured from each family's shortest form — but membership is not. Settled at M5b for roles: locality is preserved wherever it can be, and the two places it cannot
  be — the free-verse fallback and the refrain lift — say so in the output. Word-family membership
  still has no such disclosure. **Fold into M5c.**
- **A token can be both a family member and a rhyme.** "burning" may be a variation on "burn"
  (§4.3, timbre and modulation) *and* rhyme with "turning" (§4.5, pitch). The two passes stay
  structurally separate — verified, no shared state — but nothing says how a mapping should
  reconcile both signals on one word. **Deferred to M5c**, where a token's role and its voice meet.
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
- **Short poems & looping-as-style** (`SPEC.md` §9): **resolved** — looping is structure
  (`SPEC.md` §4.6), not style. Still open for short poems: a single-stanza poem never loops, because
  nothing follows it to take over.
- **Non-English input** (`SPEC.md` §9): only the trait boundary is in scope; no second
  phonemiser implementation planned.

---

## Current iteration

**None. Every milestone in the table above is done, and `SPEC.md` §7's build order is complete.**

What exists: a poem compiles to Strudel pattern text or to a schedule of OSC messages, with a
live-reload watcher; every mapping in §4 is implemented; all four invariants of §6 have property
tests; §8's acceptance test and debug view both exist; and the golden corpus of §7.2 covers all
seven cases with committed snapshots.

What does not: the open questions below, which are the honest list of what a second pass would
argue about. Several are spec-level — §4.1's measure cannot see period, §4.5's degrees are
poem-wide, pitch reaches only the rhyme words — and were raised rather than decided because
`CLAUDE.md` §10 says changing what a poetic feature maps to is a spec change.

The next iteration has no milestone to take. It should either take an open question, or stop.

**Since then: `prosody-score`**, the measuring half of a tuning loop (see the log). It gives every
open question above a number that moves when the question is answered, so the next iteration can
say what its change bought.

### The machine cycle (branch `machine-cycle`)

The user's goal: steer the output towards Daft Punk, Kraftwerk, Laibach's electronic songs, and
psytrance — repetitive, layered machine music — through an automated measure-and-refine cycle, on
its own branch.

**The target** is `prosody-score`'s `machine` profile (`--score --profile machine`): the five
interest metrics with genre bands, plus `pulse` (beats with an onset on them) and `steadiness`
(bars as long as the bar before). The bands are reasoned guesses about those records, written next
to each constant — not measurements of them. Distrust a gain your ears do not confirm.

**One iteration:**

1. `./scripts/cycle.sh` — the table, and the mean overall it ends with.
2. Pick the metric furthest out of its band across the most poems. Name the poetic feature that
   should move it — a mapping, not a knob: the cycle must not buy a number by ignoring the text.
3. Implement it with hand-worked tests, run every gate, and re-run the script.
4. **Keep** it only if the mean overall rose and no poem's legibility fell without an explanation
   that names the mechanism (as the held and refrain lanes' entry does). Otherwise revert.
5. One commit per kept iteration; a row in the ledger below; the snapshot diff read before it is
   accepted. A reverted iteration gets a row too, so the next one does not retry it.

**Every mapping this cycle adds is a spec change made on a branch, pending the user's review**
(`CLAUDE.md` §10). Each is marked so in `SPEC.md`.

| # | Change | Mean overall | Kept |
|---|---|---|---|
| 0 | Baseline: the machine profile over `9b9c67c` | 0.410 | — |
| 1 | Bass: a foundation line's primary-stressed vowels pitched by height | 0.504 | yes |
| 2 | Bar length is the stanza's period; shorter lines rest instead of stretching | 0.611 | yes |
| 3a | *Measurement fix, no change to the sound*: the pitched synth counted as an instrument | 0.702 | yes |
| 3b | The outro: when the poem ends, the loops play one more pass | 0.743 | yes |

---

---

---

## Proposal — pending the user's decision: the structural half of jazz

**Not implemented. Nothing below is in the code.** Sound and voice step 3 built the jazz *palette*
(the log entry below). What makes jazz jazz beyond its colours — chords that move, and a bass
that walks between them — adds events and decides where they fall, so it is structure (I3) and a
change to what a poetic feature maps to (`CLAUDE.md` §10). Two options for each, for the user to
pick from or reject. Whichever is chosen joins `prosody_core::mapping` as a candidate, so the A/B
table on the page, `--without` and `./scripts/cycle.sh jazz <off>` can compare it with its absence
and with the other option.

### A harmony layer — a progression derived from the poem

`SPEC.md` §4.6 already names "a real chord progression" as the natural second overrule, needing a
harmony layer that does not exist. Either option adds one chord event per bar of a foundation
stanza, in a fourth lane, its span the rhyme word of the line the bar hosts (I4); the emitter voices
it through the palette's `Voicing` (so a chord is still palette in its *colour*, and structure in
*when it changes and to what*). With a harmony layer the rhyme melody would go back to single notes
on top of it.

| | **H1 — the rhyme scheme is the harmonic rhythm** | **H2 — rhyme closure is tension and release** |
|---|---|---|
| Poetic feature | each line's rhyme class (§4.5), in the bar the line hosts | whether a line's rhyme is *open* (answered later in its stanza — the held lane's test) or *closes* one |
| Musical result | the chord on that class's degree, for the bar: ABAB → i–ii–i–ii, AABB → i–i–ii–ii, a villanelle → two chords | an open line → the dominant of its degree (*d*+4, V7); its answer → the chord on the degree (I); an unrhymed line → the subdominant (*d*+3, IV) or ii; the last bar of a stanza → a V–I turnaround onto the next stanza's first chord |
| What it reads as | the scheme you can see on the page, heard as changes | the suspension the held lane already sounds (§4.6), heard as harmony: ii–V–I falls out of ABAB |
| Strengths | the most traceable mapping possible — every chord is a rhyme class; one rule; loops with its stanza | real functional motion, which is what jazz harmony is; makes the held lane's "an open rhyme is a suspension" literal |
| Weaknesses | doubles what the melody already states (same degree); a class of one (free verse) gets no chord, so unrhymed poems have no harmony; inherits §4.5's poem-wide degree numbering (an early new rhyme transposes later chords — I2's open question) | more invented: "unrhymed → IV" and the turnaround are choices, not readings; fewer distinct chords per stanza; the turnaround looks ahead to the next stanza, so an edit to stanza *n*+1's first rhyme changes stanza *n*'s last bar (I2 crosses a stanza boundary — must be disclosed like the refrain lift) |
| Scorer expectation | pitched ↑, entropy roughly unchanged, layering +1 in foundation bars | pitched ↑, entropy ↑ (V and IV add degrees), recurrence ↓ slightly at turnarounds |

A/B: `Mapping::Harmony` (H1) and `Mapping::Cadence` (H2) — names to taste — both in
`Mapping::ALL`. Off, a bar has no chord, which is today's piece. Both on, H2 decides where it
applies (open and closing lines, turnarounds) and H1 fills the rest, so the page's A/B table can
compare all four combinations (A = without `cadence`, B = without `harmony`, and so on).

### A walking bass — from the vowel-height bass

Today's bass (*machine cycle 1*, `Mapping::Bass`): every primary-stressed syllable of a foundation
line is a note — open vowel root, mid third, close fifth. A walking bass is a note on every beat,
moving by step between chord tones.

| | **W1 — the syllables walk** | **W2 — the feet walk** |
|---|---|---|
| Poetic feature | every syllable of a foundation line; the stressed ones keep today's height note, and the unstressed ones between two stressed syllables become passing tones | each *foot* (§4.2's beat): its stressed vowel's height chooses which chord tone the beat lands on, and whether the next foot's vowel is higher or lower chooses the direction of the step to it |
| Musical result | scale steps from one height note to the next, the last syllable before each stressed one a chromatic approach tone (a semitone below the target) | one bass note on every beat, outlining the bar's chord (needs H1 or H2; without one, the scale's root/third/fifth stand in) |
| Strengths | no new rhythm — the bass plays the poem's own syllables, so it stays exactly as traceable as now (I4: one note per syllable); works without a harmony layer | the idiom itself: quarter notes on the beat, pulse ↑; the bass and the ride agree; vowel contour becomes melodic contour |
| Weaknesses | walks in syllables, not beats — an anapaest walks three notes a beat, a spondee one — so it reads as a busy bass line more than a walking one; adds events on every unstressed foundation syllable (pitched share ↑ a lot) | moves bass events from syllables onto the beat grid, so `Mapping::Bass`'s mapping is *replaced*, not extended; a passing note's span is a whole foot, coarser than a syllable; depends on harmony to sound like walking |
| Scorer expectation | pitched ↑ (likely over the jazz band's 0.85 on dense poems), entropy ↑ | pulse → 1.0, entropy ↑, syncopation unchanged (the bass is on the beat) |

A/B: `Mapping::Walk` with W1 or W2 behind it (or both, as two names); off falls back to today's
vowel-height bass, which stays its own switch, so "no bass", "height bass" and "walking bass" can
all be heard against each other.

### What the jazz profile says about this

`./scripts/cycle.sh jazz` (the log entry below) is furthest out on **syncopation** for most poems —
Blake 0.017 against a band of 0.45–0.9. That comes from stress → accent (§4.2), and neither the
palette nor either proposal touches it: swing moves the off-beat syllables, not the accents, and
the scorer reads the score before swing. If jazz is the target, syncopation is the question to
answer first, and it is a §4.2 question.

---

## Log

### Sound and voice, step 4.2, review fixes

- **OSC names what it does not apply.** The schedule sends pitch from MIDI 36 and velocity
  unscaled, so `bass_octave`, `melody_octave` and `density_level` never reached it, silently.
  Now a header that moves one of them gets `# not applied here: bass_octave = 1, density_level =
  1.00 (pitch is sent from MIDI 36, velocity unscaled)` in `--osc` text and one stderr line per
  key on `--send` (`CLAUDE.md` §6). The messages themselves are unchanged (tested). Sample keys
  (`ride_fallback`, `bank`, `kit`) are not listed: the schedule names no sound at all. Under the
  `jazz` and `warm` presets the line appears for `density_level = 0.50`.
- **Explain views align.** The CLI's and the page's explain views padded keys to 12 and the new
  keys are 13; both now pad to the longest key, so every value shares one column (tested in
  both crates by column, hand-worked: byte 14 on the page, 16 in the CLI). The page's
  `mappings` row moved one column with it.
- **Kept:** the pattern header still lists the four new keys only when they are set — listing
  them always would change every snapshot, which the step ruled out. The explain view lists all.

### Sound and voice, step 4.2 — the register, the ride's stand-in, and levels that follow density

Three palette keys the user asked to be able to change. All palette (I3): no event, onset or
pattern step moves; `prosody-core` is untouched, and no mapping changed.

- **`bass_octave` (1–3) and `melody_octave` (3–5)**, defaults 2 and 4 — today's MIDI 36 and 60,
  which the Strudel backend had hard-coded. `style::Octave`, whole numbers only (an octave of
  `2.5` is refused, not rounded). Hand-worked: bass octave 1 and melody 5 put "rot bAn" at 27
  and 72; both at 3, 51 and 48. OSC still sends every pitch from 36 (it never split the roles).
- **`ride_fallback = "open_hat" | "crash"`**, default `open_hat` — what keeps time under the
  ride kit on a bank without a ride. The `plays:` line names it ("crash in place of the ride,
  which RolandTR808 does not have"); a bank with a ride ignores it. The every-bank test now walks
  bank × kit × fallback × layer, and a test checks that crash and open hat differ in the sample
  name and nothing else.
- **`density_level` (0–1)**, default 0 — every gain in a bar × *n*^−level, *n* the lines sounding
  in that bar; 0.5 is ÷√*n*, 1 is ÷*n*, a bar of one line never moves. Folded into each event's
  `.gain` value on the layer's own grid (`grid_by_slot`), not a new parameter pattern, so the
  arrangement check sees no new shape. It reads the score's structure (lines per bar) and writes
  only gain values; a new I3 property checks exactly that (the output differs from level 0 in
  `.gain` values alone, each no louder; a mutation halving the exponent failed the unit test).
  **I2 disclosure** (`SPEC.md` §5): above 0, a shared bar's levels depend on how many lines share
  it, so an edit to one stanza can change the *level* of another stanza's line in a bar they
  share. The score is untouched, so I2's properties still hold; the emitted gains of a shared bar
  are not stanza-local.
- **Presets.** `warm` and `jazz` now set `density_level = 0.5`; `default` and `raw` keep 0. Every
  preset also restores octave 2/4 and the open hat (all four keys joined `preset_keys`).
- **Visible.** The explain view always lists the four keys; the pattern header lists them only
  when they differ from the default, so a pattern not using them is byte-identical to before, and
  with a density level it adds one line naming the factor for each line count the piece has. Page
  controls: Ride stand-in (menu), Bass octave and Melody octave (sliders), Levels follow density
  (mix slider).
- **Snapshots.** Only `pattern-villanelle-jazz` changed, because the jazz preset now sets the
  density level: two header lines added, and gain values only — checked by script against the
  old snapshot with every `.gain(…)` removed (identical) and every gain ratio in {1, 0.707,
  0.577}, the villanelle's bars of one, two and three lines. Read.

**Measured** in Chromium on port 4882: the peak of Strudel's summed output tapped by an
`AnalyserNode` at the superdough destination gain, before the page's duck, compressor and soft
clipper (knee 0.6, ceiling 0.95), each example with the palette its own header sets, setting A:

| poem | window | level 0 | 0.5 | 1 |
|---|---|---|---|---|
| blake-tyger | first 12 s | 0.788 | 0.461 | 0.272 |
| blake-tyger | first 30 s | 0.838 | 0.462 | 0.304 |
| wilde-theocritus | first 12 s | 0.731 | 0.558 | 0.469 |
| wilde-theocritus | first 30 s | 0.950 | 0.655 | 0.476 |

Wilde, 30 s, under the Sound menu's presets: warm 0.802 at 0, 0.614 at 0.5; jazz 0.718 at 0,
0.512 at 0.5 — hence 0.5 for both. At 1 the sparse bars keep their level and the dense ones drop
far (Blake ÷4 in its four-line bars), which may sound like the texture thinning rather than a
mix; that is for listening. Also checked: jazz + `tr808` + crash + octaves 1/5 plays with no
console errors or warnings, `RolandTR808_cr` and both soundfonts load (200), and the Sound menu
reads *jazz* after choosing it (density 0.5) and *custom* after a control moves. No one has heard
it. Cycle tables unchanged: machine mean overall 0.743, jazz 0.740.

### Sound and voice, step 4.1 — every candidate mapping is a selector

The user chose selectors with named variants over flat on/off switches, and asked for the reading
to become one: off-rhythm beats heard in the output were lines being read several bars after the
music had moved on, because a transforming stanza brings one line in a pass.

- **`prosody_core::mapping`** is generalised. Each `Mapping` has a fixed, ordered list of `Variant`s
  (always starting with `off`), a default, and a name per variant. `Mappings` stores only the
  non-default choices (a `BTreeMap`), so equal choices are equal values; it is changed only by a
  `Choice`, which only `Mapping::choose`/`choose_named`/`off` can make, so a variant a mapping
  lacks cannot be stored. `Mappings::all()` is every default; `has()` is "not off"; `get()` reads
  the variant; `changed()` lists non-default choices; `every()` enumerates all 96 combinations.
  The five machine-cycle switches are `off`/`on`, default `on`: no lowering, snapshot or score
  changes, and `./scripts/cycle.sh` still reads mean overall 0.743.
- **`Mapping::Reading`** joins the list: `off`, `arrives` (default — today's reading) and
  `stanza` (a stanza's lines one a bar from its first hosting slot). `reading::plan` takes the
  mappings; the cue pattern's first comment names the variant. Hand-worked on the TRANSFORMING
  fixture: `arrives` reads slots 0 1 2 5, `stanza` slots 0 1 2 3, `off` nothing; on the seed
  poem `arrives` reads lines 5–8 in slots 4 9 14 19 and `stanza` in 4 5 6 7.
- **Interfaces.** CLI `--set name=variant,…` (unknown name or variant → an error naming it and the
  variants it has), `--without` kept as shorthand for `=off`; `--explain` lists non-default
  choices. Web endpoints take `set=name:variant,…` and still `off=`; unknown → 400 naming it.
  `scripts/cycle.sh`'s second argument goes to `--set` when it contains `=`, else to `--without`.
- **The page.** The A/B table is one `<select>` per mapping per side; the separate "read each line
  aloud" checkbox is gone (the voice picker and the duck stay). Saved settings migrate: the old
  `{A: [left out], B: [...]}` becomes "those off, the rest default", and an old `prosody.read=off`
  turns the reading off on both sides; an unknown name or variant falls back to the default. B
  starts with every mapping off, *including the reading* — so B is silent of speech by default,
  where before the reading was on for both.
- **Tests.** The invariants' I1 and "invents nothing" properties draw from every variant
  combination (`proptest::sample::select` over `Mappings::every()`), not a 5-bit set.
- Checked in Chromium on port 4881: old saved settings migrated, `reading = stanza` cued lines 1–8
  in bars 1–8 on The Tyger, B with the reading off cued nothing, every request 200, no console
  errors.

### The page plays at the stated tempo

Fixes the half-tempo defect recorded in *Sound and voice, step 2* below, which also corrects that
entry: the defect was **the page's only**, not strudel.cc's. `@strudel/core` 1.2.6's `.cpm(x)` is
`fast(x / 60)` and assumes one cycle a second, while the scheduler starts at 0.5 (`cyclist.mjs`).
strudel.cc evaluates through its REPL, which redefines `.cpm` as `fast(x / 60 / scheduler.cps)`
before running the code (`repl.mjs`, `injectPatternMethods`), so a pasted pattern was always at
tempo. The page evaluates with a plain `eval`, gets core's `.cpm`, and played at half.

- **The fix is on the page**, not in the text: after `initStrudel` the page sets its scheduler to
  one cycle a second (`repl.setCps(1)`). The emitted pattern is unchanged, so no snapshot moves,
  and the same text is right on both paths.
- `reading::SCHEDULER_CPS` is now `1.0`, and must agree with the page. The reading's hand-worked
  rate tests were re-worked at half their tempos (120 bpm over two-beat bars is a one-second bar),
  so their expected rates are the same numbers.
- Measured in Chromium on the Blake preset (`.cpm(30.250)`): cues now fire 1.98–2.0 s apart (were
  3.97 s), and the lines read at 1.04–1.19× instead of the 0.6 floor.
- Everything played on the page is now twice as fast as before, as the header always said. Tempo
  ranges in the presets were tuned by ear at the old speed, so they may want halving or a look.

### Sound and voice, step 3 — review fixes to jazz

Four findings from review, all palette-side; no event moves and no mapping changes.

- **A ride on a bank with no ride muted the pulse.** `kit = "ride"` asked every bank for `rd`, and
  the TR-808 and TR-707 in `tidal-drum-machines.json` have none, so `preset = "jazz"` with
  `bank = "tr808"` made Strudel log `sound RolandTR808_rd not found!` on every hit and the whole
  foundation layer (§4.1, the pulse) play nothing. The text was unchanged, so I3 held on paper, but
  a palette choice silenced a layer. The emitter now carries the sample table of each bank (checked
  against the sample map, 2026-10-22), and a bank without a ride keeps time on its open hat (`oh`,
  which every bank has) — the drum machine's usual stand-in. The header says so: "open hat in place
  of the ride, which RolandTR808 does not have". Rejecting the pair at the style boundary was the
  alternative; it would have made Sound = jazz then Bank = tr808 on the page an error rather than a
  sound. A test walks every bank × kit × layer and checks the sample is in the bank (a mutation
  restoring `rd` everywhere failed it).
- **The Sound menu kept saying jazz after its scale, swing or bank changed.** The page re-checked
  the menu only for the sound keys; it now re-checks for any key some palette sets, so it reads
  *custom* and jazz can be chosen again to restore them.
- **"Seventh chords" in a scale without seven notes.** Stacking every other degree builds a
  seventh only in a seven-note scale; in `pentatonic_minor` it stacks fourths (C F B♭ E♭), in
  `whole_tone` and `chromatic` something else again. The header now says "four-tone chords on
  every other degree of pentatonic_minor (not sevenths: the scale has 5 notes, not 7)" there, and
  `Voicing` documents it. Hand-worked test: C pentatonic minor, 60 65 70 75 and 63 67 72 77.

### Sound and voice, step 3 — jazz: the palette, a profile, and a proposal

The palette half of jazz, all palette (I3 untouched), plus a way to measure it and a written
proposal for the structural half (the section above, pending the user's decision).

- **Two new palette keys.** `kit = "machine" | "ride"` picks which drum carries each layer: kick
  and hat, or ride (the foundation, the time) and snare (the flourish, comping). `sample_for` had
  always called this palette; now a palette can choose it. `voicing = "note" | "seventh"` voices
  each rhyme note as the seventh chord on its degree — degrees *d*, *d*+2, *d*+4, *d*+6 of the
  style's scale — written as one mini-notation step `[60,63,67,70]`, every tone taking the event's
  one gain, cutoff and stray, the melody level shared at 1/√4 per tone. The argument against I3 is
  on `prosody_emit::Voicing`: same events, same onsets, built from the event's own degree and the
  scale alone, never from another event, so it cannot carry a progression.
- **A bank, `r8`** (Roland R-8 → `RolandR8`), the sampled acoustic kit. Checked against
  `tidal-drum-machines.json` on `strudel.b-cdn.net`: none of its 71 machines has brushes; the R-8
  has a real ride (`RolandR8_rd`) and snare. Closest available, not a brushed kit.
- **The `jazz` preset**: Rhodes (`gm_epiano1`) sevenths over the upright (`gm_acoustic_bass`, GM
  program 33), `kit = "ride"` on `r8`, triplet swing `0.33` (`swingBy(x, n)` delays the second half
  of each beat by `x / 2` of a beat — `@strudel/core` `pattern.mjs` in `@strudel/web` 1.3.0 — so a
  third gives 2:1), dorian, a warmer, darker room, more stray. The first preset that sets more than
  the sound and mix; `style::preset_keys` says which keys each preset sets, and the page's Sound
  menu applies exactly those (warm after jazz restores the sound and leaves dorian). Levels are an
  estimate from the default's measurements, **not measured**.
- **A `jazz` scoring profile** (`Profile::Jazz`, `--profile jazz`, the page's Score tab,
  `./scripts/cycle.sh jazz`): layering 2–4, recurrence 0.65–0.85, pitched 0.4–0.85, entropy 2–3.3,
  syncopation 0.45–0.9, pulse 0.75–1, steadiness 0.7–1 — reasoned guesses, reasons beside each
  constant. Swing is palette and the scorer reads the score, so it is not measured at all.
  `Profile::ALL` and `Profile::name` replace the hand-written match.
- **Tests.** I3 property: one palette rendered under `note` and `seventh` voicing — every non-melody
  line byte-identical, the melody's steps identical, each chord four tones whose lowest is the
  note, thirds between them, and only the level changes beside it (a mutation that shifted the
  chord's root failed it). The arrangement property now treats a chord as one value and its
  palette generator varies `kit` and `voicing`. Hand-worked units: D dorian Dm7 = 62 65 69 72 and
  Em7 = 64 67 71 74; the ride kit's skeleton equals the machine kit's; the jazz preset's values;
  jazz bands against machine's (recurrence 0.95 fits machine 1.0 and jazz 0.444). Snapshots: the
  jazz report for every poem (values identical to machine's, bands and fits differ — read), and
  `pattern-villanelle-jazz` (Cm7 and Dm7, the two-note obsession as two chords, swing at 5 —
  read). Every existing pattern snapshot changed in its header comment only: `kit = machine`,
  `voicing = note`, and the `plays:` line naming what each drum layer plays.

Verified in Chromium: the page loads, the Sound menu's jazz sets scale, swing, bank, kit and
voicing; Play evaluates with no console errors or warnings; the R-8 ride and snare and both
soundfonts (`0040`, `0320`) load; the melody lane's pattern queried in the page gives 104 haps
for 26 written chords — four tones each, one parameter set per onset, nothing multiplied. The
Score tab measures under jazz. No one has heard it.

Cycle tables after this change (no mapping changed, so the machine table is as before):
machine mean overall 0.743; jazz 0.740.

Open: OSC does not voice chords (it sends the event's one pitch; the receiver could). The jazz
levels want the peak measurement the default had. Whether a ride on every foundation syllable is
too busy.

### Sound and voice, step 2 — review fixes to the reading

Three findings from review, all on the page side of the reading (`SPEC.md` §4.7):

- **A pasted line separator stopped the piece.** The cue pattern copies each line's text into a
  `//` comment, and JavaScript ends a comment at U+2028/U+2029 as well as at a newline, so the
  rest of the line ran as code and Play failed. `reading::comment_safe` now writes those, and
  every control character, as `\u{…}` escapes in the comment. The text the page speaks, carried
  in the cue's JSON, is the exact source and is not escaped. The page also evaluates the cue
  pattern in its own `try`, so a reading that cannot be cued costs the reading and the music
  still plays.
- **A preset or A/B switch started the reading mid-poem.** Strudel's scheduler keeps its cycle
  count across `setPattern` (`@strudel/core` 1.2.6 `cyclist.mjs`: only `stop()` resets `lastEnd`
  and only `start()` resets the counters), so the new piece came in partway through. Those two
  switches now `hush()` and start again at cycle 0. Live edits deliberately keep the clock
  running, and that is documented.
- **A stale line could be read over the new music.** Every `play()` starts a new reading
  generation. Cues and pending timers from an older generation are dropped, and `quietReading()`
  cancels the line being spoken, at the moment the new pattern starts.

### Sound and voice, step 2 — the poem read aloud over the music (page-side candidate)

A prototype, to learn whether spoken lines over the music work. `prosody_emit::reading` plans
it: each line once, in the slot where it is first heard as a host (`Stated`, `Arriving`,
`Overruling`), never on loops, yields, the call/held/refrain lanes or the outro (`SPEC.md` §4.7).
It is a **separate artefact**, not part of the Strudel pattern text: Strudel cannot speak, a cue
pattern pasted into strudel.cc would ask the default output for a sound named nothing, and keeping
it apart leaves the CLI output and every corpus snapshot untouched (none changed). It lives in the
emitter rather than the page because it must land on the pattern's own clock — the same `.cpm`,
now one helper, `cycles_per_minute`, shared by both.

The page fetches `/reading` per setting (the arrangement depends on the mappings) and gets JSON:
the cue pattern `n("<0 1 ~ 2 …>").cpm(…)` with a comment per cue (bar, line, bytes, syllables,
rate), and the cues themselves. It evaluates the cue pattern, attaches `onTrigger(fn, true)` —
checked in `@strudel/core` 1.2.6's `pattern.mjs` and `repl.mjs`: the callback receives
`(hap, currentTime, cps, targetTime)` and `true` suppresses the default output — waits out
`targetTime − currentTime` as Strudel's own `onTriggerTime` does, and speaks the line through
`speechSynthesis`, cancelling one still running. Controls: on/off (default on, remembered), a
voice menu from `getVoices()`, and a duck (the master to 0.6 while a line is spoken, a `GainNode`
ahead of the master compressor). Rate: syllables at four a second fitted to 85% of the bar,
bounded 0.6–1.6.

Found on the way, both in the browser:

- **`speak`, `duck` and `silence` are Strudel globals.** `initStrudel` puts them on the global
  scope over any page function of the same name; the first version's `speak` was silently
  replaced by Strudel's pattern function and said nothing, with no error. Renamed `readLine`,
  `readingDuck`, `quietReading`.
- **The piece plays at half its stated tempo** — a pre-existing defect, *not fixed here*.
  Strudel 1.2.6's `.cpm(x)` is `fast(x / 60)`, assuming a scheduler at 1 cycle a second, but the
  scheduler starts at 0.5 and nothing changes it, so a bar at `.cpm(x)` lasts `120 / x` seconds.
  Measured: the Blake seed is `.cpm(30.250)`, and its cues fired 3.97 s apart, not 1.98 s. The
  same holds for a pattern pasted into strudel.cc. Fixing it (emit `.cpm(2x)`, or have the page set
  the scheduler to 1 cps) doubles the speed of everything heard and changes every snapshot, so it
  is left for the user to decide. The reading is fitted to the bar as it actually sounds
  (`reading::SCHEDULER_CPS`), which is why every Blake line currently reads at the 0.6 floor.

Verified in Chromium through Playwright: the page loads, both readings fetch, Play evaluates the
stacked pattern with no console errors or warnings, the cues log lines 1–5 one bar apart and line
6 at bar 10 (the transformation's next arrival), the utterances start (180 voices were available),
the duck reaches 0.60 and returns, unticking the reading while it plays stops the cues, and Stop
cancels speech. No one has heard it.

### Sound and voice, step 1 — it sounds like instruments now (palette only)

The user listens through the web page. Making it sound good there turned up two defects first,
both invisible from the text and both fixed here:

- **The drums were silent on the page.** `initStrudel()` loads no samples, so every
  `.bank("RolandTR909")` asked for a sound nobody had registered (`sound RolandTR909_bd not
  found`, in the console of the unchanged page). The page now loads the drum-machine map
  strudel.cc uses. Strudel's effect worklets also loaded only on the *next* mousedown after Play,
  so the first play could not build `.crush`; the page now loads them itself.
- **Stacked lines multiplied events.** Where a layer sounds two lines in one bar (a stanza looping
  under the next, a held or call lane), the emitter stacked them with `,` inside every pattern.
  Strudel matches each chained parameter pattern to the events by time, so each event met both
  lines' gains, cutoffs, crushes… and became one event per combination: 2 events → 8 with two
  parameters (checked in the browser), 32 hits per syllable on a drum layer, and a bass bar that
  peaked at 12× full scale. Events not in the score are I4 broken. Each simultaneous line is now
  its own stack element (`Lane`, labelled `(lane 2)` and so on), so every parameter meets exactly
  one event. Verified on the corpus: the written drum hits and notes are the same count as before
  in every snapshot, only split across lanes; a test pins hits = `sounded(score)`.

Then the palette (`prosody_emit::style`, I3 untouched — the score is not readable from there):

- **Instruments by role.** The pitched layer splits by `PitchSource` into the melody (rhyme
  words) and the bass (stressed foundation vowels), each with its own instrument: any synth name
  or a curated list of General MIDI soundfonts. The melody moved up two octaves (MIDI 60; it had
  sat with the bass at 36). `karplus`, `fm` and `physical` now play the recorded instrument
  nearest the model (they were all bare oscillators); the waveforms stay oscillators. The page
  loads `@strudel/soundfonts@1.3.0` through an import map that answers its `@strudel/core` and
  `@strudel/webaudio` imports from the Strudel already on the page (`strudel-globals.mjs`) — a
  second copy from a CDN would register the fonts where the playing Strudel never looks.
- **A mix.** `reverb`, `room_size`, `delay` (melody), `high_cut`, `drive`, `compress`, and a level
  per role, all range-checked at the boundary. `high_cut` scales §4.4's brightness range instead
  of clipping it, and `reverb` raises the floor of §4.4's dry/wet rise, so the mappings keep their
  direction under every palette. A master stage on the page (not palette) catches the sum: a
  gentle compressor, then a soft clipper (a `WaveShaperNode`, transparent below 0.6) whose output
  cannot pass 0.95. Levels were set by measuring peaks in the browser, not by ear; see the review
  note below. A role's level is never zero (`0.1..=2`), so the palette cannot mute a role — a
  silent event is as untraceable as one behind a closed filter.
- **Review fixes.** The first levels (0.50/0.55/1.30) were tuned on a measurement taken too early
  in the piece. Measured over twelve seconds they peaked at 1.6 (opening poem) to 2.7 (the Wilde
  villanelle, which sounds the most lines at once) before the master, and the page's
  `DynamicsCompressor` limiter let drum onsets through above full scale (1.18 on Wilde) — its
  2 ms attack is too slow and Chrome adds makeup gain after it. The soft clipper is now the
  ceiling, and the default levels are 0.28/0.32/0.75 (warm 0.24/0.24/0.60), which bring each role
  to about 0.5–0.6 alone on the opening poem. The register a role plays in (bass 36, melody 60) is
  fixed in the Strudel backend; the docs had called it palette, which no header key could set.
- **Humanise.** Each syllable up to 30 ms late and 30% softer at `humanise = 1` (`.nudge`,
  `.velocity`), from its line's voice seed under a `humanise` child, restarted per bar. Strudel
  honours `nudge` only for samples, so the page wraps every other sound to honour it too.
  Property tests: reproducible; an edit in one stanza leaves every line of another straying bit
  for bit as before; strays scale exactly with the amount and stay in range.
- **Presets.** `preset = "default" | "warm" | "raw"` in a header, and a **Sound** menu on the
  page. `raw` is the old palette (triangle, dry, on the grid) so the change can be heard against
  it. A headerless poem gets `default`.

The I3 arrangement test now compares, per layer, the *set* of pattern shapes, since `humanise = 0`
leaves `.nudge` and `.velocity` out; every pattern is laid on its layer's grid, so a palette can
add one without adding a shape. Its palette generator now varies every new key.

Open for listening: whether default or warm is the better starting point; whether the bass, which
sounds on every stressed foundation syllable, is too busy against the melody now that both are
audible; and whether 30 ms is too much stray at the top of the range.

### Auditioning the candidates — A/B on the web page

The user asked for a way to evaluate each candidate mapping in the frontend, and to keep it as a
long-term artefact. `prosody_core::mapping` names five — the held lane, the refrain lane, the bass,
the stanza period, the outro — and `lower::run_with` takes the set to apply; `lower::run` is all of
them. It is deliberately not part of `Style` (I3). Left out, each restores what preceded it; for the
stanza period that needed `Slot`'s `Fit::Stretched`, since before cycle 2 a placement was stretched
across the bar rather than resting. Two fixes that are not candidates have no switch: the call lane
no longer doubling its own line (a bug fix), and 3a (measurement only).

The web page gets an A/B table of checkboxes, a switch that swaps the playing pattern live, and a
**Score A vs B** tab fed by a new `/score` endpoint (JSON; the machine profile by default). The CLI
gets `--without`, and `cycle.sh` a second argument passing it through.

Tests: one per switch with the arrangement worked by hand; a property test that any of the 32
settings is reproducible and only ever removes material; web tests for the query, the score JSON
and the shorter piece without the outro.

Each mapping left out, machine profile, mean overall over the nine examples (all on: 0.743):

| left out | mean overall |
|---|---|
| held lane | 0.723 |
| refrain lane | **0.755** |
| bass | 0.564 |
| stanza period | 0.604 |
| outro | 0.702 |

The refrain lane — from options 1 and 2, before this cycle — scores *better* left out under the
machine profile. It was judged under the interest profile; whether it stays is for listening.

**First listening (2026-10-09).** The user, on the Tyger: A — every candidate on — sounds much
better than B, every candidate off. The first verdict from ears rather than bands, and it agrees
with the bands' direction (the page's two-stanza Tyger: overall 0.886 for A, 0.684 for B). It judges the set, not each mapping;
the per-mapping comparisons are still to hear. The page also gained a menu of the example poems,
each loading the palette its header sets, so the corpus can be auditioned without pasting.

### Machine cycle 3b — the outro

After 3a the bottom two were the one-stanza poems: Prešeren 0.412, Sonnet 18 0.611, both with
recurrence near 0.5 against 0.85–0.97 — `SPEC.md` §9's "a single-stanza poem never loops, since
nothing follows it". The rule is that a loop plays until something stops it, and the end of the
poem had been treated as the thing that stops it.

Now, when the poem runs out, the loops play one more pass: the last stanza again (`Cause::
Repeating`), with the other layer's loop and every lane under it. Sonnet 18 0.611 → 0.733,
Prešeren 0.412 → 0.503, Hopkins 0.757 → 0.857; mean 0.702 → 0.743. Legibility did not move for any
poem — an outro bar is hosted by a line of the last stanza, so the edit test's existing reasons
cover it.

Writing the outro exposed a doubling: a line's call kept sounding under its own line wherever that
line was re-hosted (the outro, and a transformation's yielding bars). The call lane now follows
the same no-doubling rule as the held and refrain lanes — silent wherever its whole line already
plays. The mean did not move.

Most emitter unit tests used one-line poems and asserted one bar; each now asserts both passes, with
the arithmetic in its comment.

### Machine cycle 3a — the scorer was not counting the synth

Not a musical change. Both backends play a pitched event on two instruments — its line's drum and
one shared synth (Strudel's `note()` layer, the receiver's `\prosodyPitch`) — and the timeline
counted it once. Before cycle 1 that mattered little; with the bass, half the syllables are
pitched, so layering and recurrence were undercounting what plays. Layering and recurrence now
count the synth as its own instrument; pitch share, entropy and syncopation still count events.

Mean overall 0.611 → 0.702, with not one note changed — recorded as its own row so the ledger
does not mistake it for progress in the music. A side effect worth knowing: the generic `interest`
profile now scores Wilde at 1.000, so its bands have stopped discriminating at the top.

### Machine cycle 2 — a bar lasts its stanza's period

After cycle 1, `steadiness` was furthest out: 0.00–0.85 against 0.8–1.0, because every bar took
the length of the line arriving in it, so the meter changed nearly every bar — in Dickinson, every
one. Machine music holds a meter.

Now a bar lasts its host's stanza's *period*, the longest line of that stanza. A stanza is a loop,
and a loop has one length; odd meters survive, since the period still comes from the poem. A line
that fits keeps one beat per foot and rests for the rest of the bar, instead of being stretched;
a longer loop is still squeezed in. `Slot::room_for` is the one rule all three consumers (OSC,
Strudel, the scorer's timeline) fit a placement by.

Measured: steadiness 0.79–1.00 everywhere; mean overall 0.504 → 0.611. Layering fell (Tyger 3.12 →
2.61) and so did pulse (Prešeren 1.00 → 0.64): the rests are real silence, which those metrics
count.

Legibility fell at first (Tyger 0.811 → 0.741, Dickinson 0.831 → 0.754) through one mechanism: an
edit to a stanza's longest line changes its period, re-timing every slot the stanza hosts, and the
loops fitted under them. The edit test only excused changes under the *edited line's* slots. A
first fix excused anything under the edited *stanza's* slots, and overshot — it lifted Dickinson
to 0.908, because it also excused the role flips the user chose to keep counting. The kept fix is
narrow: a line elsewhere is explained when its only change is the length of slots the edited
stanza hosts — same material, same slots, same causes. With it, every poem's legibility is
exactly its value before this iteration.

### Machine cycle 1 — a bass from vowel height

The machine profile's baseline (0.410 mean overall) was weakest on `pitched`: about a tenth of
events carried a note, against a band of 0.35–0.7, because only rhyme words were pitched. §4.1
calls the foundation "drums, bass, ostinato" and the foundation had no bass.

Now every primary-stressed syllable of a foundation line is pitched by its vowel's height — open
vowels on the root, mid on the third degree, close on the fifth — because height is the vowel
property with an acoustic claim to pitch (close vowels carry a higher intrinsic F0). §4.5's rhyme
note wins on a rhyme word. A `Pitch` now carries its source (`PitchSource::Rhyme` or `Vowel`), so
the edit test's rhyme link and the debug view (`bass 2 AH`) can tell them apart.

Measured: pitched rose to 0.30–0.74 across the examples, pitch entropy into its band for all nine
(Dickinson and Whitman had no melody at all, 0 bits; now ~1.56). Mean overall 0.410 → 0.504.
Legibility did not move for any poem — a vowel edit reaches its own bass note and nothing else.
Three hand-worked scorer tests changed for named reasons: an iamb line is now half pitched; `mI` →
`tE` is now audible (close → mid), `mO` → `tE` still is not (mid → mid); a silenced rhyme partner
leaves the survivor on its bass note, the same degree, so that edit became local.

Steadiness (0.00–0.85) and layering (1.2–3.1 against 3–5) are now the furthest out.

### §4.6's held and refrain lanes: an open rhyme sounds until it is answered

The user chose both, for the poems that still played one thing at a time (Wilde, Sonnet 18). A line
whose rhyme is answered later in its stanza is held under every bar until its answer is first
heard; a refrain line loops in its own lane until the next refrain line is first heard. No lane
doubles a voice already in the bar, which matters where a refrain is also a held rhyme or is
already in a layer's loop. `arrange::run` now reads the rhyme analysis.

Measured (layering, before → after): Wilde 1.000 → 1.869, Sonnet 18 1.000 → 1.275, Prešeren 1.000
→ 1.167, the villanelle 1.000 → 2.238, Tyger 1.957 → 3.124, Poe 2.222 → 2.941, Hopkins 1.840 →
2.520. Interest: Wilde 0.804 → 0.972, Sonnet 18 0.535 → 0.625, villanelle 0.651 → 0.725; Poe fell
0.688 → 0.664 (layering is near the band's top, pitch entropy fell). Whitman, free verse, nonsense,
one-line and Slovenian are unchanged — no in-stanza rhymes, no refrain lines.

Sonnet 18 gains less than hoped: only 4 of its 14 bars hold anything. Two of its B rhymes
(`temperate`/`date`, `dimm'd`/`untrimm'd`) fall below the class threshold, so their quatrains hold
only the A line for one bar, and the closing couplet answers itself at once.

Explained locality fell where refrains are probed: Wilde 0.918 → 0.820, Tyger 0.860 → 0.811.
Checked edit by edit against the previous commit: every new unexplained edit replaces a word of a
refrain line, which stops it being a refrain, so it no longer takes the lane over and the *other*
refrain loops on in its place. That is a real, audible, poem-wide consequence of "until the next
takes over"; kept as measured, like the role flips.

The Tyger's refrain lane holds `In the forests of the night` from bar 3 to bar 68 — its refrains
sit at the poem's two ends. Length and persistence are uncapped for now, by decision.

### §4.6's call lane: a caesura leaves a figure behind

The user's decision: §4.1's second tier, as a third lane. The half-line before a line's first
internal caesura becomes its call; from the bar after its line is first heard it loops under
everything, fitted to each bar, until the next line with a caesura is first heard. A refrain
stanza silences it. `Placement` now carries a `Lane` (a layer, or the call lane) and a `Part`
(the whole line, or a call's span), and `Score::placed` is the one place a placement becomes
events and a beat extent — the backends and the scorer stopped fetching a bar's events themselves.

Measured (before → after, layering): Tyger 1.000 → 1.957, Jabberwocky 1.000 → 1.667, Dickinson
1.000 → 1.619, Hopkins 1.400 → 1.840, Poe 1.745 → 2.222, Whitman 1.798 → 2.193. Interest rose for
every poem with a caesura (Hopkins 0.627 → 0.768, Jabberwocky 0.729 → 0.796). Explained locality
did not move for any poem, so the lane added no unexplained edits. Wilde, Sonnet 18 and Prešeren
have no internal caesura and are unchanged; the golden corpus's patterns and views are
byte-identical for the same reason.

Side effects worth knowing: syncopation fell where a call adds accents onto beats the line left
unaccented (Tyger 0.223 → 0.111, Dickinson 0.524 → 0.317), and the pitched share fell because a
call adds unpitched events. The Tyger is now 84 bars from 24 lines; length is deliberately uncapped
for now.

### After `prosody-score` — §4.6: stanzas loop until something takes their place

The user's decision, recorded in `SPEC.md` §4.6: a stanza keeps looping until a competing stanza
of its layer transforms it (one bar per pass) or a refrain stanza overrules every loop.
Bar-locked; the piece ends when the last stanza has arrived. New pass `prosody_core::arrange`,
run at the end of lowering, producing `Score::arrangement()` — slots of placed lines, each with its
`Cause`. Each line's *material* is untouched, so the I1–I4 property suite passed unchanged; the
backends, the debug view (`sounds …` per line) and the scorer now walk slots instead of lines.

Measured (scorer snapshots, before → after): every multi-stanza poem's recurrence rose from ~0 to
0.6–0.75; interest e.g. Wilde 0.668 → 0.804, Jabberwocky 0.527 → 0.729. Layering left 1.0 only where
layers alternate — Whitman 1.80, Poe 1.75, Hopkins 1.40.

What it did not fix, and why:

- **All-foundation poems morph but never stack.** Tyger, Wilde, Dickinson, Jabberwocky and the
  villanelle have every stanza in one layer, so each takes over the last and nothing sounds under
  it. Same-layer competition is the rule chosen; layering for these needs a second source of
  simultaneity (a caesura tier, §4.1's unbuilt second tier, is the obvious one).
- **Single-stanza poems are unchanged** — nothing follows them.
- **The role threshold now matters far more.** Explained locality fell for Dickinson
  1.000 → 0.831; all 11 unexplained edits of 65 flip a stanza between layers, which now re-routes
  the whole arrangement after it rather than swapping one sample. Raises the priority of
  *Role locality under a global threshold* below.
- **Long stanzas make long pieces.** A transformation takes as many passes as the new stanza has
  lines, so an `n`-line stanza replacing an `m`-line loop lasts about `n · max(n, m)` bars.

Not done: the overruling *chord progression* — no harmony layer exists; deferred by the user's
choice of refrain stanzas as the overrule for now.

### After M9 — `prosody-score`: measuring whether it is interesting, and whether it still listens

Goal: an automatic score a feedback loop can optimise against, without the loop drifting into
`SPEC.md` §1's failure mode. New crate, depends only on `prosody-core`, scores the `Score` IR —
structure, before any palette — and `prosody --score` prints it.

**Two halves, multiplied.** *Interest*: five counts over the timeline (layering, recurrence, share
pitched, pitch entropy, Longuet-Higgins–Lee syncopation on accents), each judged against a band
rather than maximised, since listener preference peaks at moderate complexity. The fit is
`1/(1+d)²` and the aggregate a geometric mean — the first version used `1/(1+d)` and an arithmetic
mean and rated a piece with no layers at all 0.75. *Legibility*: every word swapped for a probe, the
whole poem recompiled, and each edit judged *audible* (own bar changed), *local* (nothing outside
the stanza changed — I2 strictly) and *explained* (everything that changed outside the stanza
shared a voice, rhyme class, word family or anchor with the edited bar). Legibility uses
*explained*: strict locality scored the refrain corpus poem 0.349, every one of its 28 leaks being
§4.3 doing its job, and a loop chasing it would learn to write poems without refrains.

**What an edit is compared on** is what a backend renders. Ids numbered by first appearance —
`VoiceId`, `WordVoiceId`, `RhymeClassId` — are not compared; which bars share a voice is.
Otherwise breaking a refrain early renumbers every later voice and the measure calls silence
audible.

**Baseline** (committed as snapshots over the golden corpus and the nine examples):

- **Layering is exactly 1.000 for every poem.** One line is one voice in one bar, so nothing ever
  sounds alongside anything. This, not palette, caps what any tuning can reach.
- Recurrence is 0 for most poems; only refrains return. Pitched share 0–24%.
- Explained locality is 1.0 for strict-metre poems; lowest for Whitman (0.826), Blake (0.888),
  Wilde (0.918). Checked Whitman by hand: editing almost any word of stanza 1 flips stanza 2 from
  flourish to foundation — the *role locality under a global threshold* question below, now
  measured. The hand-worked test `a_degree_renumbering_is_an_unexplained_leak` pins the other
  known cause.
- New finding: swapping one unrhymed stressed monosyllable for another is **inaudible** — the IR
  carries a vowel only through rhyme, alliteration and surprisal. Pinned in
  `breaking_a_refrain_is_heard_where_the_refrain_returns`.

**Rejected / deferred.** Audio-level scoring (rendering through `SuperCollider` NRT and an aesthetics
model) — needs the structural problems above fixed first or it only re-measures them. The bands are
provisional; calibrating them against pairwise listener preferences is the next step for the
scorer itself.

### After M9 — examples, and checking the output actually plays

Added `examples/`: nine verbatim public-domain poems, a runner, and a `SuperCollider` receiver.
Then checked the claim "this produces sound", which no test in the repository had ever established
— every test inspected the output through the same crate that produced it.

**Strudel holds up.** The emitted text was evaluated in real `@strudel/core` and `@strudel/mini`,
not merely parsed as JavaScript. Every method the emitter calls exists on `Pattern`; querying
Dickinson gives eight events in cycle 0 and six in cycle 1, matching the syllable counts of those
two lines; successive cycles differ and repeated queries agree, so the lines play in order and
deterministically; and all five bank names resolve to real samples in Strudel's default set.

**OSC did not.** The backend computed exact onsets, printed them in `to_text`, and dropped them on
the way to the wire — 85 messages arriving as fast as UDP would carry them with nothing to say when
any of them belonged. Decoding the bytes with `python-osc`, a third-party implementation, is what
showed it; the whole test suite was blind because it only ever read the schedule back through the
type that built it. The onset is now the first argument of every note and the tempo is sent once
ahead of them, and `examples/prosody.scd` gives the messages somewhere to arrive.

The lesson generalises past this bug: **a serialiser tested only against its own deserialiser is
untested.** The same is true of the Strudel text, which is why that check used Strudel rather than
`node --check`.

### Iteration 15 — M9 OSC, the watcher, and the end of the build order

Built: `prosody-emit::osc` and the `--watch` loop. 523 tests. Every milestone is now done.

**The OSC backend is where `Backend::Output` being an associated type earns itself.** Two of
`SPEC.md` §4.3's rows have been carried through the IR since M5c waiting for a backend that could
express them, and mini-notation structurally cannot: a step's position in the cycle already fixes
when it sounds, so "downbeat-anchored" has nowhere to move it to. Here time is a number. An
anaphora's words are pulled onto the pulse; an epistrophe rings on where its neighbours are
clipped. `PLAN.md` recorded this as the question M9 would answer, and the answer is that the
anchor was real.

It also does what the text backend could not for §4.2. A cycle is a fixed span, so mapping a line
to a bar made the *foot* elastic — a two-foot line and a five-foot line took the same time. Onsets
here accumulate: the second bar starts exactly where the first ended, so an odd metre stays odd
and the pulse stays constant. That is the open question from iteration 12 answered by having a
second backend rather than by arguing about the first.

Onsets are exact rationals in beats, never seconds, and the tempo is a separate number. So I3
holds here by construction: change the palette and every onset is identical.

**Two dependencies not taken.** OSC 1.0's wire format for three argument types is about forty
lines and *total* — every value has exactly one encoding and there is no dialect to get wrong,
which is the opposite of the TOML case where a partial parser would silently mis-read what it did
not understand. And the watcher polls `mtime` rather than taking `notify`, which would bring
platform backends, a thread and a channel to watch one file; at a tenth of a second the poet
cannot tell.

The watcher is what I2 was written for — `SPEC.md` §6: "Per-unit seeding + Tree-sitter's
dirty-node set means an edit touches only the voices it should — this is what makes it
live-codeable rather than render-and-pray." A failed compile prints its reason and keeps watching,
because a half-written poem mid-keystroke is the normal case, not an error worth exiting on.

Ten mutations, all killed, including every one that would have broken the wire format.

### Iteration 14 — M8 the golden corpus, and I3's property test

Built: `crates/prosody-emit/tests/corpus/` — seven poems and fourteen committed snapshots — plus
proptests for I3, the only invariant that had none. 505 tests.

**What a snapshot is for here.** Not "the output is what it is", which is a test that can only be
re-blessed. It is for the failure the unit tests kept missing: a whole *column* going empty.
Iteration 12's review found the palette block, the header counts and the layer labels could each
be blanked with the suite green, and iteration 13 found the beat position could be a constant.
Each needed its own test afterwards; one snapshot of the whole text catches all four at once.

`CLAUDE.md` §7.2 names seven cases and the corpus covers all of them, with a test asserting each
poem is still the thing it was chosen for — a corpus entry that has quietly stopped being a
villanelle passes its snapshot while covering nothing.

**Reading the snapshots, which §7.2 insists on**, the villanelle gives six events at degree 0 and
three at degree 1 across four refrain lines. That is `SPEC.md` §4.5's "villanelle → a two-note
obsession (which is what a villanelle already is)", arrived at without anything in the code knowing
what a villanelle is. Free verse produces no pitch at all and anchors on `MostRegular`; the
nonsense poem marks eleven words guessed; and no poem anywhere produced an "attribution lost" row,
so every token in every corpus poem is accounted for.

**I3 now has properties, not just examples.** For an arbitrary poem and any two palettes drawn from
the whole style space, the rendered *arrangement* is identical and so is the score. Verified to
bite: a swing value that drops one event fails it in the first few cases.

### Iteration 13 — M7 the debug annotation view

Built: `prosody-core::annotate`, and `--explain` now renders it. 497 tests.

`SPEC.md` §8 calls this "the only real defence against I4 quietly failing", and the reason is that
nothing enforces I4 by construction: a `Span` proves an event *claims* a range of bytes, not that
the claim is right, and a wrong claim is invisible in a backend's output. So the module runs the
attribution backwards — every other pass goes text → structure; this one takes the finished score
and asks, for each token the poem actually contains, what became of it. A token with no events is
as interesting as one with too many.

Two things it does that a printf would not. It names **which pass decided each value**, which is
what turns "that sounds wrong" into "that pass is wrong" — and each pass cites its `SPEC.md`
section. And it distinguishes a token that is *silent* (nobody could pronounce it, which
`CLAUDE.md` §6 requires be visible) from one that is simply **absent from the score**, which is a
hole in the attribution and has no innocent explanation.

There is a test that fails when a mapping reaches the IR without reaching the view. Twice — at
M5c and again at M6 — a row of §4.3 was computed, carried, and silently dropped one layer later,
and both times what would have caught it is a reader seeing that the column was empty.

**The view found something within a minute of working**: every word was annotated `f0+0`, a "word
family" with one member. §4.3's row is about near-repetition, so a family of one has nothing to
modulate, and printing it buried the real families in noise. Suppressed in the view and raised in
the questions above, because whether the *IR* should carry it is a mapping question.

Mutation testing: eleven applied, all killed — but only after two rounds. Printing a constant
beat position satisfied every assertion in the module, because the beat column's whole value is
that its numbers differ and nothing said so.

### Iteration 12 — M6 the Strudel backend, the style layer, and the CLI

Built: `prosody-emit` (`Style`, the `Backend` trait, the Strudel text backend) and `prosody-cli`.
The pipeline is end to end — `prosody poem.txt` emits pattern text. 487 tests.

**I3 is now a dependency edge rather than a promise.** `Style` lives in `prosody-emit`, which
depends on `prosody-core`, so `voice` and `lower` *cannot* import it. Its property test is the last
of the four invariants to exist and needs both halves: that a style change leaves the score
byte-identical, and that the style does reach the output — a backend ignoring style entirely would
pass the first alone.

**And the emitted pattern did not play the poem.** Four reviewers, 71 mutations, 26 survivors, and
the most important findings were not in the Rust at all:

| Finding | Resolution |
|---|---|
| **`\|` in mini-notation means *choose at random*, not "next bar".** Every pattern was `<bar0 \| bar1 \| …>`, so a poem's lines played in a different order every cycle and the same text produced a different piece on every play. I1 held for the text and failed for the sound; I4 failed outright | `<[bar] [bar] …>`, each bar bracketed as one element — `<a b c>` takes one *element* per cycle, so an unbracketed multi-step bar would have been four bars, not one |
| **`.cpm` is cycles per minute and was given a BPM**, so every poem ran about five times its stated tempo and §5's window was decorative | Divided by the poem's mean beats per bar. `Score` now reports `beats_in`, which M5c's review had already asked for |
| **`SPEC.md` §5's own example header played silence.** `bank = "tr909"` asks Strudel for a sample called `tr909_bd`; the bank is `RolandTR909`. `synth = "karplus"` is not a Strudel synth at all — and `tr909` is the *default*, so this was the out-of-the-box output for every poem | The poem's vocabulary is validated against a known list, and the *backend* translates it to identifiers Strudel has. Which is what "the backend owns palette-to-sound" meant all along |
| **A style header could rewrite the arrangement.** `bank` and `synth` were free strings written straight into generated JavaScript; `bank = "a\").fast(4).s(\"bd"` appended `.fast(4)` to every voice — I3's stated failure, in the milestone whose headline claim is that I3 is enforced | Validated types. `scale` and `root` always were; the two fields needing it most were the two that lacked it |
| **Every melody note landed half a foot early**, in every line of every iambic poem, because the melody derived its own grid instead of reading `Event::subdivision` | One `grid` function for every layer. It also removed a duplicate of the same 35 lines that had to stay positionally in step or each gain attached to the wrong syllable |
| **`prosody poem.txt \| head -1` panicked.** Rust ignores `SIGPIPE` | Handled |
| §4.3's `family`, `bank` and `anchor` were carried into the IR at M5c *because review caught them being dropped*, and were dropped again at emit. A doc table claimed the alliteration row shipped; `.vowel` appeared nowhere in the crate | Family drives bit-crush depth, alliteration drives vowel colour, and the anchor's deliberate absence is now argued rather than asserted |

**What the tests were not doing.** Five mutations let the *style* delete events, reorder the
melody, silence bars and double the tempo — all passing a suite that compared two `Score`s, which
`prosody-core` builds without ever seeing a `Style`, so the comparison could not fail by
construction. The half of I3 at risk is the emitter, and nothing was looking at it. §4.4's
"direction" assertions took a max and a min over a bag and asserted they differed, which is true of
any non-constant mapping — inverting both formulas survived. `Style::default()` was tested by
`assert_eq!(default(), default())`, so every field of the palette most poems are heard through
could be changed unnoticed.

The CLI had no tests at all, and it is the artefact a user touches. It has twelve now, including
one that runs `node --check` over the emitted text — because a bracket count had already let two
syntax errors through, and an earlier bug put the separating comma *inside* a `//` comment, which
made the whole program unparseable while looking fine.

### Iteration 11 — M5c the Voice IR, and §8's acceptance test

Built: `prosody-core::voice` (the IR) and `prosody-core::lower` (rhythm, melody, colour, figures),
plus `prosody-lts/tests/acceptance.rs` — `SPEC.md` §8's test, which turns out to need no backend
at all, because an event list *is* what would be heard. 437 tests.

**How six passes become one voice**, the question M5b deferred: they were never in competition.
Role decides the layer, repetition decides which lines are the same instrument, meter decides when
and how hard, rhyme decides pitch, surprisal decides brightness, families decide modulation depth.
The collision recorded since M4b — a word that is both a family member and a rhyme — dissolves the
same way: §4.3's word voice is *timbral* and §4.5's class is *pitch*.

**Three of §4.3's five rows never reached the IR.** Anaphora ("downbeat-anchored ostinato"),
epistrophe ("cadence figure") and alliteration/assonance ("same sample bank") were all computed by
the analysis passes and silently dropped by the lowering — in the section the spec calls its
highest-value pass, while both my module doc and my plan presented §4.3 as handled. They are not
recoverable downstream: `Score` is the whole contract with M6, and "downbeat-anchored" is a claim
about *where in the bar an event sits*. Now carried as `Anchor` and `bank`.

| Finding | Resolution |
|---|---|
| **The §8 acceptance test — the project's reason to exist — graded a guess.** "smoulder" is not in `CMUdict`, so the pipeline guessed it by letter-to-sound at 68% stress accuracy; my doc comment stated `S M OW L D ER`, the phonemes of the *American* spelling, while the code produced `S M AW L D ER`. Then "smolder" fixed the dictionary problem but sits in the rarity band reserved for *absence*, so the §4.4 assertion reduced to "a word we have never seen outranks one we have" — true of any nonsense string | "flicker": in the dictionary, and band 6 against burn's 5. A real gradient, and `both_words_are_dictionary_facts` pins it |
| **`roles` was the one input not fingerprint-checked**, so §4.1's load-bearing decision could come from another poem: the arrangement inverted and `run` returned `Ok`. My own acceptance criterion said all six were checked | `PoemRoles` carries a fingerprint |
| **`Domain::Poem` made a global RNG constructible.** `Seed::for_unit(Domain::Poem, whole_poem_span, source)` type-checked, which is the one thing I2 forbids, guarded only by a doc comment saying "never seed from this" | `Fingerprint` is its own type with no `rng()`. Identity and entropy are different things |
| **`Detune` derived `Ord` on a fraction**, so `1/2 < 1/4` — the worse slant rhyme sorting as the closer one, reproduced on real verse. Third occurrence: `Variance` documents fixing it, `RhymeScore` hand-writes it, and I wrote a doc comment about keeping floats out of branches while regressing to a derive | Hand-written, cross-multiplied in `u128`. `Velocity` lost its per-value denominator; `Subdivision` lost an order that was never meaningful |
| **`bars()` was derived from the last event**, so a poem ending in an unpronounceable line came back a bar short | Counted while lowering. `Score::silent` now reports the words that make no sound |
| **Provenance was dropped by lowering**, so an event resting entirely on a guess was byte-identical to one resting on the dictionary — and the §8 debug view can only render what the IR kept | Carried on every event |
| **Scale degrees were consumed by rhyme classes that never sound**, so a poem's first *audible* note could be degree 5 | Numbered over the classes that actually sound |
| **A voice was created for lines that produce no events** — an instrument that never plays, which the new I4 property asserts is impossible and could not reach | Voices are built from the events that exist |

**Five mutations survived my own first pass**, and four more survived after that. The instructive
one: seeding a voice from the *stanza* span, or from the whole poem, passed everything — because
`Voice::seed()` had no callers at all. `CLAUDE.md` §5 says `Seed::for_unit` takes a span precisely
so a caller cannot pass the whole poem where a unit belongs; the type stops a bare `&str` and
leaves a poem-wide *span* perfectly typeable, and this pass is its first real caller. I2 rests on
that one argument, and nothing was watching it.

Also caught: no fixture ever produced a secondary stress, so the middle rung of the velocity
ladder could be set to zero unnoticed; and one of my test rewrites silently failed to apply
because `cargo fmt` had reflowed the block my pattern matched, so a test I believed I had
strengthened was the old one. I only found it by re-running the mutation.

### Iteration 10 — M5b role assignment

Built: `prosody-core::role` (§4.1's load-bearing decision), a measured corpus in
`prosody-lts/tests/roles.rs`, and eight invariant properties. 400 tests.

**The decisions M5b owed.** Relative variance, not raw — §4.1 compares stanzas against each other
and raw spread grows with line length. Not `dominant_foot`, which is 50–57% accurate on the
commonest English metre. An absolute threshold with a *named* fallback, so locality holds wherever
it can and the output admits where it cannot. And a line grain for refrains, because a stanza's
variance is a fact about how its lines differ and so cannot see the one line that repeats.

**The finding that outranks all of them: the measure cannot see what §4.1 says it sees.** Variance
is permutation-invariant, so `[8,6,8,6]`, `[8,6,6,8]` and `[6,8,8,6]` are one stanza to it. My own
doc comment had claimed common measure scores well *because* it alternates; it scores well for the
same reason its shuffle does. A limerick and a Sapphic stanza — two of the most rigidly fixed forms
in English — measure raggeder than Whitman, so no threshold admits them. Now pinned as
`known_limitations` and raised in the questions above rather than absorbed into the constant.

| Finding | Resolution |
|---|---|
| **A stanza with nothing to measure became the foundation.** `Variance::relative` returns zero for an empty sum, so `...`/`!`, or `1914`/`1918`, or a Slovenian couplet under a dictionary-only phonemiser, all read as *perfectly regular* — and, being foundation, denied the free-verse fallback to the whole poem. `LineMeter::unpronounced` records exactly this and the pass never read it | `StanzaBasis::Unmeasured`. The pass now distinguishes "no spread" from "no evidence" |
| **`assign` accepted a meter and a repetition from different poems and invented refrains.** The doc claimed spans prevented it; they cannot — "war is kind maiden" and "one two three four" occupy the same bytes, and my first fix compared spans and passed the reviewer's own counterexample | Analyses carry a `Poem::fingerprint`. Equal fingerprints mean equal sources, which made the span and count checks unreachable, so `RoleError` collapsed to one variant |
| **The refrain lift is a second poem-wide dependence, and the module denied it twice** | Documented as what it is — §4.3's figure, which cannot be made local without ceasing to detect repetition — and the locality property now covers the line grain that actually fails |
| **A poem of one-line stanzas got no foundation at all**, contradicting §9's "always anchors", and the property `prop_assume!`d exactly that case away | `StanzaBasis::OnlyCandidate`: promoted for want of an alternative, and says so |
| `Variance::threshold` was `pub`, letting anyone fabricate a measurement — and its `assert!` is a *runtime* panic that `clippy::panic` does not catch, in a comparison that overflows `u128` and inverts in release | `pub(crate)`, which closes all three at once |
| Two public `foot_variance()` methods returned different quantities under one name — raw on `StanzaMeter`, relative on `StanzaRole` | Renamed, and the role now *reads* both from `StanzaMeter` instead of recomputing them |
| One `Basis` enum spanned two grains, so a `StanzaRole` could hold `Refrain` — visible as a `prop_assert!(false)` arm in the new property | Split into `StanzaBasis` and `LineBasis`, and `Refrain` now carries the `VoiceId`, so a looping line can say *what* it loops with |
| Two mislabelled fixtures. Housman's "Loveliest of trees" is iambic tetrameter throughout, not alternating — and I had leaned on the corpus containing awkward alternating forms to justify the lower bound, leaving only three genuine ones | Relabelled; Coleridge marked as the expanded variant it is |

**Five mutations survived the first test suite**, each caught by a reviewer running them rather
than reasoning about them: dropping the foot half of the rule (the discriminating case occurs in
0.11% of generated stanzas, so the property caught it 9 runs in 12 — a coin toss, not a test); a
line role carrying its *stanza's* span (containment is reflexive, so an I4 property that checks
nesting can never see it); a promoted stanza whose lines all stay flourish; the promotion ranking
and tie-break, both documented and neither pinned; and the two variances swapped, which every
symmetric assertion cancels out. All ten now die, including the swap.

Also fixed, found by a property run and unrelated to M5b: `LetterToSound` failed on any word whose
spelling is a single consonant grapheme — "dge" spells `JH`, consuming the word's only vowel — so
the *fallback* phonemiser went silent on exactly the odd input it exists to catch. A written vowel
now survives as a spoken one, while "brrr" still honestly has no syllable.

### Iteration 9 — M5a surprisal

Built: `prosody-core::surprisal` (§4.4's metric) and `crates/prosody-lexicon` (the rarity table).
364 tests.

**A process failure first.** The build was reported as "319 tests passed, all gates green" while a
test was failing. The gate command summed the passed-count from each `test result:` line, and a
failing line reads `FAILED. 9 passed; 1 failed` — so the sum counted the 9 and never saw the 1.
`CLAUDE.md` §3 now says to check the exit code. The failing test was real: the word-family
allomorphy rule from iteration 8 correctly refused `krapplesnaffle`/`krapplesnaffles`, because the
letter-to-sound module emitted a voiceless plural after a voiced sound — English would never build
that. The new rule *exposed* an old bug rather than causing one.

| Finding | Resolution |
|---|---|
| **The metric picked function words as sharpest in 4 of 6 real poems** — "shorter" over "civility" in Dickinson, "Maybe" over "explode" in Hughes, "a" over "burn". Dividing novelty by the word's own length let a one-phoneme word reach the top of the scale on a single unheard vowel | Novelty counts *how many* sounds a word brings, up to four, instead of what fraction of itself is new. Dickinson now gives "gossamer", Hughes "explode" |
| **The corpus was four fifths King James.** "thou" and "unto" ranked alongside "not", while `coffee`, `hemlock`, `dryad` and `gossamer` — one of them from Keats, a source text — all collapsed into the same bucket as invented gibberish | Fourteen sources, nine verse and five prose, each contributing its *relative* frequencies with equal weight. 20,000 words kept rather than 6,000 |
| **Long lines saturated the second term.** On a 27-word Whitman line, 17 words scored no novelty at all — with a wide enough window every phoneme is covered by something | The window is a bounded neighbourhood within the line, which is what §4.4's "local window" means |
| `sharpest()`'s tie-break was correct only by a non-obvious cancellation in `max_by`, and flipping it to the naive direction passed all 59 relevant tests | Tested |
| The "window is the line" invariant was unverified — a poem-wide context passed everything, because the only test that could catch it used a single-line poem | Tested, with a multi-line fixture |
| A rarity-blind metric passed every real-vocabulary test, because the chosen words were also the most phonetically novel in their lines | A fixture where only rarity separates the words |

Verified rather than assumed: `combine()` reduces exactly to `(α·r/bands + β·n)/(α+β)`, `Eq` agrees
with `Ord` — the third time that contract has come up and the first without being told — and
`Surprisal` cannot overflow, because its denominator is bounded by the 39-phoneme inventory rather
than by poem length.

Surprisal is also the one M5 subsystem with no poem-order dependence: rarity is a static table and
novelty reaches only a few words either side, so editing one stanza cannot move another's score.

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
