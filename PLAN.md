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

**The harmony half is now a candidate** (`Mapping::Harmony`: `rhyme` = H1, `cadence` = H2, `both`;
off by default — see the log, *sound and voice, step 5*). **So is the walking bass**
(`Mapping::Bass`: `height`, the default and the bass as it was, `syllables` = W1, `feet` = W2 — see
the log, *sound and voice, step 6*). Sound and voice step 3 built the jazz *palette*
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

*Since then (the off-rhythm trace, in the log):* the 0.017 is a measuring artefact as much as a
mapping one — `syncopation` merges every line's accents, and with three or four lines stacked some
line accents 346 of The Tyger's 350 sounding beats, so the 254 accented instants off the beat never
count. What the user heard as off-rhythm is mostly cross-rhythm and the Strudel elastic beat; the
jazz profile now measures the first (`cross_rhythm`).

---

## Log

### Staging deployment: staging.prosody.rocks

The user asked to deploy a staging version of the app to `staging.prosody.rocks` from a new branch
`staging`.

- **Wrangler environment (`wrangler.jsonc`).** Added `env.staging` with `name: "prosody-staging"`,
  custom domain route `staging.prosody.rocks`, and `vars.CANONICAL_HOST = "staging.prosody.rocks"`.
- **Worker & redirects (`deploy/worker.js`, `deploy/redirect.js`).** `redirectFor` accepts the
  canonical host (defaulting to `prosody.rocks` in production and `staging.prosody.rocks` on
  staging) so HTTP and `www.` requests on staging redirect cleanly to `https://staging.prosody.rocks/`
  with HSTS, while preserving link preview generation for shared poems. Verified by
  `deploy/check-worker.mjs`.
- **Staging deployment automation (`.github/workflows/deploy-staging-comment.yml`).**
  Staging deploys are triggered on-demand from pull requests rather than from a long-lived branch:
  `deploy-staging-comment.yml` listens for PR comments matching `staging`, `deploy`, or `🚀`.
  It acknowledges with a rocket emoji, checks out the PR branch, verifies Wasm parity and worker
  redirects, deploys to `staging.prosody.rocks` via `wrangler deploy --env staging`, and comments back
  on the PR with the deployment link. `scripts/deploy.sh` also supports `staging` for manual deploys
  from a local working copy. Keeping `ci.yml` scoped to `master` and `main` avoids duplicate check
  runs on PR branches.

### Drum kit articulation: a syllable's stress and beat position choose its piece of the kit

The user asked to start with drum kit articulation, choosing **by beat position + stress** (`bd`/`rd`
on even-beat primary stresses, `sd` on odd backbeat primary stresses, `hh` on unstressed off-beat
subdivisions, and toms/snare/hat on flourish lines) as a **candidate `Mapping` selector**
(`Mapping::Drums`, named `"drums"`, with `flat` as the default and `articulated` to compare against).

- **In the core (`prosody_core::drums`, `Mapping::Drums`).** A new `Drum` enum (`Pulse`,
  `Backbeat`, `Subdivision`, `Ornament`, `HighTom`, `MidTom`, `LowTom`) sits on every `Event`
  (`Event::drum`). Under `Variant::Flat` (the default, so every committed snapshot and existing
  behaviour is untouched), foundation syllables take `Drum::Pulse` and flourish syllables take
  `Drum::Ornament`. Under `Variant::Articulated`, `drums::run` runs right after `accent::run` (so it
  respects `function-words = light`):
  - Foundation (`Layer::Foundation`): `Velocity::Unstressed` → `Drum::Subdivision` (`hh`);
    `Velocity::Primary` or `Velocity::Secondary` → `Drum::Pulse` on even beats (`beat % 2 == 0`) and
    `Drum::Backbeat` (`sd`) on odd backbeats (`beat % 2 == 1`). When a foot has two or more stressed
    syllables (a full-force spondee), its first stressed syllable takes the beat's primary piece and
    subsequent stressed syllables in the same beat alternate to the counterpart (`Pulse` ↔
    `Backbeat`), splitting spondees across kick/ride and snare.
  - Flourish (`Layer::Flourish`): `Velocity::Unstressed` → `Drum::Subdivision` (`hh`);
    `Velocity::Primary` → `Drum::HighTom` (`ht`) on even beats and `Drum::Backbeat` (`sd`) on odd
    beats (with second spondee stresses stepping to `MidTom` on even beats or `LowTom` on odd
    beats); `Velocity::Secondary` → `Drum::MidTom` (`mt`) on even beats and `Drum::LowTom` (`lt`) on
    odd beats.
- **In the backends and views.**
  - Strudel (`prosody-emit/src/strudel.rs`): `rhythm_layer` maps each event's `Drum` through the
    palette's `Kit` (`Pulse` → `bd` under `Kit::Machine`, `rd` or its `RideFallback` `oh`/`cr`
    under `Kit::Ride`; `Backbeat`/`Ornament` → `sd`; `Subdivision` → `hh`; `HighTom`/`MidTom`/`LowTom`
    → `ht`/`mt`/`lt`), and `drums_header` discloses `drums = articulated` with the count of
    syllables off the flat piece (`Score::articulated`).
    `every_kit_asks_every_bank_for_a_sample_the_bank_has` now verifies every `Drum` variant across
    all 6 allowed Strudel drum banks (`tr909`, `tr808`, `tr707`, `linn`, `casio`, `r8`) and both
    kits (12 configurations).
  - OSC (`prosody-emit/src/osc.rs`): wire messages carry the layer (`"foundation"` or `"flourish"`),
    not a drum sample name, so `Schedule::drums_unapplied` and `# not applied here: drums =
    articulated (…)` disclose it in the schedule header and on `--send`.
  - Explain view (`prosody-core/src/annotate.rs`), CLI (`prosody-cli`), web (`prosody-web`), and
    scorer sensitivity (`prosody-score/src/sensitivity.rs`): `Annotated` summarises `drums
    articulated` and tags each syllable's `from` column with `drums(…)`; `Heard` includes `drum` so
    the one-word edit test hears drum changes; `scripts/check-wasm.mjs` checks
    `set=drums:articulated&profile=jazz` for native/Wasm parity.
- **Invariants and measurements.** I1 (determinism), I2 (`i2_an_edit_leaves_other_stanzas_drums_alone`),
  I3 (`i3_no_palette_can_change_the_articulated_drums`), and `articulated_changes_only_drum_pieces`
  all hold; `./scripts/cycle.sh machine drums=articulated` preserves 100% of legibility across all
  18 poems (mean overall 0.704).

### Sharing without storage: a creation is a link

The user wants people to share what they make, with no storage — previews of a shared link allowed,
so long as nothing is kept.

- **A link is the creation.** The compiler is deterministic (I1), so a poem and its settings are a
  whole piece. **Share** writes the poem, the controls that differ from the default palette, each
  side's variants that differ from where it starts, the side heard and the language as JSON,
  deflated raw and base64url, into `?s=` (`web/src/share.ts`): Tyger about 700 characters, the
  longest example ("Patterns") about 2,300, which the dialog says to send by message or email. A
  query rather than a fragment, so the worker can read it for the preview. The format is
  versioned (`v: 1`); a link is read defensively — junk, a later format or one unpacking past
  64 KB is no link, and unknown controls, mappings and variants fall back, as a stale save does.
  Opening one sets the page from it and says it is a shared poem, without saving over the
  visitor's own settings.
- **Previews, keeping nothing.** The worker, for `/?s=…`, decodes the link and puts the poem's first
  line in the page's title and the next lines in its description and Open Graph tags
  (`deploy/preview.js`), escaped; a static card (`og.png`, rendered from HTML in the page's own
  look) is every link's image. `deploy/check-worker.mjs` checks the preview, the escaping, a link
  made to unpack too far, and the worker's rewrite; a page test checks that the worker reads a link
  the page made.
- **The prerequisite.** A link puts a stranger's poem through code the page evaluates, so a new test
  (`no_poem_text_reaches_the_code_the_page_runs`) compiles a poem written to break out — quotes,
  backticks, `*/`, `</script>`, U+2028 and U+2029, carriage returns — and checks that no line of the
  pattern, the two reading cue patterns or the trace carries its words outside a `//` comment, that
  no JavaScript line terminator appears at all, and that a hostile control value is refused. In
  fact the compiled code carries no word of a poem at all; the words live only in the explain
  view, which the page shows as text.
- Tests: 77 in Vitest (the format, its round trip and defences, the dialog, opening a shared poem)
  and 8 end to end — one shares Jabberwocky under techno heard on B and opens the link in a second
  browser, which hears the same piece.

### A merge to main deploys itself

