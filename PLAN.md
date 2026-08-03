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
| M1 | Tree-sitter grammar: frontmatter / stanza / line / caesura / token, incremental reparse | todo | `SPEC.md` §7.1 — incremental from day one |
| M2 | `Phonemiser` trait + CMUdict + letter-to-sound fallback | todo | §7.2, §9 — trait boundary designed for eSpeak-NG second impl |
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

**Iteration 2 — M1: Tree-sitter grammar.** Not yet planned; plan it at the start of the iteration.

Carried into M1 from iteration 1's review:
- Add `Domain::Caesura` when the grammar produces real caesura nodes (`SPEC.md` §4.1's second
  tier). Deliberately absent now — no consumer yet, and adding a variant later is additive and
  does not disturb the frozen contexts.
- Grammar node spans feed `Span::new`; computed spans (caesura splits) must use `Span::try_new`.

---

## Log

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