The user asked for deployment on merge. CI's new `deploy` job runs on a push to `main` once
`gates`, `wasm`, `web` and `grammar` have all passed, and publishes the site the `wasm` job built —
the one check-wasm found byte-identical to the native compiler and the end-to-end tests played — as
an artifact, so what goes live is exactly what was tested (built from an empty `target/site`, since
the Rust cache restores `target/`). It checks the worker's redirects (`deploy/check-worker.mjs`),
then runs `wrangler deploy` through `cloudflare/wrangler-action`, wrangler pinned to 4.152.0 in CI
and in `mise.toml` alike. One deploy at a time, never cancelled half way, in a `production`
environment. It needs the repository secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`;
`scripts/deploy.sh` stays for a deploy by hand.

### Mute, solo, live meters and the lane roll

The fourth and last of the agreed steps.

- **Mute and solo**, on every Mixer channel (drums, bass, melody, chords, voice). The page plays
  the evaluated pattern through `withValue`, which Strudel applies as each event is queried, so a
  switch is heard on the next tick with nothing recompiled; an event of a channel not heard plays at
  gain 0 and is marked, and stays on the roll, faint. The compiled pattern is untouched — what is
  shown and what pastes into strudel.cc is the poem's. The voice is muted at its bus (and the
  browser's voice cancelled). With any channel soloed only the soloed are heard, as on a desk.
  Not remembered: a visit starts hearing everything. Gain is the only level the emitter writes on
  an event, so gain 0 is silence.
- **Which layer an event is** comes from the colour the emitter already writes (`.color`, display
  only to Strudel's sound); `strudel::layers()` now names each layer with its colour and
  `site.json` carries it, so the page cannot drift from the emitter. A test checks that every
  `.color` in a compiled poem is one of them.
- **The lane roll** replaces `@strudel/draw`'s piano roll (its `Drawer` still supplies the frames):
  a lane a layer with its name in a gutter, each event a bar across its time, a higher note higher
  in a pitched lane, lit while it sounds, faint when muted, the bar lines and the playhead as
  before (`roll.ts`, its geometry a pure function under test).
- **Live meters**: each channel's meter rises to its fader while its layer sounds and falls after
  (an envelope per channel from the same frames), the voice's while a line is heard; back to the
  fader alone when stopped.
- Tests: 68 in Vitest (mute and solo, the silenced and marked event, the voice's level, the lane
  geometry worked by hand, bar lines round the loop, the strips dimming) and 7 end to end — the
  new one plays the Tyger, solos the bass, and watches the drums' meter fall and the bass's move.

### Styles: six genres, and what they suggest the poem do

The third of the four steps. The user approved the reading of I3 proposed with it: a style is
palette, and what its genre would have the poem *do* is only ever suggested, on the page, on side A.

- **Six genre presets** in `prosody-emit` after `jazz`, each with its instruments, its mix and a
  feel (scale, swing, drum machine): `lofi`, `techno`, `ambient`, `bossa`, `chamber`, `hymn`. Ten
  General MIDI instruments they need joined the soundfont list, each checked against
  `@strudel/soundfonts` 1.3.0. A genre quietens the drums at most to `0.10`, never off — the pulse
  is structure. Their levels are estimates from the default's measurements, like jazz's, not
  measured; measuring them is a follow-up worth doing by ear.
- **What a style suggests** lives in `prosody-web` (`STYLES`), beside a line saying what it
  sounds like, and goes to the page in `site.json`; a test checks that every palette has an entry,
  in order, and that every suggestion names a mapping and a variant the core has. Jazz suggests the
  walking bass and cadences (the "structural half of jazz" candidates), lo-fi a chord a rhyme and
  light function words, techno the bass on every foot and the straight stack, ambient the elastic
  pulse and a chord a rhyme, bossa the bass on the feet and cadences, chamber a walking continuo and
  cadences, hymn cadences, the bass on the feet and each stanza read through.
- **On the page** the Sound folder opens with a card per style, the one the sound matches lit (none
  once a control is turned: custom). Picking one applies its palette and sets its suggestions on
  A; the folder says what it set, Mappings tags each row "set on A by …" and offers "Put A back as
  it was", which restores what A had before any style. Setting a tagged row by hand makes it the
  writer's own.
- Tests: the genres' instruments and feel, hand-written from their descriptions (scale by name and
  by steps, so a misspelt scale cannot fall back to minor unnoticed); every preset reads back
  through its header; the styles table against the core's mappings; on the page, suggesting,
  keeping the first "from" through a second style, putting back, a hand choice taking a row over,
  and the cards, tags and button rendered.

### The desk, rearranged: a mixer, the roll on a switch, patch displays

The second of the four steps: the redesign the user chose from the design canvas's second row
(boards A–D), after a review of the live page. Nothing the compiler does changed, and no control's
id, value or meaning.

- **Every level in one place.** A Mixer folder holds the four band faders and the voice's own, set
  apart as the reader's channel, with the duck switch and the effects (reverb, room, echo, high
  cut, drive, compress, density). The voice level and the duck left Reading; swing and humanise sit
  together on Sound as the feel.
- **The roll is a switch, not a folder.** Shown or hidden from the top bar, remembered, whichever
  folder is open; it sits under the poem (the bars-shown switch and its key above it). A reader
  who had left the old Roll folder open finds the roll shown and the sheet folded.
- **The layout.** On a wide screen (≥ 1100 px) the folders are a panel beside the poem, the whole
  height, always showing one; on a phone a sheet along the bottom edge, folded to its tabs until
  one is opened, folded again by the open tab or a handle, at most two fifths of the screen, the
  roll a slim strip — so the poem keeps a few lines in view. A new folder starts at its top.
- **Patch displays and switches instead of menus.** A long list (scale, palette, drum machine,
  bass, melody, chords, language, voice, the poem picker) is a value on a small dark screen with ◀
  and ▶, and a real menu lies over the screen, so a click, the keyboard or a phone's picker still
  chooses from the whole list. A short one (kit, ride stand-in, rhyme voicing, pace, who reads,
  the views, the score's profile, bars shown) is a segmented switch. Values have names a reader
  knows: "Acoustic bass", "Roland TR-909", "phrygian dominant" (`names.ts`); the page still sends
  `gm_acoustic_bass`.
- **Mappings** are a row each, A over B as switches, the default underlined; a row whose A and B
  differ glows and says so, the head counts them, and "?" opens what the mapping does.
- **Calmer labels.** Plain sentence-case words for controls and folder tabs; the engraved capitals
  kept for the hardware's own markings. Long explanations fold behind "How the reading works" and
  "About the mappings".
- Tests: 55 in Vitest (the folders and the handle, the roll switch and the old Roll folder, the
  patch display's names and steps, the poem picker, the voice fader, the mappings' glow, count,
  default and "?") and 6 end to end (the panel beside the poem, the roll with any folder, a phone
  with no sideways scroll and the poem still in view). Checked by eye at 1440 and 390 px, dark and
  light.

### The page as its own program: Preact, Signals, TypeScript, Vite

The first of four agreed steps (then the desk redesign from the canvas, styles and genres, and the
mixer's mute/solo with a lane roll): the page, which had grown into one 3,100-line `index.html`
with its data substituted in by `prosody-web` and its tests string-matching its source, is now its
own program in `web/`, looking and behaving exactly as before. Nothing the compiler does changed.

- **What replaces what.** Preact 10 with `@preact/signals` for the components and the state the
  screen shows; Vite to build; TypeScript in strict mode; Vitest with Testing Library and
  happy-dom for unit and component tests; Playwright for end-to-end tests of the built site. All
  pinned exactly, none newer than two weeks. They replace hand-written DOM code and the Rust tests
  that asserted on the page's text (`html.contains("…")`), which caught a renamed variable and
  missed a broken behaviour.
- **The boundary.** `prosody-web`'s library no longer renders HTML: it answers `/site.json`, the
  page's data (option lists, scales and their steps, mappings, presets, palettes, the seed, the
  transport), tested in Rust as data. The server serves the built page from `web/dist`
  (`--page DIR`), the static build writes `site.json` beside it, and `speech-worker.mjs` and
  `strudel-globals.mjs` moved to `web/public`. `npm run dev` proxies to a running `prosody-web`.
- **The port is close, not a rewrite.** The engine code — compiling and measuring, Strudel and the
  master stage, the reading's engines, render queue and both paces, the roll and the lit verse —
  moved function by function into modules, comments and all, typed. Only what the screen shows is
  signals; the lit verse is still drawn by hand, being relit every frame.
- **Tests.** 46 in Vitest: the A/B starting settings and every saved shape; palette matching; who
  reads a line (recording, live stand-in, browser) and why; the bars on the audio clock and the
  refit of an overlong line; the soft clipper's ceiling; the sounds a pattern warms; and the page
  rendered whole against a hand-made `site.json` — the welcome, folders, the key circle, knobs,
  mappings, the download question, compiling, A/B and the score table, a Slovenian preset. Four in
  Playwright against the built site with the real WebAssembly compiler: the seed compiles to the
  native summary, Zdravljica is read in Slovenian from its recordings, settings survive a reload,
  and it plays, lights and stops. CI gains a `web` job and runs the end-to-end tests after
  check-wasm. Node 24 is pinned in `mise.toml`.

### The folders as a studio desk

The user chose the "studio desk" direction from three proposals (a canvas of studio desk, score
paper and signal). The folders are now hardware on a desk that slides up from the bottom edge, in
place of paper; the welcome and the dialogs stay paper, notes on the desk. Nothing the compiler does
changed, and no control's id, value or meaning: every knob and fader is still the range input it
was, so dragging, the keyboard and screen readers work as before.

- **Sound** is five modules on a grid that reflows on a phone: the **key** (the twelve notes on a
  circle — a note sets the root, the root lit, the scale's other notes tinted from `SCALE_STEPS`,
  which `Scale::steps` now sends to the page), the **tempo** window as an amber readout with the
  swing knob, the **instruments** as selectors in dark wells with the two octave knobs, the
  **mixer**'s four faders with meters (drums, bass, melody, chords, each in its layer colour), and
  the **mix** as a row of knobs with lit arcs.
- **Reading**: the voice level is a knob and the duck a flip switch; **Mappings** and **Under the
  hood** sit on the desk with the same wells.
- `showValues` sets `--v`, each knob's and fader's 0–1 position, from which CSS draws the arc, the
  pointer and the meter; the note circle follows the root and scale from the same place.
- Dark anodised in dark mode, light aluminium in light; the tempo readout stays a dark display in
  both. Checked in Chromium at 1440 and 390 px, dark and light: no sideways scroll, the knobs and
  faders move with their values, the circle lights D phrygian dominant's notes for The Tyger.

### The page for everyone: folders, a welcome, light and dark, downloads asked about

Before publishing at prosody.rocks, the page was reworked to work on a phone as on a desk, to say
plainly that it is an experiment, and to be more playful. Nothing the compiler does changed.

- **Folders along the bottom edge** replace the right-hand column of panels: Roll, Sound, Mappings,
  Reading, Under the hood, each a paper folder whose tab sticks up in one of the five layer colours.
  One open at a time; it slides up over the lower part of the page; remembered. The poem has the
  page. Checked at 1280–1440 and 390 px: no sideways scroll (the page grid's column is pinned to
  the viewport — it had sized itself to the row of tabs), the poem keeps ~180 px with a folder open
  on a phone.
- **What it is:** "prosody" set in the lit poem's colours, syllable by syllable, with an "an
  experiment" tag; a dismissible line under the bar; and a **welcome** dialog on a first visit — the
  dictionary entry for *prosody*, what the experiment does, the source on GitHub and
  me@prosody.rocks — reopened by "about".
- **Light and dark**: the system's choice by default, or the reader's (◐ cycles auto, light, dark),
  applied in the head so it never flashes. Every colour is a token; in light mode the lit syllables
  are the layer colours mixed toward black (`--lit-strength`), since the tints are made for a dark
  ground; the roll stays a dark screen in both.
- **No download unasked:** the default reading engine is now the recorded readings (no model). The
  first time Kokoro, Piper or Kitten is chosen, a dialog says the size (Kokoro's by the device it
  will use: about 90 MB compact, 330 MB on WebGPU) and asks; accepting opens a loader with the
  engine's own progress, Hide and Cancel — Cancel terminates the worker (a new `cancel()` on the
  worker engines), keeps the voice there was, and says so. The loader stays until the voice has
  rendered the poem's first line, so that line is ready before Play (Piper fetches each voice with
  its first line). The instruments get the same loader on the first Play.
- **The Play button beats** with the piece under the steady pulse (the trace's cycles a minute are
  its beats a minute); still under elastic and with reduced motion.
- The "stanza 1 never read" seen on the deploy branch's engines commit does not reproduce on this
  branch (the render queue was reworked by the recordings step): on Naidu with Piper cold, line 1
  read 7 s late (its voice's download), the rest in time.
- Zdravljica's recordings re-rendered for the full poem (52 lines, Piper Slovenian); 1,025
  recordings, 9.3 MB.

### Recorded readings — review: a recording fetched at the natural pace is fitted at the fitted pace

A second review of the uncommitted recordings. `SPEC.md` §4.7 ("Engines", `recorded`; the fitted
pace's refit).

- **The bug.** A recording plays at its own speed (the line's pace) under both paces, so its render
  key, `recorded|voice|pace|text`, is the same at natural and at fitted. A line fetched at the
  natural pace was cached with no bar `room`. When the fitted pace asked for it, `whenRendered`
  returned the cached buffer and never called `afterRender` with the cue's room, so no refit was
  made. Dunbar's line 10 (3.140 s of speech for a 2.267 s bar) ran 0.47 s past its bar and lost
  "our cries" at the line 11 cue. Live engines did not hit this, because their fitted speed is
  the cue's rate, which gives a different key.
- **The fix.** When `whenRendered` is asked with a room, it calls `fitWhenRendered` before it
  returns. For a buffer already cached, that runs `afterRender` at once. For a fetch still in
  flight, it runs `afterRender` when the fetch resolves. For a live job still queued without a
  room, it gives the job the room. A key that has already been refitted is skipped.
- **Checked in a browser,** on the served page and on the static site. With recorded George at
  natural, Dunbar's 23 recordings were fetched first; then the pace was switched to fitted and
  the poem played. The page logged "rendering again" for lines 7, 10 and 11 (refitted to 1.135,
  1.274 and 1.071), played the refitted buffers, and said "past its bar" for no line. The earlier
  checks still pass: the edited line falls back, switching voices works, and there are no
  console errors.

### Recorded readings — review: a file is named for its pace, the manifest at startup

A review of the uncommitted recordings. `SPEC.md` §4.7 ("Engines", `recorded`).

- **A changed pace rule no longer relabels old audio.** A file was named by voice and text only, and
  only missing files were rendered, so after a pace rule changed (no text changed) `node render.mjs`
  — as the README said to run — rendered nothing and rewrote the manifest with the new paces: the
  test, `--check` and the page were all satisfied, and the page played old-pace audio as if at the
  new pace (rate = speed / pace = 1), with no resampling and no warning. Now a file is
  `<key>-<pace>.mp3` (`lib.mjs` `fileOf`, pace to the manifest's three places): a new pace wants new
  files, the old are left over (removed; `--check` fails on both), and the Rust test asserts each
  listed file's name carries its line's pace. The 976 committed files were renamed from the
  manifest's paces (which the test had already checked against the compiler); `node render.mjs` then
  had 0 to render and `--check` passes. Checked by moving one file to another pace: `--check` names
  it missing and left over; a manifest naming a file at another pace fails the Rust test
  ("kokoro-af_heart/027269a22bdb16b2-0.950.mp3 is not at pace 0.990").
- **The Rust test's total is gone.** `323 * voices + 7` was the tool's own count, not worked by hand;
  in its place, Crane's "In the desert" is counted by hand (ten lines, each spoken).
- **A returning reader of the recordings has the manifest at once.** With `recorded` saved, the
  manifest (121 kB, no model) is fetched at startup rather than at the first Play, so the saved
  voice is listed and line 1 of the first play is read from its file, not by the browser voice.
  Before it loads, the menu lists the saved voice (or the default) by id.
- **A side that reads nothing says so** in the recorded voice menu ("this side reads nothing")
  instead of "(0 of 0 lines recorded)".

### Recorded readings — the examples spoken ahead of time, a fifth engine

The user decided: make pre-recorded versions of the readings on this machine and ship them with the
site (to be hosted on Cloudflare), above all for browsers where the in-browser engines do not work,
with a set of alternative voices. `SPEC.md` §4.7 ("Engines", `recorded`).

- **The tool, `scripts/readings/`** — a Node package (one dependency, playwright-core 1.64.0,
  Apache-2.0, published 2026-10-07, zero dependencies of its own; lockfile committed; kept out of the
  Rust workspace). Node rather than Python because the point is that a recording is the *same voice*
  as the live engine: it runs the page's own `speech-worker.mjs` in headless Chromium (1248, the
  revision playwright-core 1.64.0 names), so Kokoro, Piper, their models and their phonemisers are
  exactly the page's; a Python port (kokoro's misaki G2P, piper-tts's native phonemiser) would be
  another pronunciation. playwright-core replaces nothing — driving a browser's WebGPU and module
  workers by hand over the DevTools protocol is not reasonable. It asks the site's WebAssembly
  compiler (as `check-wasm.mjs` does) for `/presets.json` (new: the presets with their language, one
  source for it) and for each preset's `/reading` under `breath`, `arrives`, `stanza`, outro off,
  and outro off + `arrives`, takes every line any of them speaks with its natural pace (a pace that
  differs between settings throws), renders it in each voice of the poem's language at that pace
  (baked in), and encodes MP3 with ffmpeg (mono, 32 kbit/s, the voice's rate, bit-exact flags, no
  tags). Files are `<engine>-<voice>/<key>.mp3`, key = first 16 hex of SHA-256 of the line's UTF-8
  text; the manifest (deterministic, sorted, a line of the file per voice and per line) carries
  each line's text, pace and each voice's file and length, and each voice's licence and what it was
  rendered on. Incremental (only missing files unless `--force`), prunes files nothing wants,
  `--check` renders nothing and fails on anything missing, left over or a stale manifest. A per-line
  timeout restarts the browser: on the first full run Kokoro on WebGPU stopped answering after 385
  lines; in a fresh browser the same line rendered at once, and the rerun had no stalls.
- **Voices** (`lib.mjs` `VOICES`): Kokoro `af_heart` and `bm_george` (Apache-2.0, fp32 on WebGPU),
  Piper `en_GB-cori-medium` (piper-voices MIT, LibriVox public-domain data) for English; Piper
  `sl_SI-artur-medium` (ARTUR corpus, CC BY 4.0, attribution in README) for Prešeren. Chosen for
  licences that allow publishing the audio; the non-commercial Piper voices are left out.
- **What it made.** 18 poems, 330 distinct lines (323 English, 7 Slovenian), 976 recordings
  (323 × 3 + 7), **8,994,308 bytes** with the manifest (121 kB): Heart 3.0 MB, George 3.8 MB, Cori
  3.4 MB, Artur 48 kB (du). Committed size well under the 25 MB line. Kokoro on WebGPU ~0.3 s a line,
  Piper ~0.3 s.
- **Serving.** `prosody-web` serves `/readings/…` from disk (`--readings DIR`, the checkout's
  `examples/readings` by default) in the binary, not the library, so the WebAssembly build never
  carries the audio: `readings.rs` answers only `manifest.json` or `*.mp3` by a path of plain names
  (no `..`, no hidden files), 404 otherwise; the server now writes bytes (`Reply`). `build-site.sh`
  copies the directory into the site whole.
- **The page.** A fifth engine, `recorded`: `load` fetches the manifest, `voices(language)` lists the
  recorded voices of the language that have recordings of the poem heard, each "(n of m lines
  recorded)", refreshed on each compile and A/B switch; `render` fetches, decodes (an
  `OfflineAudioContext` of its own, so it works before Strudel's context exists), resamples only if
  asked for another speed than it was recorded at, and trims the silence (an MP3's encoder delay
  included) to the worker's 10 ms margin and `lead`. Scheduling is the neural engines' own. Per line
  (`speakerOf`): a recorded line from its file; one without (edited, or a poem of the writer's own)
  by the recording's live voice if its engine has loaded, otherwise the browser voice — no model is
  downloaded unasked — and the status says `line N: no recording of it; read …`. Recordings are
  fetched at once, outside the serial render queue: a browser check found line 1 started 5.9 s late
  behind a Piper render left over from a moment before; after the change it starts on its cue. At
  the fitted pace a recording plays at its own speed (its pace, not the fitted rate) and is only
  resampled faster if it overruns its bar, through the existing refit.
- **Tests.** Rust: `/presets.json` by GET and POST (Prešeren `sl`, one language per preset); the
  page has the fifth engine, the manifest and file URLs relative to the page, the per-line fallback
  and its notes, the per-poem voice counts, the direct fetch; `readings.rs` refuses `..`, hidden,
  empty segments, absolute paths, other extensions; and
  `every_line_of_every_example_is_recorded_in_every_voice_of_its_language` checks the committed
  manifest against the compiler — every line every setting speaks, at its pace, with a non-empty
  file in each voice of its language, 976 in all (mutation-checked: a pace changed in the manifest
  fails it, a deleted Artur file fails it). Node (`npm test`, 5): SHA-256 keys by hand from FIPS
  180-2 and known digests ("abc", "", "a", "b"), file naming, lines and the pace conflict, voices
  per language, the manifest's exact bytes for a hand-built case.
- **In the browser** (headless Chromium via Playwright, port 4923). Served page: Crane, recorded
  Heart: every line from its file, each source started its 10 ms lead before its scheduled time
  (line 1 cue 0.257 s, start 0.2500); line 3 edited → "(9 of 10 lines recorded)" in all three voices,
  read by the browser voice, chained on its end, said in the status; George chosen while playing →
  the next round all George; Prešeren at the fitted pace: language `sl`, the menu lists only Artur
  (7 of 7), lines 2–7 from files on their cues (line 1, cued 30 ms before its fetch finished at the
  preset change, read by the browser voice, said); Piper loaded then recorded Cori: the edited line
  read live by Piper `en_GB-cori-medium`. Static site (WASM compiler, `python3 -m http.server`):
  The Tyger with line 5 edited, "(22 of 23 lines recorded)", Heart then Cori mid-piece, 22 files
  each fetched from `readings/`, the edited line by the browser voice. 0 console errors and 0
  warnings after the decoder fix (before it, 12 warnings: `getAudioContext is not defined`, a fetch
  before Strudel started, retried). Nothing was listened to; WebKit/Safari, where MP3 was chosen
  for, was not run.
- **Raised, not done** (in the report): whether `recorded` should be the default engine on the
  static site, or the fallback when a live engine fails, instead of the browser voice; whether a
  line with no recording should load the recording's live engine; the fitted pace's line 1 race at
  a preset change (shared with the neural engines).

### Reading engines — second review: the seam, a stale queue, a slow engine's order

A second review of the uncommitted engines work. `SPEC.md` §4.7 ("Engines", "Two paces").

- **A reading that runs over the loop's seam no longer loses the next round's first stanza.** When
  the round changed while the chain was busy (the browser voice still saying the last line, or a
  neural line still rendering), the chain's position was not moved and the anchors were cleared,
  so the line's `after()` stepped past the end and the next stanza's cue jumped there: stanza 1
  went unread, with no note. Now the move waits (`wrapTo`) and is taken when the line ends
  (`wrapRound`), and a line of the old round left unread is said. Checked by running the page's
  own chain code in node with a virtual clock (two stanzas of two lines, 16 s piece, 3 s browser
  lines): before, round 2 read lines 3, 4 only; after, it reads 1 (at 20 s, its breath after the
  last line's 16.3 s end), 2, 3, and says line 4 was not read when round 3 came round mid-line 3.
- **The render queue holds only lines still to be read.** `prerender()` dropped jobs only for a
  change of engine or voice, so an old poem's unrendered lines (and, after a pace change, fitted
  jobs) stayed ahead of the new poem's. It now keeps exactly the lines of the playing piece and
  of A and B at the chosen pace, engine and voice, and drops the rest. A preset in another
  language first lets go of the old poem's reading (stopped, its readings cleared, so its lines
  leave the queue) and then switches the language, so neither poem is rendered in the other's
  voice; a compile whose text was already replaced no longer asks for its lines.
- **The line due next is rendered next.** The chain asked for line n+1 only once line n had
  rendered, by which time the queue had started the next job in poem order from the top, so on
  Kokoro/WebAssembly every line waited for an unrelated render (4–6 s late). The queue is now in
  wanted order — the playing piece from the line its reading is at, round to the one before — and
  re-ordered every time the reading moves on (`advance`, `fittedCue`).
- Comments on `dropJob`/`prerender` now say what they drop.

### Reading engines — review fixes: no stalled reading, honest status, fitted lines fit

A review of the uncommitted engines work found the reading could stop for good. `SPEC.md` §4.7
("Engines", "Two paces").

- **A change of engine or voice no longer strands lines.** `prerender()` dropped queued jobs for the
  old engine/voice but left their `renders` entries pending for ever, so going back to that engine
  or voice found an entry, queued nothing, and the natural chain waited with `busy = true` until
  the next play (reproduced by the reviewer: Kokoro → Piper → Kokoro, and cori → ryan → cori →
  ryan). A dropped job now takes its entry with it and rejects it as `dropped` (`dropJob`); the
  natural chain, on `dropped`, asks again for the line from whoever reads now instead of skipping
  it; and `whenRendered` re-queues any entry that is neither queued nor rendering.
- **A worker that dies after loading fails its engine** (`engine.crashed`): waiting requests are
  rejected, the worker let go, the status says so in red, the browser voice reads, and choosing the
  engine again retries. `ask()` on an engine with no worker rejects rather than throwing.
- **Only the chosen engine speaks in the status bar.** A failed engine is retried only when it is
  chosen again (`ensureEngine(retry)`, true only from the engine menu), not on every change of
  pace or language; load, ready and failure messages of an engine no longer chosen are not shown;
  a render's download progress is shown only while its voice is the chosen one, and cleared when
  the render finishes.
- **"browser default" works again**: `voiceOf('browser')` is `null` (the system's voice for the
  language) unless a listed voice is saved.
- **Fitted lines fit.** The fitted rate assumes 4 syllables a second at rate 1; a neural render that
  runs past `fill` (85%, now sent in `/reading` as `fill`, from `reading::FILL`) of its bar is
  rendered once more at `rate × speech / room` (at most 2.0) and that render is read once ready; a
  line still running past its bar is said. Measured on Piper ryan: "In what distant deeps or
  skies." 1.861 s of speech for a 1.686 s room was re-rendered at 1.158 (1.803 s — VITS speed is not
  linear in its length scale), inside its 1.98 s bar, so nothing was cut.
- **The I2 proptest cannot pass vacuously.** It no longer skips lines missing on one side: a stanza
  must be heard on a side exactly when that side's `natural_unread` does not list it, every line of
  a stanza heard on both sides is compared, and the count compared must equal the lines of the
  stanzas heard on both sides. Mutation-checked again (pace seeded from the whole source fails it).
- **Tests.** `/reading` carries `"fill":0.850`; the page drops a job with its entry, fails a crashed
  engine, retries a failed one only on being chosen, and refits a fitted line.
- **Browser (headless Chromium, `?device=wasm`).** The reviewer's stub reproduction (5 queued
  Kokoro lines, switch to Piper, ask again) now resolves all five (it left four hanging). Real
  runs: Kokoro WASM ready with 7 queued → Piper → Kokoro: the next round read all 8 lines by Kokoro,
  heard exactly when scheduled, nothing pending; Piper cori → ryan → cori → ryan: next round read by
  ryan, heard within 0.1 ms; on the static site cori → ryan → cori mid-stanza: stanza 2 and the
  next round read, heard within 0.1 ms. Kitten fails, pace change does not retry it, choosing Piper
  leaves a clean status. A simulated crash (`engines.piper.crashed`) rejects the render in flight,
  shows the red status, reads with the browser voice; choosing Piper again reloads it.

### Reading engines in the page — four voices, and a natural pace

The user decided: one engine interface on the page with four engines to choose between, a page
setting remembered like the voice picker (not a style key — the CLI never reads aloud); and, added
after the plan, a **natural pace** as the default, since squeezing every line into its bar sounds
forced. `SPEC.md` §4.7 ("Two paces", "Engines", "Language").

- **Rust, the natural pace** (`prosody_emit::reading`). Per line: a `Pace` in hundredths, 92..=108,
  `Seed::for_unit(Domain::Line, line span)` child `"reading pace"`, one draw modulo 17 — local to
  the line (I2), integer everywhere but the speed handed to a voice; a `Pause` from the line's last
  character that is not space or a closing quote/bracket — run-on 250 ms, comma/semicolon/colon/
  dash 450, full stop/?/!/… 800, a stanza's last line 1500 (estimates, documented); per-word
  syllable counts (`Word`); its stanza. An `Onset` per line: under `breath` a stanza's first line
  `Breath { slot }` at its **arrival** (not the fitted breath slot — the breath is measured on the
  page against the previous stanza's actual spoken end), the rest `Follows`; under `stanza`
  `At { slot }` then `Follows`; under `arrives` every line `At` its arrival. A second cue pattern,
  `Reading::natural_pattern`, carries the onset bars, its header every line's onset, pace and
  pause; `natural_unread` lists stanzas never begun. `bars()`/`cpm()` give the page the clock. The
  fitted pattern and every existing cue are unchanged (`Cue` now wraps a `Spoken`).
- **Web.** `/reading` gains `natural`, `bars`, `cpm`, `lines` (each with `onset`, `slot`) and on
  every spoken line `stanza`, `pace`, `pause`, `ends`, `words`; `natural_unread`. Presets carry a
  language (`PRESETS` 4-tuples; Prešeren `sl`, the rest `en`). `GET /speech-worker.mjs`, written by
  `--site`, so the static site has it too.
- **The page.** Engines `browser` (speechSynthesis; instant, the fallback, word boundaries),
  `kokoro` (default), `piper`, `kitten`, each neural one in its own module worker
  (`speech-worker.mjs`): `load(onProgress)`, `voices(language)`, `render(text, {voice, speed})` →
  `AudioBuffer`. Loaded on choice or on the first play; progress in the status bar; until ready,
  or on failure (said in the status and the engine menu; choosing it again retries), the browser
  voice reads. Every line rendered ahead after each compile, cached by engine|voice|speed|text, at
  the natural pace's speed (or the fitted rate), trimmed of silence with a 10 ms margin whose length
  (`lead`) the page starts early, so the speech lands on the time. Both cue patterns are stacked
  with the music; the fitted trigger starts a buffer at `targetTime` and cuts the last at it; the
  natural trigger anchors a chain that schedules each line at max(anchor, previous end + pause) —
  for a `breath` onset on the first bar ≥ that and ≥ two bars after the bar the last line ended in
  — on the audio clock, a stanza at a time; the browser voice is chained on `onend`. A line that
  would start at or after the piece's end is not read that round, and said. Voice through a
  `voiceBus` (the reading level, 0–2) into the master's glue; the music ducked per stanza on the
  audio clock (held through pauses). Word marker: spread by syllables for rendered lines. Language
  menu; a preset sets it; an engine without a voice in it gives way to the browser voice and says
  so. `?device=wasm` keeps every engine off WebGPU.
- **Dependencies (CDN, exact versions, each checked against its npm tarball, sha-256 identical on
  jsDelivr):** kokoro-js 1.2.1 (`dist/kokoro.web.js`, bundles Transformers.js 3.5.1; English
  voices only, 20 en-us + 8 en-gb; Apache-2.0) — replaces nothing, hand-rolling Kokoro's
  phonemiser and tokenizer is not reasonable; onnxruntime-web 1.18.0 (`dist/esm/ort.wasm.min.js`)
  and the espeak-ng phonemiser ES module of @mintplex-labs/piper-tts-web 1.0.5 (published
  2026-08, the maintained fork of @diffusionstudio/vits-web, last 2024; piper-tts-web 1.1.2 ships a
  46 MB bundle) with its binary/data from @diffusionstudio/piper-wasm 1.0.0 — the package's own
  `TtsSession` was not used because it is a singleton that keeps its first voice and has no speed
  control, so the ~30 lines of inference are here (phoneme ids → `scales` with `length_scale /
  speed`); voices from `rhasspy/piper-voices` pinned at `5041a7a…`; kitten-tts-webgpu 0.1.1
  (WebGPU compute shaders only; no WebAssembly path). Licences in README "Voices and their licences".
- **Tests.** Hand-worked (`reading.rs`): the pause table on twelve endings; the pace's draw onto
  seventeen steps (0→92, 16→108, 17→92, 25→100, 2³²−1→92 because 2⁸ ≡ 1 mod 17) and its locality;
  the natural onsets of TRANSFORMING under all four variants and of THREE (stanza 2 at its arrival,
  slot 3, where the fitted pace has 4), their cue patterns, header lines and bars; per-word
  syllables; I3 (another palette moves no onset, pace or pause). Properties (`tests/breath.rs`):
  the natural plan against the `arrives` plan on random poems, all 18 examples and all 18 without
  the outro; I2 — replacing one stanza leaves every other stanza's lines' pace and pause unchanged
  (mutation-checked: seeding from the whole source fails it). Web: the JSON by hand for the seed's
  first line (bytes, words 0..5, 6..11, 13..20, 21..27 with 2, 2, 2, 1 syllables, pause 0.450
  comma); Pied Beauty outro off (fitted loses line 11, natural lists it); the worker route; the
  engines, pins and master routing in the page; Prešeren's language.
- **In the browser** (headless Chromium via Playwright, port 4922; a test-only AudioWorklet tap on
  the voice bus, recording the first sample above −50 dBFS after 0.3 s of quiet). Served page,
  `?device=wasm`: Kokoro q8 on WebAssembly loaded (92 MB) in 12.7 s, 3.6 s to render a 1.66 s line;
  Piper loaded in 1.0 s, `en_GB-cori-medium` and `sl_SI-artur-medium` (63 MB) rendered, speed 1.08
  shortening a line 2.150 → 1.927 s, two sentences in one line both voiced (3.86 s). Natural pace,
  seed poem, Piper: line 1 scheduled at its bar-1 cue (0.2007 s) and heard at 0.2007; every line
  heard within 0.1 ms of its scheduled start; stanza 2's cue at 8.135 s, stanza 1's voice ended
  8.46 s inside that bar, so it started at 12.1015 = two bars later, as the rule says. Switching
  to Kokoro while playing: status "Kokoro-82M loading; reading with the browser voice meanwhile",
  the next round read by Kokoro from its bar-1 cue exactly (47.804). Kitten under `?device=wasm`:
  "could not load: Kitten runs only on WebGPU…", red, the menu says so, and the browser voice read
  the next stanza, chained on `onend` (line 2 at line 1's end + 0.45 s). Fitted pace with Piper:
  all eight lines started at their cue times exactly. Prešeren preset: language `sl`, voice
  `sl_SI-artur-medium`, read from its cue at 280.419 s. Static site (WASM compiler), The Tyger,
  Kokoro on WebAssembly: lines heard within 0.3 ms of schedule, 0 skipped ticks while the worker
  rendered. Headless Chromium on this Mac *does* offer WebGPU (Metal): Kitten nano loaded in 4.4 s
  and rendered a line in 0.30 s; Kokoro fp32 on WebGPU loaded in 31 s and rendered a line in 0.31 s
  (ORT logs two benign node-placement warnings as errors). Nothing was listened to.
- **Raised, not done.** Kitten has no WebAssembly path (an ONNX port on onnxruntime-web would
  need the voices' NPZ parsed and Kitten's symbol table); WebAssembly Kokoro renders at about half
  real time, so a first play waits for lines (said: "waiting for Kokoro-82M to render line N"); the
  stanza-end pause is mostly a floor under the one-silent-bar rule; Piper's VITS noise makes a
  re-render not bit-identical, so a stanza's start can differ by a few ms between machines.

### Erosion, second review — editing "Patterns" while it plays

Review found that a live edit to "Patterns" while it played stopped the music: on the static page
the compiler (pattern and trace for A and B, then both scores) ran on the page's own thread, one
7.2 s block and 170 skipped ticks; on the served page Strudel evaluating the 0.9 MB pattern blocked
about 0.9 s (16–17 ticks, sometimes a reading line). Fixed:

- **The static page's compiler runs in two Web Workers** (`index.html`, `ask` → `threadFor`): one
  answers what plays, one only scores, so a new pattern never waits behind the last one's scores.
  Each loads its own copy of `prosody_bg.wasm` from a blob-made module worker, given the bindings'
  absolute address. Without module workers it falls back to compiling in-process, as before.
  Compilations and measurements are each one at a time and of the newest poem only, so typing into
  a long poem never queues one per keystroke and answers never arrive out of order.
- **The page parses ahead and remembers.** Evaluating a pattern is almost all Strudel parsing its
  mini-notation (Node, @strudel 1.2.6: "Patterns" 1,206 ms plain; 1,111 of 1,119 ms parsing). The
  page sets Strudel's string parser (`setStringParser`) to one that remembers each string's
  pattern, and before each evaluation parses the strings it has not seen in 8 ms slices between
  ticks. After the "Christ!" → "Lord!" edit 9 of the evaluation's 1,189 string lookups miss, and it takes
  47 ms. Remembered and plain evaluations give identical haps: 216 evaluations (18 examples × 4
  settings × pattern, trace, reading), 942,040 haps. Entries no play has used in the last 3 are let
  go.
- **A skip is never left unreported.** Ticks skipped while a new pattern was being prepared counted
  against the old pattern's record, whose report was then dropped; the new record now takes over
  what the old had not yet said (`behindFrom`). A 1.5 s stall followed by a replay at once reported
  all 26 skipped ticks.
- B still starts `long-stanzas` at `erode`: with the freeze gone, the reason is now cost —
  `gradual` adds about 5 s of compiling and 8 s of scoring to every edit of "Patterns", and A waits
  for B's pattern. Still a spec question.

In the browser (headless Chromium, 20 ms probe, blocks over 100 ms recorded): on both pages,
loading "Patterns" while Tyger plays, 25 s of play, the edit, and A→B→A switches gave 0 skipped
ticks and no block over 100 ms; audio starts 196 for 98 onsets over 10 s. Static page: the edit
reached the scheduler in 2.1 s, its scores in 8.5 s; served page: 1.1 s. B set to `gradual` by
hand: 3.6 s to compile, 12.5 s to score, 7.2 s to switch to, with blocks of 113–158 ms and 0 skips.

### Erosion, review fixes — "Patterns" plays whole on the page

Review of the erosion step found the page played about a sixth of "Patterns" (Strudel skipping
~12 ticks a second), lost most reading lines with it unsaid, and froze for seconds on an A/B
switch. Fixed:

- **The pattern is grouped** (`strudel::nested`, `GROUP` = 16; the steady `<…>`, the elastic one
  via `one_a_cycle`, and the reading's cues). A sequence of more than 16 elements joins its runs of
  rests into one rest as long, then brackets elements 16 at a time, recursively. Cause, read in
  `@strudel/core` 1.2.6: a weighted sequence is a `stack` of every element `_compress`ed, visited on
  every query; the flat elastic `<a b …>` measured as slow. Effect under Node with the page's
  `@strudel/core`/`@strudel/mini` 1.2.6: "Patterns" 48.6 → 3.9 ms a 0.1 s tick, eval 2.0 → 1.4 s,
  1,856,772 → 898,178 bytes; elastic B 55 → 4.8 ms. **Same music:** every hap (whole, part, value)
  of a whole loop compared old vs new for 18 examples × 6 settings × pattern, trace and reading:
  324 answers, 223 of the 322 compared with changed text, 319,091 haps identical — all but "Patterns" under `gradual`
  (pattern and trace), whose old 8.6 MB text runs Node out of memory, so it was not compared; and
  the two changed villanelle snapshots (2,695 haps identical). Text changed for the 13 examples over 16
  bars and the two `corpus__pattern-villanelle*` snapshots (read: rests joined, e.g.
  `~@5 ×8` → `~@40`; 24 bars, so ≤16 elements after joining, no groups). OSC, explain and every
  score are byte-identical before/after for all 18 (90 files); `cycle.sh` means unchanged (0.703
  machine, 0.729 jazz over 18; 0.711/0.733 over the 17).
- **Lost ticks are said.** The page wraps `console.log` for Strudel's `skip query: too late`
  (verified in the published @strudel/web 1.3.0 bundle: the cyclist sets `lastBegin`/`lastEnd`
  before logging), asks the cue pattern which lines fell in the skipped span, and puts
  `Strudel fell behind: N tick(s) not played, their notes unheard; not read: line …` in the status
  bar in red (forced with a 3 s main-thread stall: 58 ticks, line 10 named).
- **B starts eroding** (`B_STARTS_AS_A = ['long-stanzas']` in the page): `gradual` is still the
  first variant and still selectable, but B no longer loads the quadratic piece behind every long
  poem.
- **Lanes over an eroding line** documented in `SPEC.md` §4.6 rather than changed (an eroding line
  counts as sounding, so call/held/refrain do not double it).
- Also: the page's warm-up no longer asks for sounds named `~@5` (weights now stripped).

Browser (headless Chromium, port 4921), "Patterns" A over 40 s: native 0 skips, 650 starts for
317 onsets (2.05 — Tyger 1.98), reading lines 2–9 and 11–13 all cued; load 0.75 s, scores 3.6 s,
A→B 0.63 s, B→A 1.1 s. WASM site: 0 skips, ratio 2.06, same lines; load 8.0 s (a 6.3 s main-thread
block — compiling and scoring both sides synchronously), A→B 0.69 s, B→A 1.1 s (were 21.6 s and
26.3 s). B set to `gradual` by hand on the WASM site: 12.1 s to compile and score (8.6 s block),
A→B 2.3 s, then it plays with 0 skips.

### Erosion for long stanzas — loss, and "Patterns" back as the 18th example

The user decided: for long pieces, *loss*, as erosion (Basinski). The line-by-line handover makes
a stanza of *n* lines take ~*n* passes of ~*n* bars, so "Patterns" was 1,331 bars and 8.6 MB.

- **The rule** (`prosody_core::arrange`, `SPEC.md` §4.6). Where a stanza arrives into a running
  loop and either has more than `LONG_STANZA` = 8 lines, the arriving stanza is stated whole for
  `EROSION_PASSES` = 3 passes (each one statement of it: `Arriving { pass }`), and the old loop
  loops under it from its first line, a line a bar (`Cause::Eroding { pass }`). On pass *p* an
  eroding line of *k* syllables keeps ⌈k·(4−p)/4⌉ (¾, ½, ¼), then the loop is gone. Order
  (`arrange::erode`, documented at the mapping site): unstressed, then secondary, then primary
  stress, the rhyme word last; within each, earlier first. **Decided here:** the rhyme word is the
  line's *last word*, not "a pitched word" — pitch depends on rhymes elsewhere in the poem, which
  would have made a line's erosion depend on other stanzas (I2). **Decided here:** a pass is one
  statement of the *arriving* stanza (so host and loop share pass numbers and the arrival ends on
  its own last bar); the alternative, a pass of the old loop's own length, is in the open
  questions below.
- **Selector** `Mapping::LongStanzas` (`long-stanzas`): `gradual` (first, what preceded it; no
  `off`) and `erode` (default). It reaches the page's A/B table, the CLI `--set`/usage and the web
  `set=` through `Mapping::variants` (tested: page JSON `["gradual","erode"],"erode"]`, CLI refusal
  of `long-stanzas=off`, web `set=long-stanzas:gradual` named in the explain view).
  `Mappings::every` grows 8192 → 16384.
- **One rule for every consumer.** `Score::placed` drops what an `Eroding` pass loses, an eroding
  line keeps its whole line's beats; `Score::sounds` / `Placed::sounds` replace the span-only
  `Part::covers` filter in the Strudel walk (`grid_by_slot`, `Lane::all`) and the trace. The OSC
  schedule, the scorer's timeline and its edit test already read `placed_in`. The reading reads
  hosts only, so an eroding line is never read again. Header lines: pattern
  `//   long-stanzas = erode: N bar(s) over an eroding loop …`, OSC `# long-stanzas = erode: …`,
  explain `long-stanzas erode · …` and, per eroding line, `erodes pass 1 keeps 6 of 7 · pass 2
  keeps 4 · pass 3 keeps 2: figured`; all from `Arrangement::eroding`. Nothing is printed where
  nothing erodes.
- **"Patterns" restored** (`examples/lowell-patterns.poem`, PG #841, verbatim except: 3-space
  indent removed, "…and the buckles / on his shoes." rejoined, "--" as an em dash; palette E minor,
  tr707, 92–100 bpm). Stanzas 9, 18, 15, 16, 22, 10, 17. **284 bars** (was 1,331), **1,856,772
  bytes** (was 8,602,140), compile 0.79 s, `--score` 2.8 s native release (was 5.7 s; gradual still
  5.7 s). Stanza 1 is foundation and stanzas 2–7 flourish, so stanza 1 loops under the whole piece
  unchanged (nothing competes with it) and 240 bars play over an eroding loop. A preset, a corpus
  entry (3 snapshots, new: machine 0.577, jazz 0.667, interest 0.764; edit test 646/646 audible,
  511 local, 598 explained), an `examples/README.md` row (and its stale "nine" counts fixed), the
  breath test's example count 17 → 18, `check-wasm.mjs` gains `set=long-stanzas:gradual`.
- **Unchanged, proven.** Every existing snapshot untouched. The 17 other examples' CLI pattern,
  OSC, explain and two alternative-mapping patterns byte-identical before/after (85 files).
  `cycle.sh` rows for the 17 byte-identical: machine **0.711**, jazz **0.733** over the 17; with
  "Patterns", 0.703 and 0.729 over 18. `check-wasm.sh`: 1,170 answers byte-identical.
- **Tests.** Hand-worked (`prosody-core/tests/arrange.rs`): the full arrangement of a 9-then-2 and
  a 2-then-9 poem under erode and gradual (15 vs 20 slots; 29 slots), what `ba bA ba bA` and
  `ga gA gAga` keep on each pass (the rhyme word whole outlasting a stress on pass 2), the 8/8
  boundary (identical under both) and 8/9 (erodes). Properties (`tests/erosion.rs`, 128 cases):
  the count, monotone subset and rank order of what each eroding placement keeps (I4 spans
  resolve); I1; poems with no stanza over 8 lines are equal `Score`s under both variants for any
  other choice; each erosion takes exactly 3·n bars hosted `Arriving { pass }`; I2 — an edit to
  another stanza leaves what every line keeps on each pass unchanged; a generator-reach check
  (14 of 64 generated poems erode). Emit (`tests/erosion.rs`, real dictionary, "Patterns"): per
  slot, Strudel drum strokes, trace marks (by span) and OSC messages equal the arrangement's
  placed events — mutation-checked by restoring the span filter in `grid_by_slot`; the reading
  reads all 107 lines once, never in a slot where the line erodes. Scorer: timeline hits per slot
  equal placed events; a hand-worked edit test (22 edits: 22 audible, 18 local and explained — the
  4 not are stanza-2 edits that flip it to flourish, so nothing erodes). CLI and web tests above.
- **In the browser** (headless Chromium, port 4921). Native server: the preset compiles A and B in
  1.8 s (A's pattern 0.73 s, B — gradual with every other mapping at its first variant — 1.7 s,
  4.0 MB); the Score tab's two `/score` requests took 2.7 s (A) and 4.7 s (B). It plays, no
  console errors, but Strudel logs **"skip query: too late"** ~11 times a second (87 in 8 s; 0 for
  The Tyger, Whitman, Ball): querying the 1.86 MB pattern costs 73 ms per 0.1-cycle tick in the
  page (Tyger 9.6 ms), so the scheduler falls behind — dropouts are likely. Every drum onset in
  all 284 slots has its trace onset (15,133 each). Static site (WASM): the preset takes 11.8 s
  to compile and score (8.4 s with B set to A's choices), plays with 161 late queries in 8 s and a
  few "cannot schedule sounds in the past" warnings, scores 0.577 vs 0.434. Nothing was heard.
- **Raised, not done.** (1) A pass as the *old* loop's length instead of the arriving stanza's
  (fully local erosion, but a short stanza arriving into a long loop would host ~3·m bars).
  (2) "Patterns" is still too heavy for Strudel's scheduler in the page; the pattern's size, not
  the bar count, is now the limit — e.g. a stanza-1 loop under 284 bars, or a shorter mini-notation.
  (3) Whether stanza 1's foundation loop, never competed with, should also erode over a long
  piece (today it loops whole to the end).

### Eight more poems

Eight public-domain poems join the examples (each also a preset on the page and a scored corpus
entry): Ball's *Karawane* (a Dada sound poem — no dictionary word at all), Stein's *Tender Buttons*
(prose lineated a sentence a line, noted in its header), Crane's "In the desert", Dunbar's "We Wear
the Mask" (a rondeau), Naidu's "In the Bazaars of Hyderabad" (call and response), Tagore's
*Gitanjali* 35 (anaphora), Psalm 23 (KJV) and Lowell's "Night Clouds". All published before 1931 by
authors who died before 1956, so public domain in the US and the EU; each file's header names its
source (Wikisource or Project Gutenberg), and each text was taken from that source, not from memory,
with any editorial change stated there.

- **Lowell's "Patterns" was tried first and dropped**: 107 lines in stanzas of up to 23 arrange into
  1,331 bars and an 8.6 MB pattern (6 s to score natively), because a transforming stanza replaces
  one line a pass — growth with the square of stanza length. The user chose a short Lowell poem
  instead, and asked about introducing *loss* for long pieces; that is a structural question, open.
- Machine profile over 17 poems: mean overall 0.711 (the original nine unchanged, 0.743 among
  themselves). Highest: Naidu 0.954, Dunbar 0.949; lowest: Stein 0.502, Prešeren 0.503, Tagore 0.539
  — prose poems and unmetred verse have the least pulse and steadiness to give the machine bands.
  Jazz: 0.733. `check-wasm.sh` now compares 1,020 answers.

### The breath says what it leaves unread — a lost line is listed, not silent

Review of the breath step: with `outro` off, Pied Beauty (eleven bars) lost its closing line,
"Praise him.", under `breath` and nothing said so — the cue header read "10 lines over 11 bars"
(CLAUDE.md §6: a fallback must be visible). Hand-worked: stanza 1 in bars 1–6; stanza 2 arrives in
bar 7, waits for its breath, max(7, 6 + 2) = bar 8, so bars 8–12, and bar 12 is past the piece.
`arrives` and `stanza` read all eleven. The rule is unchanged; the loss is now said.

- **`Reading::unread()`** (`prosody_emit::reading::Unread`, with `Why::PieceEnds` /
  `Why::NeverArrives`): every line no cue speaks, under every variant but `off`, with its span and
  text (I4). A line of a stanza that was read in part, or one that arrives, lost its turn to the end
  of the piece; any other never arrives.
- **Said in three places:** the cue pattern's header (`// 1 of 11 lines not read:` and
  `//   line 11 (bytes …) is not read, the piece ends before its turn: Praise him.`), the web
  `/reading` answer's new `unread` list, and the page's status bar and console while it plays
  (`not read: line 11, the piece ends before its turn`). Nothing changes when every line is read.
- **Tests.** Hand-worked: Pied Beauty outro off — 11 slots, arrives bars 1–11, breath 1–6 and 8–11,
  line 11 unread with `PieceEnds`, both header lines exact; the same through the web `/reading`;
  a unit test of the reason (a cut stanza's lines `PieceEnds`, a never-heard stanza's
  `NeverArrives`, bytes 17..20). The breath property now also checks the unread list against the
  cues on random poems, every example, and every example with the outro off.
- `SPEC.md` §4.7's heading no longer describes `arrives` ("the poem spoken over the music, a stanza
  at a time"), and the section says how a lost line is reported. Snapshots and `cycle.sh`: unchanged.

### The reading takes a breath — stanzas read whole, a silent bar between, never ahead

The user decided: under the old default `arrives`, a transforming stanza's lines were read one a
pass (five bars apart) and each stanza's first line straight after the previous stanza, so a stanza
sounded joined to the one before and then broken by long pauses (The Tyger: lines 1–4 in bars 1–4,
line 5 in bar 5, lines 6–8 in bars 10, 15, 20). A **new default variant, `breath`**; `arrives` and
`stanza` stay as variants.

- **The rule** (`prosody_emit::reading::by_breath`). Each stanza is read through, one line a bar, in
  order. Stanza *k* starts at max(its first line's first arrival as a host — `Stated`, `Arriving`,
  `Overruling`, the same arrivals `arrives` reads — , stanza *k − 1*'s last cue + 2). A stanza whose
  first line never arrives starts at the earliest arrival of any of its lines; one with no arrival
  is skipped and leaves no breath.
- **Past the end, decided:** a line that would fall after the piece's last slot is not read (as
  under `stanza`); a stanza that would start there is not read at all. A later cue would wrap onto
  the next round's first bars, and starting earlier breaks the breath or reads ahead of the music.
  No example loses a line; four one-line stanzas do (bars 1, 3, 5 of a five-bar piece, the fourth
  unread) — raised, not decided further (`SPEC.md` §4.7).
- **Selector.** `Variant::Breath` (`breath`), `Mapping::Reading`'s fourth variant and its default
  (`off, arrives, stanza, breath`). It reaches the page's A/B table, the CLI's `--set` and usage, and
  the web `set=` through `Mapping::variants`; `Mappings::every` grows from 6144 to 8192. The cue
  pattern's first comment says `reading = breath` and what it does; the explain view (CLI and web)
  now lists `reading=arrives` when the old reading is chosen, and nothing under the default.
- **Timing.** Nothing new on the clock: a breath cue is an ordinary cue in a slot, weighted as the
  music's bar under the steady pulse, and its rate is fitted to *that* slot's bar — which under a
  breath is often not its own line's bar (the strudel clock test: line 2's eight syllables read in a
  three-beat bar of the old loop, 1.569, where `arrives` read it in its own four-beat bar, 1.176).
  The trace, the music, the OSC schedule and the roll are untouched; the page's reading wash follows
  the cue that fires.
- **Tests.** Hand-worked: `TRANSFORMING` under breath (0, 1, 3, 4 and its cue pattern
  `<0@2 1@2 ~@2 2@2 3@2 ~@2 ~@2 ~@2>`), a new three-stanza fixture with its full 24-slot
  arrangement asserted and the cues of every variant (arrives 0 1 2 3 7 11 12 16 20; stanza
  0–5, 12–14; breath 0–2, 4–6, 12–14), the refrain fixture (breath 0 1 3 4 6 7), hand-made arrivals
  for the wait-for-music, wait-for-breath, cut-short, never-started, first-line-late and
  never-heard edges, the steady/elastic clock test, the web reading test on the seed (breath 0–3,
  5–8) and the Tyger plan. A proptest (`prosody-emit/tests/breath.rs`) checks every stanza's cues
  against the arrives plan on random poems and all nine examples (and that every example reads
  every line), with I1, I3 and I4. `arrives` and `stanza` tests now name their variant and are
  unchanged.
- **Snapshots: none changed** (the reading is not in the pattern, the trace or the explain view's
  default). `cycle.sh` machine and jazz: identical before and after (0.743, 0.737) — the scorer
  never reads the reading.
- **I2, disclosed in `SPEC.md` §4.7:** a stanza that waits for its breath has cues that depend on
  the previous stanza's reading, so adding a line to stanza *n* can move stanza *n + 1*'s cues by a
  bar. Cues are not voices; the voice property is untouched.

### The melody over the harmony — single rhyme notes on top of the chord lane

The user decided: with the jazz palette (`voicing = "seventh"`) and a harmony variant on, the rhyme
melody and the chord lane both played four-tone chords, which was thick. Now, **whenever the harmony
mapping is on (any variant but off), rhyme notes play as single notes on top of the chord lane,
whatever the voicing says; the voicing shapes only the chord lane.** Harmony off: unchanged. No new
variant, no core change.

- **Emitter** (`prosody_emit::strudel`). `melody_voicing(score, style)` is the style's `Voicing`
  unless the score has the harmony on (`!score.harmony().is_empty()` — the core records a harmony,
  chord or rest, for every line exactly when the mapping ran), then `Voicing::Note`. The melody
  layer uses it for its tones and for `chord_share`, so a single note takes the whole
  `melody_gain` (jazz: postgain 0.375 → 0.750). `melody_over_harmony(score)` gives the words for the
  header and the explain view. Both re-exported from `prosody_emit`.
- **Decided, and documented in `SPEC.md` §4.6:** the switch is "the mapping is on", not "some bar
  has a chord". The second is a whole-poem fact — an edit giving one stanza the poem's first chord
  would revoice every other stanza's melody (I2). So a harmonised poem with no chord anywhere still
  plays single notes; the header then says `single notes (… though no bar has a chord)`.
- **Views.** Pattern header: `melody on gm_epiano1 as single notes over the chords (the harmony
  mapping is on, so the voicing shapes only the chord lane)` in place of `as seventh chords (…)`.
  Explain view (CLI and web), in the style block on the palette's column: `rhyme notes   single
  notes over the chords (…)`. The page's Voicing tooltip, `README.md` and `SPEC.md` §5 say so too.
- **Timing: nothing moved.** Only note values and one postgain change; the trace, reading, OSC
  (which never voiced chords — one pitch a message) and the roll's bar maths are untouched.
- **Tests.** Strudel unit (hand-worked, ABAB in C dorian under jazz): harmony off, melody steps
  `60,63,67,70 / 62,65,69,72` ×4 at postgain 0.375; cadence, `60 / 62` ×4 at 0.750, chord lane
  still sevenths (`[55,58,62,65]` = V of degree 0). I3 property: under rhyme/cadence/both, `note`
  vs `seventh` differ only in the chord lane (3 vs 4 tones, same steps); the melody is never a
  stack; the header names it exactly when there is a chord. CLI and web end-to-end tests for the
  pattern and the explain row. The unit test and the property both fail with the change disabled.
- **Snapshots: none changed** (no corpus snapshot has the harmony on). Across five header-less
  examples × {default, jazz} × {note, seventh} × {off, cadence, both}, every `off` pattern and
  explain is byte-identical to before; under the harmony the only changes are the `plays:` header
  line, the explain row, and — seventh only — the melody lanes' notes (each chord → its lowest
  tone) and postgain (0.375 → 0.750). The chord lanes are byte-identical.
- **Measured**: every `cycle.sh` table (machine, jazz; off, cadence, both) is identical to before —
  the scorer reads the score, never the voicing. machine 0.743 / cadence 0.778 / both 0.763; jazz
  0.737 / 0.730 / 0.733.
- **Browser** (Wilde, jazz palette, B harmony=cadence): B's rhyme-note haps are one per onset (87
  onsets, notes 60/62…), chords below at 53 57 60 63…; A (harmony off) plays 60 63 67 70 stacks and
  no chord lane. No console errors or warnings.
- **Open, from review — register.** "On top" is a layer, not a register: the chord lane's sevenths
  climb from MIDI 48 to as high as 69 in C dorian, the melody starts at 60, so in Wilde under jazz +
  cadence many rhyme notes sit inside the sounding chord, some in unison with a chord tone (60
  against `[53,57,60,63]`). Not changed in code: whether "on top" means register is the user's call;
  if it does, a register offset for the chord lane (or the melody) under the harmony is palette and
  moves no event. Recorded in `SPEC.md` §4.6.

### The cadence's ending — the last bar split, its own chord then the turnaround

The user decided: under `harmony = cadence` (and `both`) a stanza's last bar had always been a
whole bar of V turning round onto the stanza's first chord, even when its line closes a rhyme, so
ABAB, AABB and ABBA never landed on I. Now that bar is **split**: its own chord first — I when the
line closes a rhyme, whatever H2 would otherwise give it — then, from the bar's half, V of the
stanza's own first chord (still I2-safe: it reads only its own stanza). No new variant; `rhyme`
and every line that is not last in its stanza are unchanged.

- **Core** (`prosody_core::harmony`). `Harmony::Turning { own: Option<Chord>, turn: Chord }` beside
  `Chord` and `Rest`; `Harmony::chord()` (from the bar's start), `turnaround()`, `chords()`,
  `chord_at(beat, beats)`; `Chord::onset()` (`Onset::Start`/`Half`, the turnaround's being `Half`).
  `Score::chord_in` became `Score::harmony_in(slot)`, the one place a slot's harmony is chosen, so
  every consumer has to handle the second chord; `Score::chords()` yields both.
- **Decided and documented** (`SPEC.md` §4.6): the turnaround's **span is the last line's rhyme
  word** — the line whose bar it shares; the first line, which gives it its root, is in its reason
  (`Turnaround { onto }`). **An unanswered last line keeps its IV** and turns round from the half
  (IV–V into the loop's first chord), rather than a whole bar of V — the user's rule is "its own
  chord first". Under `both` a last line that rhymes with nothing **rests for the first half**
  (`own: None`), then turns round. No turnaround, as before, for a one-line stanza, a last line with
  no word, or a stanza whose earlier lines have no chord. The half is the bar's, in time: beat 2 of
  4, beat 1½ of 3 (swing then moves it like any offbeat — palette).
- **W2's walk follows the split, at the bar's half.** A foot from the half of the bar its line is
  stated in (`2·beat ≥ beats`, `beats` the bar's) walks on the turnaround, the ones before on the
  bar's own chord; a repeat is now judged by (root, step), so the same step over another root is no
  repeat. The bar's length comes from the arrangement's own rule (`arrange::periods_by`, shared with
  the slots; `lower::own_bars`), since the bass runs before there is a score to arrange. A first
  draft turned round at the *line's* half; review found that under the stanza period (the default)
  a last line shorter than its stanza's longest walked on the turnaround while the lane still held
  the bar's own chord (an F against C minor). Over the shipped poems that was 9 feet (Annabel Lee 7,
  the Slovenian corpus poem 2) under cadence; now none: every W2 foot of a split bar, 57 of them
  under another host (under cadence), walks on the lane's chord. Tests: in `tests/bass.rs`, ABBA
  with a four-foot last line in five-beat bars (roots 0 0 0 8, steps 0 2 3 4) and its period-off
  twin (0 0 8 8, steps 0 2 2 4); in `tests/invariants.rs`, the property that W2 walks on the chord
  the lane is sounding, over rhymed ragged stanzas (it fails on the first draft). Left, and disclosed in §4.6:
  a walk is one per line, so under the period a stanza's last line looping under a flourish stanza
  of another period can turn a foot away from the lane — no shipped poem reaches it.
- **Every surface.** Strudel: a turning slot's element is `[own turn]` (or `[~ turn]`), gains
  likewise; the header gains `//   chords: a stanza's last bar is split — …` and writes the bar as
  `4:I/1+V/8` (`-+V/8` for a resting first half). Explain view: a second row, `harmony then, from the
  bar's half, V deg 8 (V of deg 4) · cadence: the stanza's last bar, …`. OSC: `# not scheduled here:
  110 chords of the harmony layer, 22 of them turnarounds from a bar's half` (Tyger), the same count
  on `--send`'s stderr; `Schedule::unsent_turnarounds`. Scorer: a second chord hit `slot_beats / 2`
  beats into the bar. The trace lights syllables only, never chords — checked, unchanged; the
  reading is unchanged.
- **Found and fixed on the way: the chord lane was off the clock under the steady pulse.** It wrote
  its own `<a b c>` (one element a *cycle*), so since the steady pulse every chord lasted one beat
  and the lane drifted through the piece, contrary to that entry's claim that the chord lane agreed
  bar for bar. It now goes through `strudel::sequence` with every other pattern (`<a@4 b@3 …>`), so a
  chord lasts its bar; a test checks the lane's weights equal `strudel::weights`. This changes the
  chord lane under `rhyme` too (weights only; under `elastic` `rhyme` is byte-identical apart from
  the lane's comment).
- **Tests (hand-worked).** Core: ABAB → `1 I closes B then 8 V turnaround onto 4`; AABB → `I/1 then
  V/8`; ABBA → `I/0 then V/8`; ABAC (unanswered last line) → `IV/3 then V/8`; the mixed fixture under
  cadence and both (resting first halves); AAAO; a one-line stanza (no split, `IV/3` whole bar; `both`
  rests); spans (`dA dO tA tO tO`); slot onsets (`Start`/`Half`) stated, in the outro and looping
  under a flourish. W2 on ABAB: roots `1 1 8 8` in the last bar, steps `0 2 2 1`. Explain rows, incl.
  the resting half. Strudel: ABBA four-beat `[[48,51,55] [50,53,56]]@4`, weights `4 ×8`, elastic
  `[[…] […]]`, rhyme unsplit; `[~ [50,53,56]]@1` for the resting half; the existing chord tests now
  carry `@1`. OSC: ABAB cadence 10 chords, 2 turnarounds. CLI: the couplet `2:I/0+V/8 4:I/1+V/9`, the
  explain row, OSC `12 chords …, 4 of them turnarounds`. Scorer: AABB pitched 24/40 (rhyme) and 26/42
  (cadence). Properties: the I2 harmony shape now includes the split; the W2 I4 property checks each
  note's root against `chord_at`.
- **Snapshots: none changed** — no corpus snapshot has the harmony on. Every example's pattern,
  OSC and explain under `harmony=off` is byte-identical to before.
- **Measured** (`./scripts/cycle.sh <profile> harmony=<variant>`). Off: machine 0.743, jazz 0.737,
  every row identical. Before → after: machine cadence 0.775 → **0.778**, both 0.763 → 0.763; jazz
  cadence 0.730 → 0.730, both 0.733 → 0.733. The extra onset raises layering and pitched slightly
  (Tyger cadence layering 3.837 → 3.900); legibility cannot move (a chord is no event).

`machine`, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady  cross |  inter  legib overall
blake-tyger             3.900  0.949  0.701  2.063  0.017  0.994  1.000      - |  0.932  0.811  0.756
carroll-jabberwocky     2.915  0.911  0.559  1.651  0.600  0.887  1.000      - |  0.826  0.957  0.790
dickinson-because       2.761  0.907  0.701  1.573  0.362  0.887  1.000      - |  0.936  0.831  0.777
hopkins-pied-beauty     3.305  0.829  0.468  1.745  0.163  0.953  0.933      - |  0.955  0.946  0.904
poe-annabel-lee         3.950  0.911  0.514  1.847  0.154  0.841  0.811      - |  0.876  0.914  0.801
preseren-zdravljica     2.417  0.791  0.735  1.542  0.214  0.643  1.000      - |  0.561  1.000  0.561
shakespeare-sonnet-18   2.345  0.818  0.766  1.854  0.364  0.993  1.000      - |  0.821  1.000  0.821
whitman-song-of-myself  2.797  0.879  0.339  1.546  0.150  0.894  1.000      - |  0.948  0.837  0.794
wilde-theocritus        3.059  0.913  0.738  2.099  0.341  0.944  0.844      - |  0.971  0.820  0.796
mean overall 0.778 over 9 poems
```
`machine`, harmony=both:
```
poem                    layer  recur  pitch   entr   sync  pulse steady  cross |  inter  legib overall
blake-tyger             3.900  0.949  0.701  2.063  0.017  0.994  1.000      - |  0.932  0.811  0.756
carroll-jabberwocky     2.761  0.903  0.540  1.651  0.600  0.887  1.000      - |  0.809  0.957  0.774
dickinson-because       2.458  0.879  0.670  1.573  0.362  0.887  1.000      - |  0.903  0.831  0.750
hopkins-pied-beauty     3.305  0.829  0.468  1.745  0.163  0.953  0.933      - |  0.955  0.946  0.904
poe-annabel-lee         3.906  0.908  0.511  1.847  0.154  0.841  0.811      - |  0.876  0.914  0.801
preseren-zdravljica     2.139  0.743  0.705  1.542  0.214  0.643  1.000      - |  0.521  1.000  0.521
shakespeare-sonnet-18   2.288  0.800  0.761  1.854  0.364  0.993  1.000      - |  0.793  1.000  0.793
whitman-song-of-myself  2.706  0.869  0.324  1.546  0.150  0.894  1.000      - |  0.926  0.837  0.775
wilde-theocritus        3.042  0.911  0.737  2.099  0.341  0.944  0.844      - |  0.972  0.820  0.797
mean overall 0.763 over 9 poems
```
`jazz`, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady  cross |  inter  legib overall
blake-tyger             3.900  0.949  0.701  2.063  0.017  0.994  1.000  0.031 |  0.726  0.811  0.589
carroll-jabberwocky     2.915  0.911  0.559  1.651  0.600  0.887  1.000  0.144 |  0.881  0.957  0.843
dickinson-because       2.761  0.907  0.701  1.573  0.362  0.887  1.000  0.013 |  0.785  0.831  0.652
hopkins-pied-beauty     3.305  0.829  0.468  1.745  0.163  0.953  0.933  0.279 |  0.845  0.946  0.800
poe-annabel-lee         3.950  0.911  0.514  1.847  0.154  0.841  0.811  0.093 |  0.797  0.914  0.729
preseren-zdravljica     2.417  0.791  0.735  1.542  0.214  0.643  1.000  0.000 |  0.710  1.000  0.710
shakespeare-sonnet-18   2.345  0.818  0.766  1.854  0.364  0.993  1.000  0.021 |  0.880  1.000  0.880
whitman-song-of-myself  2.797  0.879  0.339  1.546  0.150  0.894  1.000  0.119 |  0.765  0.837  0.640
wilde-theocritus        3.059  0.913  0.738  2.099  0.341  0.944  0.844  0.302 |  0.885  0.820  0.725
mean overall 0.730 over 9 poems
```
`jazz`, harmony=both:
```
poem                    layer  recur  pitch   entr   sync  pulse steady  cross |  inter  legib overall
blake-tyger             3.900  0.949  0.701  2.063  0.017  0.994  1.000  0.031 |  0.726  0.811  0.589
carroll-jabberwocky     2.761  0.903  0.540  1.651  0.600  0.887  1.000  0.144 |  0.889  0.957  0.850
dickinson-because       2.458  0.879  0.670  1.573  0.362  0.887  1.000  0.013 |  0.808  0.831  0.671
hopkins-pied-beauty     3.305  0.829  0.468  1.745  0.163  0.953  0.933  0.279 |  0.845  0.946  0.800
poe-annabel-lee         3.906  0.908  0.511  1.847  0.154  0.841  0.811  0.093 |  0.800  0.914  0.731
preseren-zdravljica     2.139  0.743  0.705  1.542  0.214  0.643  1.000  0.000 |  0.710  1.000  0.710
shakespeare-sonnet-18   2.288  0.800  0.761  1.854  0.364  0.993  1.000  0.021 |  0.880  1.000  0.880
whitman-song-of-myself  2.706  0.869  0.324  1.546  0.150  0.894  1.000  0.119 |  0.768  0.837  0.643
wilde-theocritus        3.042  0.911  0.737  2.099  0.341  0.944  0.844  0.302 |  0.886  0.820  0.726
mean overall 0.733 over 9 poems
```

- **In the browser** (Chromium, port 4914, the page's Tyger preset, B harmony=cadence): with B's
  default `pulse=elastic` the scheduler's chord lane struck bar 3's IV/3 at 4.219 s and V/8 at
  4.941 s (the half, 4.922 s, swung by `swingBy(0.08, 3)`); with B `pulse=steady`, IV/3 at 5.156 s and
  V/8 at 6.094 s, exactly half of the 1.875 s bar, chords lasting 1.875, 1.406 and 0.938 s (a 4-beat
  bar, a 3-beat bar, a half); a drum onset at each of the first 8 bar starts; 86 trace onsets in 8
  bars, every one a music onset; the roll showed the split bar's two chords and the poem lit as it
  played. No console errors or warnings. Nobody has heard it.
- **Raised, not done.** A stanza that opens on an open rhyme starts on V of its degree *d*, so its
  turnaround is V of V, *d*+8, which folds onto *d*+1 in a seven-note scale: whenever the last line
  closes the class one degree above the first's, the bar re-strikes the same triad at its half
  (ABAB ending I/1: `4:I/1+V/8`; in The Tyger `8:I/4+V/11`, `16:I/7+V/14`, `20:I/9+V/16`). ABBA,
  ending I/0, does turn. Whether the turnaround should lead onto the stanza's first
  *tonic*, or on a beat rather than the exact half in odd bars, is the user's call.

### Softening function words — a variant, full kept

The user decided: `CMUdict` gives one-syllable function words primary stress, so §4.2's "stress →
accent" strikes "or" in The Tyger's "hand or eye" like "hand"; today's behaviour stays the default
and a **light** variant softens them where the metre does not need them. A new selector,
`Mapping::Function` (`function-words`): **`full`** (default, first — what preceded it; no `off`)
and **`light`**.

- **The rule** (`prosody_core::accent`). Under `light`, a syllable is struck at
  `Velocity::SECONDARY` (2/3) when its word is a function word (the lexicon says), has one syllable,
  carries primary stress, and is *not* on a strong position of the meter's own scansion: its foot's
  template stressed there (read from `Event::foot` and `Event::subdivision`, nothing re-scanned).
  The spondee is read further, because the meter makes one exactly where a function word's
  dictionary stress meets a content word: its strong position is the first in a falling line (more
  trochees/dactyls than iambs/anapests), the second in a rising one, and in a level line the content
  word's (two function words: the one on the beat). Hand-worked: "What immortal hand or eye" scans
  trochee, trochee, spondee, single — falling — so "or" is light and "What" (a trochee's head)
  stays; "In the forests of the night" scans anapest, iamb, iamb, and "of" (an iamb's strong half)
  stays; "Did he who made the Lamb make thee" scans spondee, spondee, iamb, spondee — rising — so
  "Did" and "who" are light and the rhyme word "thee" stays. The direction rule was added after the
  first version (spondee → content word always) lightened Blake's rhyme word "thee" in that line.
- **Where the list lives.** `Lexicon::is_function_word` — a *required* method on the core's
  language-agnostic `Lexicon` trait, so a second language must answer it — with the English closed
  class in `prosody-lexicon/src/function.rs`, beside the frequency table: word class is language
  data (§9), and the lexicon is already the one English-word table every caller passes in, so no
  call site of the pipeline changed. A fixed sorted list (binary search, a test checks the order):
  articles, pronouns incl. thou/thee/thy/thine, possessives, conjunctions, short prepositions,
  auxiliaries and their archaic forms. Left out on purpose: negatives, quantifiers, `so`/`yet`/
  `when`/`where`/`how`/`while`, `up`/`down`/`out`/`off`, `there`, `art` (Tyger's "what art").
  The surprisal pass, which already asks the lexicon about each word, records the answer
  (`WordSurprisal::is_function_word`); it plays no part in surprisal.
- **Every consumer.** Decided once in the lowering, right after the rhythm and before the bass, on
  `Event::velocity` itself: Strudel gains, OSC velocities, the scorer's accents, the height bass
  (primary syllables only — a light "or" loses its bass note) and W2's strongest syllable all read
  it. `Event::is_light` marks the event; `Score::function` and `Score::lightened` name and count.
- **Visible.** Pattern header `function-words = light: … in N syllable(s)`; OSC
  `# function-words = light: …`; the explain view's `function-words light · … N syllable(s)` line and
  `function-words` in the *from* column of each light row; the CLI help; the page's mapping table
  (from `Mapping::ALL`, unchanged code). Under the default nothing new is printed, so **no snapshot
  changed** (the insta suites passed untouched; no `.snap.new`).
- **Tests (hand-worked).** Core `tests/accent.rs` (a table phonemiser with `CMUdict`'s stresses):
  the three lines above, full vs light, with their scansions asserted first; a level line ("hand or
  eye": "or" light; "or thy hand": "thy" light, "or" keeps the beat); a two-syllable "function word"
  left alone; the height bass losing "or"'s note and nothing else moving; the explain view; a
  property under every other mapping choice that only force changes and only on lightened
  one-syllable function words off a non-spondee strong position; an I2 property (an edit to stanza 1
  leaves the last stanza's forces alone). Mutation-checked: removing the one-syllable clause, the
  spondee rule, the direction or the strong-position test each fails a test. Lexicon: list order,
  members, omissions, case/apostrophe matching. Emit (real `CMUdict`): "or"'s gain 1.000 → 0.667 in
  both passes of the one-line poem, the bass lane `[36 36]` → `[36 ~]`, every other pattern line
  identical; OSC: exactly the two "or" messages change, velocity 1.000 → 0.667, degree 36 → −1.
  Scorer: `dAda dAda dA dO da dA` syncopation 0.25 → 0, pitched 5/8 → 1/2, pulse/steadiness/
  cross-rhythm/layering/recurrence equal. CLI: the counts agree across header, OSC and explain;
  `--without function-words` is the default; `function-words=off` refused; `node --check` passes.
  Web: the selector, `/pattern` and `/explain` agree, the score's syncopation moves. `check-wasm.sh`
  gained two `function-words:light` settings: 495 answers byte-identical. The invariants' lexicon
  now calls words of ≤ 2 letters and "and"/"the" function words, so the I1/I4 properties under any
  mappings exercise `light`; the stack property's `pick` range grew to cover the doubled space (6144).
- **Measured** (`./scripts/cycle.sh`, mean overall). Default: machine 0.743, jazz 0.737 — every row
  byte-identical to before. `function-words=light`: machine **0.710**, jazz **0.747**. Syllables
  lightened: Tyger 24, Sonnet 24, Whitman 18, Dickinson 15, Poe 13, Hopkins 9, Jabberwocky 7,
  Wilde 4, Prešeren 0. Syncopation rises almost everywhere (Dickinson 0.362 → 0.694, Sonnet 0.364 →
  0.657, Jabberwocky 0.600 → 0.738, Whitman 0.150 → 0.331, Tyger 0.017 → 0.020): a light function
  word *on a beat* (a rising line's spondee head, "Could", "Did") leaves that beat unaccented after
  the previous offbeat accent, which is exactly what LHL counts. Machine's band (0.1–0.4) punishes
  that, jazz's (0.45–0.9) rewards it — the Sonnet 0.880 → 0.926 under jazz, 0.733 → 0.636 under
  machine. Pitched falls with the lost bass notes (Tyger 0.681 → 0.570). Legibility unchanged
  (Tyger 116 of 143 explained). The scorer reads it, as it should; whether it sounds better is the
  user's call.
- **In the browser** (Chromium, the page's Tyger preset — its first two stanzas — A `full`, B
  `light`, every other B mapping set to A's): B's header counts 6 (bars 0–7: "or" ×2, "Could",
  "thy", "thine", "what" — the same rows the CLI marks); evaluating each side's pattern, "or" at bar 2
  beat 2½ is `bd` gain 1 plus a bass note on A and `bd` gain 0.667 with no bass note on B; on both
  sides every trace onset in a 6 s window was a music onset on the scheduler's clock (22 of 22); the
  lit poem followed the lines in order; no console errors or warnings.
- **Raised, not done.** Secondary (2/3) or unstressed (1/3) for a light word; whether a spondee's
  direction should come from the stanza rather than the line; phrasal stress beyond function words
  (§9's "content" noun/adjective) is still not attempted.

### Straightening stacked lines — a variant, crossing kept

The user decided: where a line stacked under the bar's host divides a beat incompatibly with it
(three against two — the cross-rhythm traced in *the off-rhythm feel* below), today's crossing stays
the default, and a **straight** variant fits the stacked line to the host. A new selector,
`Mapping::Stack` (`stack`): **`crossing`** (default, first — what preceded it; no `off`) and
**`straight`**.

- **The rule** (`prosody_core::stack`). Per beat, a line's grid is the lcm of its onsets' reduced
  denominators — exactly what `cross_rhythm` reads — and two grids cross when neither divides the
  other. Where a stacked, on-grid line crosses the host in a beat, its *n* syllables are fitted to
  the host's *h* parts, "the first takes the long": *n* > *h* — the first *h* − 1 take a part each
  and the rest share the last part (three over two: `[a@2 b c]`, onsets 0, ½, ¾); *n* < *h* — the
  first holds the parts the line lacks (two over three: `[a@2 b]`, 0 and ⅔). Every syllable stays
  its own event, in the same beat, traced to the same word. Left alone: the host; beats where the
  host is silent; placements off the slot's grid (a squeezed or stretched loop); two stacked lines
  crossing each other but not the host. `fit` is tested over every *n* ≤ 9, *h* ≤ 7: distinct
  ordered onsets filling the beat, never crossing *h*.
- **Where it lives: the core, not an emitter.** `Score::placed_in(slot, placement)` is
  `Score::placed` with each event's place in its beat *in that slot* (`Placed::subdivision`,
  `Placed::onsets`, `Placed::straightened`); the Strudel walk (`grid_by_slot`, so the page's trace
  too), the OSC schedule and the scorer's timeline all read it, so the four agree by construction
  and the scorer measures what is played. An emitter-side fit would have left the scorer counting
  crossings no one hears. `Score::events` is unchanged; the variant is recorded (`Score::stack`).
  `Subdivision` gained `parts` (1 everywhere but a fitted long syllable), written in the pattern
  with `@` inside the beat.
- **Checked against the real packages.** With `@strudel/core` and `@strudel/mini` 1.2.6 (the
  versions `@strudel/web` 1.3.0 pins) in node, `[[bd@2 bd bd] [bd bd]]@2` as slot 1 sounds at 2,
  2.5, 2.75, 3, 3.5 cycles. Wilde's whole pattern, evaluated the same way, sounds 331 distinct
  onset instants in one pass under `crossing` and 293 under `straight` — in both cases exactly the
  OSC schedule's instants (38 fewer: each 3-into-2 beat trades ⅓ and ⅔ for ¾).
- **Visible.** The pattern header's `stack = straight: … in 38 beat(s) — three syllables over two
  are [a@2 b c], two over three [a@2 b]`; the OSC schedule's `# stack = straight: …`; the explain
  view's `stack straight · … 38 beat(s)` line and, under each fitted line, `straight 2 into 3 at
  4.0 · 3 into 2 at 3.1 3.2 4.1 … (slot.beat)`; the CLI help; the page's mapping table, which
  lists it unchanged from `Mapping::ALL` (B starts at `crossing`, its first variant). Under the
  default nothing new is printed — the precedent of the height bass — so **no snapshot changed**.
- **Tests (hand-worked).** Core (`tests/stack.rs`, toy `a`/`A`/`o`/`O`): ABAB with an opening
  anapaest — slot 1's held A line is `(0/3 1/3 2/3)` crossing and `(0/4 ×2, 2/4, 3/4)` straight in
  beat 0, beat 1 untouched, the host and every other slot identical, `Score::events` and the
  arrangement identical; a three-foot A line fitted with the stanza period (bar of 3) and left alone
  without it (squeezed into 2); the explain view's two new lines and nothing else; and a property
  over generated poems of two- and three-syllable feet, rhymes, refrains and commas under any
  mappings — events/voices/arrangement/harmony unchanged, same events per placement, host never
  moved, unfitted beats as crossing, distinct instants, and no on-grid stacked line crossing its
  host (mutation-checked: it fails with the fit disabled, and with two syllables sharing an onset).
  Emit: the pattern's `[[x x x] [x x]]` → `[[x@2 x x] [x x]]` in the held line's slot, every
  parameter on that grid, only that line and the bass's second lane changed, the trace's lanes the
  music's rhythm slot by slot; OSC 2, 2⅓, 2½, 2⅔, 3, 3½ → 2, 2½, 2¾, 3, 3½. Scorer: the fixture's
  `cross_rhythm` 1/16 → 0, pulse/steadiness/layering/pitched/recurrence equal. CLI: Wilde's header
  and OSC count 38, `cross_rhythm` 0.302 → 0.000, `--without stack` is the default, `stack=off`
  refused, the pattern passes `node --check`. Web: the selector, `/pattern` and `/trace` carry the
  same 38 fitted long syllables, `/score` under jazz reads 0.000, `/explain` names it.
  `check-wasm.sh` gained two settings with `stack:straight`.
- **Measured** (`./scripts/cycle.sh`, mean overall). Default: machine 0.743, jazz 0.737 — both
  unchanged, every row identical. `stack=straight`: machine 0.743 (`cross_rhythm` is not in it, and
  no other column moved at three places), jazz **0.705**: cross-rhythm falls to 0.000 on every
  example but Poe (0.005: three slots where a six-beat line is squeezed into a five-beat bar, which
  straight leaves alone), so it drops below the jazz band's 0.1 floor everywhere and interest falls
  (Wilde 0.903 → 0.840, Hopkins 0.845 → 0.786, Jabberwocky 0.900 → 0.838). Beats fitted: Wilde 38,
  Hopkins 24, Jabberwocky 23, Poe 19, Whitman 19, Tyger 11, Sonnet 3, Dickinson 2, Prešeren 0. The
  jazz profile rewards some cross-rhythm, so this is the number moving the way the profile says,
  not a verdict; whether the ear prefers it is the user's call.
- **Disclosed against I2** (`SPEC.md` §4.2): under `straight` a stacked line's onsets within a beat
  depend on its host's feet, which can be another stanza's. Events, beats, voices, seeds, accents
  are untouched, and every I2 property compares those.
- **Review fixes.** (1) The one-word edit test (`prosody_score::sensitivity`) built each line from
  `Score::events` alone, so under `straight` it compared timing nobody plays: an edit to a host that
  re-fitted another stanza's stacked line still counted as local. Each slot a line sounds in now
  carries its onsets from `Score::placed_in`, and a slot hosted by the edited stanza may change them
  as it may change its length. Under `crossing` those onsets are each event's own, so every number
  is as it was. Under `straight` *local* falls — Wilde 29 → 18 of 61, Jabberwocky 46 → 28 of 69, Poe
  83 → 51 of 116, Tyger 68 → 50 of 143 — and *explained* does not move anywhere (the re-fitted line
  sounds under the edited one), so the `legib` column is unchanged. Hand-worked in
  `prosody-score/tests/pipeline.rs`: one edit (`so` → `sasa`) turns stanza 2's host beat into a
  triplet, so stanza 1's looped triplet is no longer fitted — 9 local under crossing, 8 under
  straight, 9 explained in both (mutation-checked: it fails with the scorer reading the events' own
  subdivisions). (2) The explain view summed fitted beats per line while the pattern and OSC header
  counted each `(slot, beat)` once; both now print `Score::straightened_beats`. ABCABC fixture in
  `tests/stack.rs`: slot 2 fits two held lines in beat 0 and slot 3 one — 2 beats, not 3.
- **Raised, not done.** Two stacked lines crossing each other where neither crosses the host are
  left crossing (fitting them to each other would need a choice of which one leads); a squeezed or
  stretched loop is not fitted (its beats are not the host's); the "first takes the long" rule
  ignores stress — an anapaest's stressed third syllable lands on ¾, off the host's onset, where
  a stress-led rule would put it on ½.

### A steady beat — the pulse, decided

The user decided the Strudel pattern's bar lasts as long as its beats, so the beat is one speed
throughout; the old behaviour stays as a variant to A/B. A new selector, `Mapping::Pulse`
(`pulse`): **`steady`** (default) and **`elastic`** (as it was). It is structure, recorded on the
score (`Score::pulse`) and laid out by the backends; the lowering places nothing by it.

- **Strudel.** Under `steady` a cycle is a beat: one `<…>` per pattern whose bars carry `@` weights
  equal to the slot's beats, and `.cpm(tempo)`. Checked against the `@strudel/mini` 1.2.6 source:
  `<…>` is a `polymeter_slowcat`, its sequence a `timeCat` of the weighted children slowed by the
  weights' sum, so a bar of weight *w* lasts *w* cycles (`@n` adds `n − 1` to a default weight of
  1). One function, `strudel::sequence`, writes every pattern's bars, so the drums, every parameter,
  the pitched layers, the chord lane and the trace agree bar for bar; `strudel::weights`/`starts`
  are the bars' lengths and starts for everything else. Under `elastic` every byte is as before.
- **Swing.** `swingBy(x, n)` is `inside(n, late(seq(0, x/2)))` (`@strudel/core` 1.2.6): it slices a
  cycle. A cycle being a beat, `steady` writes `.swingBy(x, 1)` — every beat swung in bars of three
  and four alike. Measured with the real 1.2.6 packages in node on Theocritus at swing 0.33: every
  half-beat onset lands at 0.665 of its beat and every second triplet at 0.832, 331 onsets as
  without swing. Under `elastic` (`swingBy(0.33, 3)`) the four-beat bars land at 0.125, 0.25, 0.375,
  0.54, 0.79, 0.915 of a beat, and Strudel sounds 338 onsets — seven doubled across slice edges.
- **On one clock.** The trace carries the music's bar starts and `.cpm` (`Trace::bars`,
  `Trace::cycles_per_minute`, and `"bars"`/`"cpm"` in `/trace`'s JSON); the page's roll draws its
  bar lines at those starts, repeated each loop, and sizes its window from the mean bar. The
  reading's cue pattern is weighted like the music's bars, and each line's rate is fitted to the
  bar it is spoken in (under `elastic` an eight-syllable line in a four-beat bar is hurried to
  1.569; under `steady` it reads at 1.176 like the rest). The OSC schedule was always steady; asked
  for `elastic` it says `# not applied here: pulse = elastic …` (and so does `--send`).
- **The pulse has no `off`.** A piece cannot be without one, so its first variant is what preceded
  it, `elastic`; `Mapping::off` now returns a mapping's first variant (`off` for every other one),
  `--without pulse` / `off=pulse` mean `elastic`, and the page's B side starts each mapping at its
  first variant — so A is steady and B elastic by default.
- **Visible.** The pattern header's `pulse = steady: a cycle is a beat at 121 bpm, … (bars of 3, 4
  beats)` or `pulse = elastic: … 3 beats at 121.000 bpm, 4 beats at 161.333 bpm`; the explain view's
  `pulse …` line under the passes; the CLI help; the page's mapping table.
- **Tests (hand-worked).** A two-stanza fixture of three-iamb then four-iamb lines: slot weights
  3 3 4 3 4 4 4 4 and starts 0 3 6 10 13 17 21 25 (29) under `steady`, at `.cpm(120.000)`; all ones
  at `.cpm(40.000)` under `elastic`, the same bar strings otherwise; swing slices 1 and 3; the
  trace's bars, clock and lane weights are the music's, the cues' `<0@3 1@3 2@4 ~@3 ~@4 3@4 ~@4 ~@4>`
  and rates (1.176 ×4 steady; 1.176, 1.176, 1.569, 1.569 elastic); OSC onsets on beats 0 3 6 10 13
  17 21 25 under either. The web trace JSON for Theocritus (126 beats in bars of 3 and 4 against 0
  to 33), the CLI's `--set`/`--without pulse`, and the selector's variants.
- **Snapshots.** All 15 `prosody-emit` corpus snapshots changed, and only mechanically: every
  pattern gained the `pulse = steady` header line, `@n` weights on each bar, and `.cpm` became the
  tempo (16 → 128, 30.25 → 121, 25 → 125, 25.2 → 126) with the villanelle-jazz `.swingBy(0.330, 5)`
  → `(0.330, 1)`; every view gained the one `pulse steady …` line. No corpus poem mixes bar
  lengths, so in every snapshot each bar lasts exactly what it did (checked: with weights, the
  header line and the clock lines stripped, every snapshot is byte-identical to before). Of the
  examples only Hopkins (bars of 5 and 6), Poe (5, 6) and Wilde (3, 4) change in time; with
  `pulse=elastic` every example's pattern is byte-identical to before apart from the header line,
  and every OSC schedule is unchanged.
- **Measured.** `./scripts/cycle.sh machine` 0.743 and `jazz` 0.737, unchanged, and the same under
  `pulse=elastic`: the scorer reads the score, which already kept a steady beat, so it cannot tell
  the two apart (raised below). `check-wasm.sh` gained a `pulse:elastic` setting: 315 answers.
- **In the browser** (Chromium, Wilde, A steady / B elastic): over 60 samples on each side the
  trace's active syllables equalled the music's, every bar start carried a drum onset at the
  trace's bar time (33 of 33 on A, 30 of 30 on B), cues fired at 0, 1.488, 2.975, 4.463, 11.405 s
  — the steady bar starts — and the roll's bar lines followed the 3- and 4-beat bars. No console
  errors or warnings.
- **Raised, not done.** The scorer could model the elastic pulse (a slot as the mean bar) so the
  A/B score shows a difference; the roll's "bars shown" is a mean bar under `steady`.

### Watching it play — the roll and the lit poem

A **Watch** tab, the page's new default, shows the piece while it plays. No mapping changed and no
event moved; the pattern text gained one display-only call per layer.

- **The roll** is Strudel's own (`@strudel/draw` 1.2.6, the release matching the page's Strudel
  core), loaded after Strudel starts so its `@strudel/core` imports resolve to the playing Strudel
  through the import map, as the soundfonts' do (`strudel-globals.mjs` now also exports `State`,
  `TimeSpan`, `getTime`, `isPattern`). Its `Drawer` runs on the page's scheduler; `drawPianoroll`
  paints onto the tab's own canvas, two seconds either side of the playhead, rows folded to the
  sounds in use. Notes are coloured by layer through `.color("#…")`, which the emitter now writes on
  every layer (`strudel::Colour`) — read by Strudel's visualisations, never by its sound. Every
  pattern snapshot changed by exactly those calls and nothing else (checked by stripping them).
- **The lit poem** comes from a new artefact, the trace (`prosody_emit::trace`, `POST /trace`): one
  silent pattern lane per lane of the music's rhythm layers, built by the music's own
  `grid_by_slot` walk with an event id where the music has a sample, closed by the music's own swing
  and tempo (`strudel::clock`, now shared). So a trace event begins where its sound does by
  construction — the emitter's history has one bug from walking bars twice. Each id is a mark: the
  syllable's span, its layer colour, whether its line is new in that slot (stated, arriving,
  overruling) or looping, and whether it also plays a rhyme or bass note. The page queries the trace
  at the drawer's time each frame and lights the matching pieces of the poem; the roll and the verse
  share one clock and cannot disagree.
- **Positions are the text box's.** The source the server compiles is the controls' header plus the
  poem with any pasted header removed, so `/trace` moves each span back onto the text as typed and
  counts in UTF-16 units, as JavaScript does — tested with a pasted header containing an em dash.
- The verse is set in Spectral (Google Fonts; Georgia otherwise); a long poem scrolls to keep the
  line being heard anew in view, smoothly unless the system asks for reduced motion. The writing
  column now scrolls as a whole, so the controls no longer squeeze the poem to one line.
- Verified in Chromium on the seed, Wilde (jazz palette, harmony and walking bass on B, switched
  while playing) and Poe (scrolling): no console errors; syllables light fresh and looping as the
  arrangement says; Stop clears it. `check-wasm.sh` now compares `/trace` too.
- Nobody has watched it with sound on. Humanise's few milliseconds of stray are not in the trace,
  so a lit syllable can lead its sound by that much.
- **Then, at the user's request, the layout.** The poem's text box is now the lit verse: a
  transparent `<textarea>` over the verse it lights, both laid out identically (font, padding,
  wrapping, scrollbar gutter) and scrolled together, so typing, the caret and selection are the
  text box's own while the colour beneath shows what sounds. Marks apply only while the text box
  says exactly what they were compiled from; mid-edit the verse is plain until the recompile. The
  roll shows a set number of bars (2, 4, 8 or 16; a bar is a poem line, `60 / cpm` on the
  scheduler) with bar lines, instead of two seconds either side. Roll, palette, mappings, reading
  and the pattern/annotation/score views are folding `<details>` panels in the right column,
  remembered per browser; the poem has the left column to itself. Checked in Chromium at 1440 and
  390 px: the two layers measure the same, a selection sits on its letters, an edit while playing
  recompiles and lights again, folding persists, no console errors.
- **The reading, shown apart from the music.** The music has the letters (colour by layer,
  underline for a note); the reading now has the background: a pale wash behind the line being
  read, a brighter one behind the word being said, driven by the utterance's `boundary` events
  (voices that send none show the line only). `/reading`'s cues carry `from`/`to` in the text box's
  UTF-16 units (one `text_box` mapping now serves the trace and the reading), and the verse is cut
  at those edges too. A cancelled utterance's late `end` cannot clear the line that replaced it.
  Checked in headless Chromium, which has a voice: the wash followed "Burnt the fire of thine
  eyes?" word by word and cleared at the line's end; Stop clears everything.

### The page without a server — the compiler in WebAssembly

The page now also builds as static files that compile in the browser (`scripts/build-site.sh`).
No mapping and no emitted byte changed.

- **The C parser builds for `wasm32-unknown-unknown`.** The generated `parser.c` includes
  `stdlib.h`, which that target lacks; `tree-sitter-language` ships the minimal headers the runtime
  builds against there and passes their path to its direct dependents, so the grammar's build
  script includes them on a wasm target (five lines). The link then needs `llvm-ar`: macOS's `ar`
  writes archives whose wasm objects the linker cannot see, and every `ts_*` symbol comes back
  undefined. `rust-toolchain.toml` now asks for `llvm-tools` and the wasm target.
- **`prosody-web` is a library and a binary.** The library (`Site`, `Request`, `Response`) is the
  page and the answer to every request, with no IO; the binary is the localhost server around it,
  plus `--site DIR` to write the static page. `Request`/`Response` and the query decoding moved out
  of the server into `exchange.rs`, so a request can be built from a target string anywhere.
- **`prosody-wasm`** binds that library to the page with `wasm-bindgen` (pinned `=0.2.129`; the
  user chose it over hand-written pointer-passing exports, which needed `unsafe` and a second
  lint exemption). One function, `respond(target, body) -> Answer { status, body }`. The crate
  keeps the workspace lints, `forbid(unsafe_code)` included, on both targets. The generator CLI
  must match the crate exactly, so `build-site.sh` installs that version into `target/tools`
  rather than touching whatever `wasm-bindgen` is on PATH. New crates: the `wasm-bindgen` family,
  `bumpalo`, `once_cell`.
- **The page has one transport.** Every request goes through `ask(path, query, body)`, which is a
  `fetch` when served and a call into the compiler when static; both return a `Response`, so
  nothing else in the page changed. The import map points at `./strudel-globals.mjs`, relative, so
  the static page works under a subdirectory of a host.
- **I1 across targets is checked, not assumed.** `scripts/check-wasm.sh` asks the wasm build and the
  native server for all four views of all nine examples under six settings (palettes, swing,
  humanise, density, every mapping's variants, all three profiles) — 216 answers, byte-identical,
  scores included. A deliberately wrong palette on the wasm side fails it at the first differing
  byte. CI runs it.
- Measured: `prosody_bg.wasm` is 5.1 MB, 1.2 MB gzipped, 0.97 MB brotli; almost all of it is the
  3.6 MB dictionary. The first compile in the page takes about 100 ms (the dictionary loads), later
  ones well under a millisecond for a short poem. In Chromium from `python3 -m http.server`: the
  page compiles, plays Blake with the reading on Strudel's clock, scores (overall 0.757, the cycle
  table's), with no console errors and every request 200.
- Not done: no hosting is set up — that publishes the page, and is the user's call. The dictionary
  could be packed far smaller (a sorted, prefix-shared table) if the download matters.

### The off-rhythm feel — a trace, and cross-rhythm in the jazz profile

The user heard off-rhythm beats; the scorer's syncopation gave The Tyger 0.017. Traced on the
score (every slot's placements, each line's per-beat subdivision) and on the emitted pattern text,
before any metric. No mapping changed.

**What puts onsets off the shared beat.**

- *Lines with different feet in one bar (cross-rhythm, 3 against 2).* §4.2 divides a beat by its
  foot's syllables, and the held, refrain and call lanes stack lines, so a three-syllable foot can
  sound against a two-syllable one. **Theocritus**, 38 of 126 beats: its dactylic line-ends —
  "Dost thou remember Sicily" (bytes 52..77, `[bd bd] [bd bd bd] [bd bd bd]`) looping in the
  refrain lane, "O Singer of Persephone" (147..169) likewise — against the iambic tetrameter of the
  host: slots 3, 6 and 9 ("Still through the ivy flits the bee", 80..115, `[bd bd]` ×4) cross in
  beats 1 and 2; slots 12–19 ("Simaetha calls on Hecate", 172..196, and "And hears the wild dogs")
  cross in beat 2 against "Persephone"'s triplet; slot 1's "In the dim" (24..34) anapaest against
  the refrain "O singer" in beat 0. **The Tyger**, 11 of 352 beats: only the anapaest "In the
  for-" of "In the forests of the night" (187..201 and 847..861; `[bd bd bd]`, its accent on the
  third triplet) in beat 0, against the call lane's "Tyger Tyger," (158..170, `[bd bd]`) in slots 1,
  2 and 5, and in slots 73–85 against the call "And what shoulder" (407..424) and the refrain "What
  immortal hand or eye" (216..241).
- *The elastic beat, in the Strudel backend only.* A slot is one cycle, and the whole piece has one
  `.cpm` — tempo ÷ the integer mean bar length — so bars of different length play beats of
  different length. Theocritus (126 beats over 33 slots, so mean 3, `.cpm(40.333)`) plays its
  3-beat bars at 121 bpm and its 4-beat bars at 161, switching at slots 3, 4, 6, 8 and 9 — the
  five changes behind its steadiness of 27/32 = 0.844 — and most of the piece is 4-beat bars. The
  Tyger's bars are all four beats, so it has none. The OSC backend and the scorer's timeline keep the beat constant, so
  no metric can see this; `steadiness` (0.844) counts the changes but scores them as meter, not
  tempo. Swing (`.swingBy(x, mean)`) slices by the same mean, so on a mixed-length poem it would
  slice across beats; no example has both today (Tyger's swing 0.08 at `n = 4` is aligned: second
  halves of beats 4% late, which also makes "In the for-"'s triplet uneven).
- *Not off the grid, but felt.* Squeezed loops (`Slot::room_for`, a longer stanza's line under a
  shorter bar) would put every onset off the host's beats; neither poem has one (every placement's room
  is its own length: no line is longer than the bar it sounds in). Lines shorter than the period rest: The Tyger's slot 77 — the 3-foot "In the forests"
  hosting and the 2-beat call — leaves beat 3 empty (pulse 0.994). Humanise (≤ 6 ms, each line its
  own seed) flams lines that coincide by a few milliseconds.
- *Why syncopation reads 0.017.* LHL over the merged accents: some line accents 346 of the 350
  on-beat instants in The Tyger, so an accent on the "and" (254 of its 600 accented instants — "or"
  in "hand or eye", "frame" and "thy" in "Could frame thy", CMUdict's primary stress on monosyllables,
  §9's lexical-stress problem) is always followed by an accented beat, and LHL counts nothing.

**The metric.** `cross_rhythm` (`prosody-score`, jazz profile only): per beat and per line, the line's
subdivision is the lcm of its onsets' denominators within the beat; a beat counts when two lines'
subdivisions do not divide one another. Share of the piece's beats. Band 0.1–0.4, a reasoned guess
(three against two is a device in small-group jazz, heard against a shared ride; no study counts its
share). Hand-worked tests: 3 against 2 crosses and 2 against 4 does not; one line never crosses
itself; a 5-in-4 squeezed loop crosses every beat; a chord is no grid; through the real lowering, an
anapaest held under an iamb is 1 of 16 beats, and only jazz reports it. The Score tab lists metrics
from the report, so it shows the new row unchanged; `scripts/cycle.sh` gained a `cross` column (`-`
outside jazz).

**Measured.** Machine: unchanged, mean 0.743 — the metric is not in that profile (a straight
machine grid has no use for it). Jazz: mean 0.740 → 0.737. Cross-rhythm: Theocritus 0.302, Hopkins
0.279, Jabberwocky 0.144, Whitman 0.119, Poe 0.093, The Tyger 0.031, Sonnet 18 0.021, Dickinson
0.013, Prešeren 0.000. The jazz snapshots each gained the one row; interest moved only through the
geometric mean over eight fits instead of seven.

**Raised, not done** (each would be a switch or variant, `CLAUDE.md` §10): a Strudel bar that lasts
its beats (constant pulse, as OSC already does), with swing sliced per bar; cross-rhythm as a
mapping to choose rather than a by-product (e.g. a held/refrain line fitted to the host's
subdivision); accents of unstressed monosyllables; a per-line syncopation beside the merged one.

### Sound and voice, step 6, second review fix — the drum keeps its syllable's accent

The first fix struck an OSC message carrying a W2 bass note at the foot's accent — but one
`/prosody/note` is both the drum hit and the pitched note, and `examples/prosody.scd` read the same
`gain` for both. Under `bass = feet` a foot's unstressed onset was struck as a full-force drum hit
(the Sonnet: 48 messages changed gain against `bass = off`; The Tyger: 68), an OSC-only source of
off-rhythm accents that the Strudel drums did not share.

- `gain` (argument 3) is the syllable's own velocity again, always (§4.2's stress → accent).
- A new last argument, `noteGain`, carries the pitched note's accent: the bass note's under a walk,
  the event's otherwise. Appended so every earlier argument keeps its position;
  `examples/prosody.scd` strikes `\prosodyPitch` with it.
- Checked against `bass = off` on the Sonnet, The Tyger and Theocritus: 0 messages change their
  (onset, layer, voice, gain) under `height`, `syllables` or `feet`. Test
  `the_drums_keep_their_syllables_accents_whatever_the_bass_walks` holds it on a fixture.

### Sound and voice, step 6, review fixes — one walk a bar

The step-6 review heard the walks stacking: a line held, refrained or called under another kept
walking in its own lane, so the bass instrument played two or three walks at once, and their passing
and approach tones met a semitone apart. Counted on the page's evaluated pattern (cycles 0–64, bass
onsets grouped by start, onsets holding notes a semitone apart): The Tyger, `height` 0 of 192,
`syllables` 59 of 257, `feet` 33 of 136, `feet`+cadence 53 of 136; Theocritus 0, 22, 17, 15. It may
be part of the "off-rhythm" impression too.

- **One walk a bar** (`Score::sounds_bass`, the one place it is decided): under `syllables` and
  `feet`, only the slot's foundation-layer line — the one `Score::chord_in` takes the chord from —
  sounds its bass; a line under it keeps its drums and melody. `height` sounds under every line as
  before (its notes are chord tones, so stacked ones never clash), so the default piece and every
  snapshot are unchanged. The Strudel bass layer, the OSC schedule and the scorer's timeline all
  ask it. Re-counted on the page after the fix: 0 semitone clashes, one bass lane (see the step
  report).
- **OSC strikes a bass note as the bass note**: where a message carries the bass's pitch, its
  velocity is the bass note's (under W2 the foot's stressed syllable's), as the Strudel backend's
  gain is. Its sustain stays the syllable's: `sustain` is the receiver's envelope multiplier
  (`examples/prosody.scd`), not a length in beats.
- **The explain view agrees with itself**: walking, the token table's pitch column shows each
  syllable's own bass note on its own row (`bass 0-` on Tyger's `ger`), and a rhyme word both notes
  (`deg 0~ bass 0@4`); each line's bass block says where it walks and where it is silent
  (`walks in stated 1 · yielding 5(1)` / `silent in refraining 2 — one walk a bar`). `height`
  renders as it did.
- **Tests**: a property that under a walk each slot sounds at most one bass line, and that one its
  foundation-layer whole line (and every line under `height`); hand-worked ABAB fixtures for the
  Strudel lane (one bass lane, `[36 34]` then `[39 38]`, drums still in lane 2), OSC (`de`'s
  messages at 0, 2, 8: `(1.0, 36)`, `(1/3, -1)` held, `(1.0, 36)`), the scorer (pitched 26 of 40
  under `feet`, 20 of 40 under `height`) and the view (walks/silent lines, per-syllable cells). The
  old pitched-share test moved to AABB, where nothing is held. Disabling the rule fails all four
  new rule tests.
- **Measured** (`./scripts/cycle.sh`, means): machine `height` 0.743 (unchanged), `syllables`
  0.667 (was 0.617), `feet` 0.723 (0.740), `feet`+cadence 0.765 (0.777); jazz `height` 0.740
  (unchanged), `syllables` 0.773 (0.739), `feet` 0.754 (0.775), `feet`+cadence 0.753 (0.766).
  The pitched share falls most (Blake under `feet` 0.575 → 0.293), because the held and refrain
  lanes no longer double it; legibility is unchanged everywhere.

### Sound and voice, step 6 — the walking bass, as candidates

The user approved the proposal's W1 and W2 as variants to compare by ear. `Mapping::Bass` is now a
selector — `off`, `height` (default: today's vowel-height bass), `syllables` (W1), `feet` (W2) —
following the proposal table; `SPEC.md` §4.6 has the rules and the choices the table left open.
With `height` nothing changes: no snapshot moved, the machine cycle reads 0.743 and the jazz cycle
0.740, as before.

- **A bass note is its own field** (`prosody_core::bass`, `Event::bass`), no longer a `Pitch` beside
  the melody's. The walks need a note under a rhyme word (W1 plays every syllable; W2's note often
  rides on a rhyme word's foot), and with one pitch per event the melody won. `PitchSource::Vowel`
  is gone. `height` keeps the old rule — no bass on a rhyme word — so its output is byte-identical.
  The pass runs after the melody and the harmony (W2 reads the bar's chord).
- **W1** (`syllables`): every syllable of a foundation line. Stressed (primary) syllables keep their
  height note; the last syllable before a stressed one is a semitone below that note; other
  unstressed syllables between two stressed ones step through the scale from the first towards the
  second, holding one short (the upper neighbour between equal notes). Choices made here: a lead-in
  climbs into the first stressed note and a tail steps down from the last, so every syllable
  sounds; a line with no primary stress has no bass. In iambic or trochaic verse there is one
  unstressed syllable between stresses, so W1 is mostly approach tones (The Tyger's first line:
  `0 0- 0 2- 2 0- 0`).
- **W2** (`feet`): one note a foot, on its first syllable, held for the beat (the Strudel lane writes
  each beat whole), traced to the foot's span, struck with the foot's stressed syllable's accent.
  The stressed vowel's height picks root, third or fifth of the bar's chord (harmony on) or of the
  scale (off, or a bar with no chord). As built, the proposal's direction rule decides only where the
  foot's tone would repeat the walk's last note: it steps up if the next foot's vowel is higher,
  down otherwise. Between different tones the move already goes the way the vowels do, since chord
  tones sit in height order in one octave.
- **Emitter.** A bass note renders as its root (folded into one octave, as the chord lane folds it)
  plus its step, through the scale, a semitone lower for an approach (`Scale::step`, signed). The
  bass lane's comment names the variant, and a walking bass adds one header line (`walking bass
  (mapping bass = feet): …`); `height` adds nothing. OSC sends a bass note as the pitch where there
  is no melody note, and its text says how many bass notes under rhyme words it cannot send
  (`unsent_bass`; zero under `height`).
- **Visible.** `--explain` and the page's explain tab add a `bass` block under each foundation line
  when the bass walks — one row a note: where it sounds, what it traces to, `step 2`, `step 0-` (a
  semitone below) or `2@3` (from chord root 3), and why (`approach: a semitone below the next
  stressed note, 0`; `step down: AH is mid, so the third of IV deg 3 would repeat the last note; the
  next foot's ER is not higher`). The legend lists a `bass §4.6` pass only then.
- **Scorer.** The timeline counts a bass note as pitched. A W2 note is keyed by its step above its
  chord's root, not root + step: the root is the harmony's, which the step-5 review left out of
  pitch entropy (keyed by root + step, feet+cadence read Blake's entropy as 3.876 and the machine
  mean as 0.670 — the same miscount, through the bass). An event with a melody and a bass note is
  one hit carrying the melody's degree. The edit test still treats a melody note and a height-bass
  note on the same degree as one heard pitch, as it did when both were `Pitch` values
  (`a_rhyme_across_stanzas_carries_an_edit_with_it` pins it); every other bass note is compared on
  its own.
- **Tests.** Hand-worked in `crates/prosody-core/tests/bass.rs` (W1 up, down, lead-in and tail,
  equal notes, no stress; W2 steps up and down and below the root, harmony off, rhyme, cadence roots
  4 5 0 8, a chordless bar, flourish, off), `bass::tests` (passing tones), the explain view's rows,
  Strudel MIDI (W1 `39 38 35 36` in C minor; W2 `[36 39 38 36]`, and `[41 38 39 41]` over H1's
  degree-1 chord), OSC pitches and the unsent count, `Scale::step` below the root, and the scorer's
  pitched share (height 0.5, feet 0.75) and unchanged entropy under H1. Properties: I4 (a note traces
  to its syllable or foot, only in a foundation line, W2 from its bar's chord root), each variant
  sounds where its rule says, the bass changes nothing but the bass, I2 for the bass (scoped as
  `SPEC.md` §4.6 discloses; a mutation reading the poem's first bar's chord failed it), I3 for the
  walking bass, and the harmony property now lets W2's chord tones move with the harmony. The I1 and
  invents-nothing properties draw from all 768 combinations. Two more mutations — a W2 note traced to
  its syllable, a W1 walk crossing lines — failed the I4 and rule properties.
- **Checked in Chromium on port 4884**, The Tyger and Theocritus, A = `syllables`, B = `feet` (then
  `feet` with `harmony=cadence`), switching A/B while playing: patterns evaluate and play, no console
  errors or warnings in 28 messages, `gm_acoustic_bass` (`0320_JCLive_sf2_file.js`) loads 200. The
  evaluated W2 lane on The Tyger has one onset a beat, each lasting the beat; W1 one a syllable. No
  one has heard it.

**Measured** (`./scripts/cycle.sh <profile> <choices>`). The default rows are the step-5 tables,
unchanged. Pulse does not move under W2: the drums already strike every beat a foot starts on, so the
proposal's "pulse → 1.0" was already as high as these poems' feet allow. Legibility does not move
under any variant: no walk crosses its line, and the bars a chord-driven change reaches are bars the
rhyme already links. W1 pitches nearly everything (pitched 1.000 on six poems, past both bands);
W2 under cadence is the machine profile's best mean so far (0.777), and W2 alone the jazz profile's
(0.775).

`machine`, bass=height (the default):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.681  2.063  0.017  0.994  1.000 |  0.933  0.811  0.757
carroll-jabberwocky     2.563  0.890  0.513  1.651  0.600  0.887  1.000 |  0.790  0.957  0.755
dickinson-because       2.408  0.883  0.664  1.573  0.362  0.887  1.000 |  0.898  0.831  0.746
hopkins-pied-beauty     3.085  0.800  0.442  1.745  0.163  0.953  0.933 |  0.905  0.946  0.857
poe-annabel-lee         3.700  0.896  0.492  1.847  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     1.972  0.724  0.683  1.542  0.214  0.643  1.000 |  0.503  1.000  0.503
shakespeare-sonnet-18   2.129  0.750  0.745  1.854  0.364  0.993  1.000 |  0.733  1.000  0.733
whitman-song-of-myself  2.615  0.859  0.307  1.546  0.150  0.894  1.000 |  0.904  0.837  0.757
wilde-theocritus        2.689  0.892  0.713  2.099  0.341  0.944  0.844 |  0.950  0.820  0.779
mean overall 0.743 over 9 poems
```
`machine`, bass=syllables (W1):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  1.000  2.728  0.017  0.994  1.000 |  0.720  0.811  0.584
carroll-jabberwocky     2.599  0.890  1.000  2.983  0.600  0.887  1.000 |  0.589  0.957  0.563
dickinson-because       2.465  0.883  1.000  2.725  0.362  0.887  1.000 |  0.698  0.831  0.580
hopkins-pied-beauty     3.110  0.800  0.712  2.680  0.163  0.953  0.933 |  0.834  0.946  0.789
poe-annabel-lee         3.700  0.896  0.765  2.673  0.154  0.841  0.811 |  0.776  0.914  0.709
preseren-zdravljica     2.083  0.724  1.000  2.603  0.214  0.643  1.000 |  0.401  1.000  0.401
shakespeare-sonnet-18   2.137  0.750  1.000  2.624  0.364  0.993  1.000 |  0.597  1.000  0.597
whitman-song-of-myself  2.706  0.859  0.487  2.768  0.150  0.894  1.000 |  0.866  0.837  0.725
wilde-theocritus        2.689  0.892  1.000  2.788  0.341  0.944  0.844 |  0.734  0.820  0.602
mean overall 0.617 over 9 poems
```
`machine`, bass=feet (W2):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.575  2.584  0.017  0.994  1.000 |  0.881  0.811  0.715
carroll-jabberwocky     2.599  0.890  0.526  2.132  0.600  0.887  1.000 |  0.793  0.957  0.759
dickinson-because       2.465  0.883  0.529  2.217  0.362  0.887  1.000 |  0.904  0.831  0.751
hopkins-pied-beauty     3.110  0.800  0.399  2.337  0.163  0.953  0.933 |  0.898  0.946  0.850
poe-annabel-lee         3.700  0.896  0.446  2.143  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     2.083  0.724  0.561  2.205  0.214  0.643  1.000 |  0.508  1.000  0.508
shakespeare-sonnet-18   2.137  0.750  0.560  2.361  0.364  0.993  1.000 |  0.749  1.000  0.749
whitman-song-of-myself  2.706  0.859  0.261  2.043  0.150  0.894  1.000 |  0.886  0.837  0.742
wilde-theocritus        2.689  0.892  0.639  2.168  0.341  0.944  0.844 |  0.960  0.820  0.787
mean overall 0.740 over 9 poems
```
`machine`, bass=feet, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.596  2.584  0.017  0.994  1.000 |  0.881  0.811  0.715
carroll-jabberwocky     2.880  0.911  0.562  2.132  0.600  0.887  1.000 |  0.822  0.957  0.786
dickinson-because       2.746  0.907  0.571  2.217  0.362  0.887  1.000 |  0.934  0.831  0.776
hopkins-pied-beauty     3.305  0.829  0.424  2.337  0.163  0.953  0.933 |  0.947  0.946  0.897
poe-annabel-lee         3.911  0.911  0.467  2.143  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     2.472  0.791  0.625  2.205  0.214  0.643  1.000 |  0.580  1.000  0.580
shakespeare-sonnet-18   2.338  0.818  0.595  2.361  0.364  0.993  1.000 |  0.850  1.000  0.850
whitman-song-of-myself  2.846  0.879  0.287  2.043  0.150  0.894  1.000 |  0.918  0.837  0.769
wilde-theocritus        2.966  0.913  0.663  2.168  0.341  0.944  0.844 |  0.995  0.820  0.816
mean overall 0.777 over 9 poems
```
`jazz`, bass=height (the default):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.681  2.063  0.017  0.994  1.000 |  0.742  0.811  0.602
carroll-jabberwocky     2.563  0.890  0.513  1.651  0.600  0.887  1.000 |  0.887  0.957  0.848
dickinson-because       2.408  0.883  0.664  1.573  0.362  0.887  1.000 |  0.839  0.831  0.697
hopkins-pied-beauty     3.085  0.800  0.442  1.745  0.163  0.953  0.933 |  0.825  0.946  0.781
poe-annabel-lee         3.700  0.896  0.492  1.847  0.154  0.841  0.811 |  0.790  0.914  0.722
preseren-zdravljica     1.972  0.724  0.683  1.542  0.214  0.643  1.000 |  0.732  1.000  0.732
shakespeare-sonnet-18   2.129  0.750  0.745  1.854  0.364  0.993  1.000 |  0.923  1.000  0.923
whitman-song-of-myself  2.615  0.859  0.307  1.546  0.150  0.894  1.000 |  0.742  0.837  0.622
wilde-theocritus        2.689  0.892  0.713  2.099  0.341  0.944  0.844 |  0.890  0.820  0.729
mean overall 0.740 over 9 poems
```
`jazz`, bass=syllables (W1):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  1.000  2.728  0.017  0.994  1.000 |  0.684  0.811  0.555
carroll-jabberwocky     2.599  0.890  1.000  2.983  0.600  0.887  1.000 |  0.875  0.957  0.836
dickinson-because       2.465  0.883  1.000  2.725  0.362  0.887  1.000 |  0.838  0.831  0.696
hopkins-pied-beauty     3.110  0.800  0.712  2.680  0.163  0.953  0.933 |  0.868  0.946  0.822
poe-annabel-lee         3.700  0.896  0.765  2.673  0.154  0.841  0.811 |  0.816  0.914  0.745
preseren-zdravljica     2.083  0.724  1.000  2.603  0.214  0.643  1.000 |  0.738  1.000  0.738
shakespeare-sonnet-18   2.137  0.750  1.000  2.624  0.364  0.993  1.000 |  0.876  1.000  0.876
whitman-song-of-myself  2.706  0.859  0.487  2.768  0.150  0.894  1.000 |  0.853  0.837  0.714
wilde-theocritus        2.689  0.892  1.000  2.788  0.341  0.944  0.844 |  0.820  0.820  0.672
mean overall 0.739 over 9 poems
```
`jazz`, bass=feet (W2):
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.575  2.584  0.017  0.994  1.000 |  0.742  0.811  0.602
carroll-jabberwocky     2.599  0.890  0.526  2.132  0.600  0.887  1.000 |  0.949  0.957  0.908
dickinson-because       2.465  0.883  0.529  2.217  0.362  0.887  1.000 |  0.910  0.831  0.756
hopkins-pied-beauty     3.110  0.800  0.399  2.337  0.163  0.953  0.933 |  0.868  0.946  0.821
poe-annabel-lee         3.700  0.896  0.446  2.143  0.154  0.841  0.811 |  0.816  0.914  0.745
preseren-zdravljica     2.083  0.724  0.561  2.205  0.214  0.643  1.000 |  0.801  1.000  0.801
shakespeare-sonnet-18   2.137  0.750  0.560  2.361  0.364  0.993  1.000 |  0.951  1.000  0.951
whitman-song-of-myself  2.706  0.859  0.261  2.043  0.150  0.894  1.000 |  0.790  0.837  0.661
wilde-theocritus        2.689  0.892  0.639  2.168  0.341  0.944  0.844 |  0.890  0.820  0.729
mean overall 0.775 over 9 poems
```
`jazz`, bass=feet, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.596  2.584  0.017  0.994  1.000 |  0.735  0.811  0.597
carroll-jabberwocky     2.880  0.911  0.562  2.132  0.600  0.887  1.000 |  0.926  0.957  0.886
dickinson-because       2.746  0.907  0.571  2.217  0.362  0.887  1.000 |  0.885  0.831  0.735
hopkins-pied-beauty     3.305  0.829  0.424  2.337  0.163  0.953  0.933 |  0.868  0.946  0.822
poe-annabel-lee         3.911  0.911  0.467  2.143  0.154  0.841  0.811 |  0.802  0.914  0.733
preseren-zdravljica     2.472  0.791  0.625  2.205  0.214  0.643  1.000 |  0.801  1.000  0.801
shakespeare-sonnet-18   2.338  0.818  0.595  2.361  0.364  0.993  1.000 |  0.951  1.000  0.951
whitman-song-of-myself  2.846  0.879  0.287  2.043  0.150  0.894  1.000 |  0.780  0.837  0.653
wilde-theocritus        2.966  0.913  0.663  2.168  0.341  0.944  0.844 |  0.870  0.820  0.713
mean overall 0.766 over 9 poems
```


### Sound and voice, step 5, review fixes

- **Swing now slices the bar, not the second.** The pattern ended `.cpm(c).swingBy(x, n)`.
  `.cpm` is `_fast(cpm / 60)` (`@strudel/core` 1.2.6 `pattern.mjs`, pinned by `@strudel/web`
  1.3.0), so after it a cycle is the scheduler's second, and `swingBy` sliced *that* into `n`.
  The result was out of step with the bar's beats. This was on HEAD before the harmony and is a
  likely source of the off-rhythm beats the user heard. It is now `.swingBy(x, n).cpm(c)`. The only
  snapshot that moved is `corpus__pattern-villanelle-jazz` (the two lines swap), and a unit test pins
  the order. In Chromium, the evaluated pattern was queried in 0.05 s windows as the scheduler does,
  with the old order rebuilt from the same text for comparison. Under the old order, `s("hh*8")` at
  32 cpm with swing 1/3 over 4 had 6 onsets at irregular times; it now has 8 (0, .313, .469, .781,
  …). For Theocritus at swing 0.33 under harmony=both, the old order dropped the chords of bars 2,
  15, 17, 28 and 30, doubled bars 3, 14, 16, 27, 29 and 31, and put 15 bars off the bar line. Now
  every bar that should have a chord starts exactly once, on the bar line. The same holds under
  rhyme and cadence. For The Tyger at swing 0.08, every odd bar was off the bar line before; now
  none is. During real playback, a hook on `gm_pad_warm`'s trigger saw 9 chord onsets, 27 tones,
  one bar (1.487 s) or two (across a rest) apart, with no doubles.
- **Chord roots are out of melodic entropy.** The scorer counted unfolded roots (V/9 and V/16 as two
  pitches), but the emitter folds them into one octave of a scale the scorer cannot see. A chord
  still counts as pitched and as its own instrument. Entropy is now identical with the harmony on
  and off. The step-5 tables below are re-measured. Machine: off 0.743, rhyme 0.761, cadence
  0.775, both 0.763. Jazz: off 0.740, rhyme 0.736, cadence 0.731, both 0.735. Before the fix, jazz
  read 0.739, 0.767 and 0.755: cadence's lead there came from the miscount.
- **The header no longer calls non-triads triads.** Under a scale that does not have seven notes,
  the chords are "three-tone chords on every other degree". A second `chords:` line says that the
  numerals count steps of that scale and are not functions: in five notes, "V" is not a dominant.
- **Verification trap:** measure from `target/release` only after `cargo build --release
  --workspace --locked`. Building only the web crate leaves a stale CLI without `harmony`.

### Sound and voice, step 5 — the harmony layer, as a candidate

The user approved the proposal's H1 and H2 as variants to compare by ear. `Mapping::Harmony`
joins `prosody_core::mapping` with `off` (the default), `rhyme` (H1), `cadence` (H2) and `both`
(H2 where it applies — open and closing lines, the turnaround — and H1 in every other bar). Off,
nothing changes: no snapshot moved, the machine cycle reads 0.743 and the jazz cycle 0.740.

- **Structure in the core** (`prosody_core::harmony`, called from `lower::run_with`). One entry per
  line: a `Chord` (bar, span = the line's rhyme word, root degree, `Function` I/IV/V, `Reason`) or a
  `Rest` (`Unrhymed`, `Wordless`, `Flourish`). Degrees are the melody's (§4.5's numbering, shared
  through `lower::degrees`). `Score::chord_in(slot)` is the one place a slot's chord is chosen: the
  chord of the whole line sounding in the foundation layer there — host or loop under a flourish —
  never a call, held or refrain lane. Every rule follows the proposal table; the choices it left
  open are in `SPEC.md` §4.6: open beats closing (AAAA → V V V I), the key for IV is the stanza's
  first in-stanza rhyme's degree (else 0), and the turnaround replaces the last bar's chord.
- **The turnaround is I2-safe**: V of the stanza's *own* first chord, the bar the loop plays next,
  not of the next stanza's (which would make stanza *n*'s last bar depend on stanza *n*+1). The
  cross-stanza form is a question for the user, not code.
- **Palette in the emitter.** `chord_sound` and `chord_gain` (style keys, page controls, preset
  defaults: default `gm_pad_warm` 0.40, warm `gm_epiano1` 0.35, raw `triangle` 1, jazz `gm_epiano1`
  0.45 — estimates). The chord lane is one `note("<…>")` pattern, one stacked chord or `~` a slot:
  a triad under `voicing = "note"`, a seventh chord under `"seventh"`, the root folded into one
  octave of the scale, an octave below the melody, the level shared between the tones and times the
  bar's density factor, attack 0.05 and release 0.6. The header lists `chord_sound`, `chord_gain`, a
  `chords:` line and `chords by line: 1:V/4 2:I/0 …` only when the piece has a chord. OSC has no
  chord message: its text and `--send` say how many chords are not scheduled.
- **Visible.** The debug view (`--explain`, the page's explain tab) adds `harmony V deg 5 (V of deg
  1) · cadence: B opens, answered later in its stanza` under each line, or `harmony no chord · rhyme:
  the line rhymes with nothing, so it has no degree`, and lists the `harmony §4.6` pass only when the
  layer is on. The scorer counts a chord as one onset a bar on its own instrument, pitched (but
  not in melodic entropy — see the review fixes above).
- **Tests.** Hand-worked (`crates/prosody-core/tests/harmony.rs`): ABAB → rhyme 0 1 0 1; cadence
  V/4 V/5 I/0 V/8; the mixed fixture under all three variants; open-wins; a one-line stanza; spans;
  flourish rests; the chord a slot plays under a held lane and under a flourish host. Emitter:
  triads and folded sevenths by MIDI number, rests, the header, palette-only changes; OSC
  disclosure; style keys; CLI and page end to end. Properties: I4 (every chord is its line's rhyme
  word, in a foundation stanza), the harmony changes nothing but the chords (events, voices,
  arrangement identical under any mappings), I2 for the harmony (scoped as `SPEC.md` §4.6
  discloses; a mutation making the turnaround depend on the stanza's bar offset failed it), I3 for
  the chord lane, and the I1 and invents-nothing properties now draw from all 384 combinations.
- **Checked in Chromium on port 4883**, The Tyger and Theocritus, each variant chosen on side A:
  the pattern evaluates and plays with no console errors or warnings; the chord lane queried from
  the evaluated pattern gives 15 chord-tone onsets in the first 8 cycles of The Tyger (5 bars × 3
  tones); `gm_pad_warm`
  (`0890_JCLive_sf2_file.js`) loads 200. Peak / RMS at the superdough destination gain over 8 s:
  The Tyger (rhyme) whole piece 0.783 / 0.127, chord lane alone 0.225 / 0.043; Theocritus
  (cadence) 0.703 / 0.118 and 0.179 / 0.045. No one has heard it.

**Measured** (`./scripts/cycle.sh <profile> harmony=<variant>`, after the review fixes below:
a chord's root no longer counts toward pitch entropy). Legibility does not move: the edit
test compares events, and a chord is not one — so these numbers say what the chords add to
interest (layering, recurrence, the pitched share), not whether an edit's harmony change is
audible. Entropy is now the same with the harmony on as off, by construction.

`machine`, harmony off:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.681  2.063  0.017  0.994  1.000 |  0.933  0.811  0.757
carroll-jabberwocky     2.563  0.890  0.513  1.651  0.600  0.887  1.000 |  0.790  0.957  0.755
dickinson-because       2.408  0.883  0.664  1.573  0.362  0.887  1.000 |  0.898  0.831  0.746
hopkins-pied-beauty     3.085  0.800  0.442  1.745  0.163  0.953  0.933 |  0.905  0.946  0.857
poe-annabel-lee         3.700  0.896  0.492  1.847  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     1.972  0.724  0.683  1.542  0.214  0.643  1.000 |  0.503  1.000  0.503
shakespeare-sonnet-18   2.129  0.750  0.745  1.854  0.364  0.993  1.000 |  0.733  1.000  0.733
whitman-song-of-myself  2.615  0.859  0.307  1.546  0.150  0.894  1.000 |  0.904  0.837  0.757
wilde-theocritus        2.689  0.892  0.713  2.099  0.341  0.944  0.844 |  0.950  0.820  0.779
mean overall 0.743 over 9 poems
```
`machine`, harmony=rhyme:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.933  0.811  0.757
carroll-jabberwocky     2.718  0.900  0.535  1.651  0.600  0.887  1.000 |  0.805  0.957  0.770
dickinson-because       2.444  0.879  0.668  1.573  0.362  0.887  1.000 |  0.901  0.831  0.749
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.955  0.946  0.904
poe-annabel-lee         3.867  0.908  0.507  1.847  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     2.083  0.727  0.698  1.542  0.214  0.643  1.000 |  0.510  1.000  0.510
shakespeare-sonnet-18   2.273  0.800  0.759  1.854  0.364  0.993  1.000 |  0.792  1.000  0.792
whitman-song-of-myself  2.664  0.859  0.316  1.546  0.150  0.894  1.000 |  0.916  0.837  0.767
wilde-theocritus        2.950  0.911  0.731  2.099  0.341  0.944  0.844 |  0.969  0.820  0.795
mean overall 0.761 over 9 poems
```
`machine`, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.933  0.811  0.757
carroll-jabberwocky     2.845  0.911  0.550  1.651  0.600  0.887  1.000 |  0.818  0.957  0.782
dickinson-because       2.690  0.907  0.694  1.573  0.362  0.887  1.000 |  0.928  0.831  0.771
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.955  0.946  0.904
poe-annabel-lee         3.911  0.911  0.511  1.847  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     2.361  0.791  0.729  1.542  0.214  0.643  1.000 |  0.560  1.000  0.560
shakespeare-sonnet-18   2.331  0.818  0.765  1.854  0.364  0.993  1.000 |  0.820  1.000  0.820
whitman-song-of-myself  2.755  0.879  0.332  1.546  0.150  0.894  1.000 |  0.938  0.837  0.785
wilde-theocritus        2.966  0.913  0.732  2.099  0.341  0.944  0.844 |  0.971  0.820  0.796
mean overall 0.775 over 9 poems
```
`machine`, harmony=both:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.933  0.811  0.757
carroll-jabberwocky     2.746  0.903  0.538  1.651  0.600  0.887  1.000 |  0.808  0.957  0.772
dickinson-because       2.444  0.879  0.668  1.573  0.362  0.887  1.000 |  0.901  0.831  0.749
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.955  0.946  0.904
poe-annabel-lee         3.867  0.908  0.507  1.847  0.154  0.841  0.811 |  0.876  0.914  0.801
preseren-zdravljica     2.139  0.743  0.705  1.542  0.214  0.643  1.000 |  0.521  1.000  0.521
shakespeare-sonnet-18   2.273  0.800  0.759  1.854  0.364  0.993  1.000 |  0.792  1.000  0.792
whitman-song-of-myself  2.706  0.869  0.324  1.546  0.150  0.894  1.000 |  0.926  0.837  0.775
wilde-theocritus        2.950  0.911  0.731  2.099  0.341  0.944  0.844 |  0.969  0.820  0.795
mean overall 0.763 over 9 poems
```
`jazz`, harmony off:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.586  0.939  0.681  2.063  0.017  0.994  1.000 |  0.742  0.811  0.602
carroll-jabberwocky     2.563  0.890  0.513  1.651  0.600  0.887  1.000 |  0.887  0.957  0.848
dickinson-because       2.408  0.883  0.664  1.573  0.362  0.887  1.000 |  0.839  0.831  0.697
hopkins-pied-beauty     3.085  0.800  0.442  1.745  0.163  0.953  0.933 |  0.825  0.946  0.781
poe-annabel-lee         3.700  0.896  0.492  1.847  0.154  0.841  0.811 |  0.790  0.914  0.722
preseren-zdravljica     1.972  0.724  0.683  1.542  0.214  0.643  1.000 |  0.732  1.000  0.732
shakespeare-sonnet-18   2.129  0.750  0.745  1.854  0.364  0.993  1.000 |  0.923  1.000  0.923
whitman-song-of-myself  2.615  0.859  0.307  1.546  0.150  0.894  1.000 |  0.742  0.837  0.622
wilde-theocritus        2.689  0.892  0.713  2.099  0.341  0.944  0.844 |  0.890  0.820  0.729
mean overall 0.740 over 9 poems
```
`jazz`, harmony=rhyme:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.735  0.811  0.597
carroll-jabberwocky     2.718  0.900  0.535  1.651  0.600  0.887  1.000 |  0.877  0.957  0.838
dickinson-because       2.444  0.879  0.668  1.573  0.362  0.887  1.000 |  0.843  0.831  0.700
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.825  0.946  0.781
poe-annabel-lee         3.867  0.908  0.507  1.847  0.154  0.841  0.811 |  0.780  0.914  0.713
preseren-zdravljica     2.083  0.727  0.698  1.542  0.214  0.643  1.000 |  0.735  1.000  0.735
shakespeare-sonnet-18   2.273  0.800  0.759  1.854  0.364  0.993  1.000 |  0.923  1.000  0.923
whitman-song-of-myself  2.664  0.859  0.316  1.546  0.150  0.894  1.000 |  0.746  0.837  0.625
wilde-theocritus        2.950  0.911  0.731  2.099  0.341  0.944  0.844 |  0.871  0.820  0.714
mean overall 0.736 over 9 poems
```
`jazz`, harmony=cadence:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.735  0.811  0.597
carroll-jabberwocky     2.845  0.911  0.550  1.651  0.600  0.887  1.000 |  0.865  0.957  0.828
dickinson-because       2.690  0.907  0.694  1.573  0.362  0.887  1.000 |  0.816  0.831  0.678
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.825  0.946  0.781
poe-annabel-lee         3.911  0.911  0.511  1.847  0.154  0.841  0.811 |  0.777  0.914  0.710
preseren-zdravljica     2.361  0.791  0.729  1.542  0.214  0.643  1.000 |  0.735  1.000  0.735
shakespeare-sonnet-18   2.331  0.818  0.765  1.854  0.364  0.993  1.000 |  0.923  1.000  0.923
whitman-song-of-myself  2.755  0.879  0.332  1.546  0.150  0.894  1.000 |  0.733  0.837  0.614
wilde-theocritus        2.966  0.913  0.732  2.099  0.341  0.944  0.844 |  0.870  0.820  0.713
mean overall 0.731 over 9 poems
```
`jazz`, harmony=both:
```
poem                    layer  recur  pitch   entr   sync  pulse steady |  inter  legib overall
blake-tyger             3.837  0.949  0.697  2.063  0.017  0.994  1.000 |  0.735  0.811  0.597
carroll-jabberwocky     2.746  0.903  0.538  1.651  0.600  0.887  1.000 |  0.874  0.957  0.836
dickinson-because       2.444  0.879  0.668  1.573  0.362  0.887  1.000 |  0.843  0.831  0.700
hopkins-pied-beauty     3.280  0.829  0.465  1.745  0.163  0.953  0.933 |  0.825  0.946  0.781
poe-annabel-lee         3.867  0.908  0.507  1.847  0.154  0.841  0.811 |  0.780  0.914  0.713
preseren-zdravljica     2.139  0.743  0.705  1.542  0.214  0.643  1.000 |  0.735  1.000  0.735
shakespeare-sonnet-18   2.273  0.800  0.759  1.854  0.364  0.993  1.000 |  0.923  1.000  0.923
whitman-song-of-myself  2.706  0.869  0.324  1.546  0.150  0.894  1.000 |  0.739  0.837  0.619
wilde-theocritus        2.950  0.911  0.731  2.099  0.341  0.944  0.844 |  0.871  0.820  0.714
mean overall 0.735 over 9 poems
```

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
