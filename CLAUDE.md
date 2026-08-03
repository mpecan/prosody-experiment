# prosody-experiment

A poem is the source code for an electronic music track. `SPEC.md` is the design document —
read it before any non-trivial change. This file says *how* we build it.

Rust 1.96, edition 2024. Everything below is enforceable; if a rule can't be checked by a
command or a type, it doesn't belong here.

---

## 1. Non-negotiables

The four invariants in `SPEC.md` §6 are the product. They are not aspirations — each one has a
mechanical enforcement and a test. Never weaken the enforcement to make a feature fit.

| Invariant | Enforced by | Test |
|---|---|---|
| **I1** Determinism: same poem + style ⇒ bit-identical output | no ambient nondeterminism (§4) | property test: N runs, same bytes |
| **I2** Per-unit seeding from that unit's content | `Seed::for_unit(domain, span, source)` — a span, never a bare `&str`, so a caller cannot pass the whole poem where a unit belongs; there is no global RNG and no unseeded one | property test: edit stanza *n*, stanza *m ≠ n* voices byte-identical |
| **I3** Style is palette, not structure | `Style` is only readable during *realisation*, never during analysis or role assignment | property test: change style ⇒ event structure identical, only parameters differ |
| **I4** Traceability: every event attributable to a token span | every IR node carries a non-`Option` `Span`; constructors require it | property test: every emitted event resolves to a byte range that exists in the source |

If you find yourself wanting to violate one, stop and say so in the response rather than
quietly working around it. I3 in particular has a known pressure point (`SPEC.md` §9, short
poems / looping) that is unresolved — surface it, don't decide it silently.

---

## 2. Layout

Workspace, so the purity boundary is a compiler-enforced dependency edge rather than a
convention:

```
crates/
  prosody-core/      pure analysis + IR + realisation. NO io, NO clock, NO env, NO fs.
  prosody-cmudict/   the CMU dictionary, vendored and embedded
  prosody-lts/       guessing pronunciation from spelling
  prosody-emit/      backend emitters (Strudel/Tidal text first; OSC later)
  prosody-cli/       argument parsing, file reads, stdout, the watcher. All IO lives here.
  tree-sitter-prosody/  the grammar
```

**A concrete phonemiser is its own crate.** `prosody-core` holds the inventory-agnostic model —
`Phoneme`, `Inventory`, `Phonemiser`, `syllabify` — that `SPEC.md` §9 requires so a second
language can be added without a retrofit. An English ruleset or an English dictionary is not
that, and a future Slovenian phonemiser must not inherit one by depending on the core. The
`ARPAbet` inventory stays in the core as the one canonical `Inventory` value the abstraction
needs in order to exist at all.

`prosody-core` must not depend on `std::fs`, `std::time`, `std::env`, or any network/audio
crate. Adding such a dependency to `prosody-core` is a design error, not a shortcut.

Inside `prosody-core`, follow the pipeline shape from `SPEC.md` §3 — one module per pass:

```
parse → phoneme → meter → rhyme → repetition → surprisal → role → lower → realise
```

Each pass is a free function `fn run(input: &PrevStage, ...) -> Result<NextStage, Error>` with
its own tests. `SPEC.md` §10 asks whether lowering is one pass or several: **several**. It is
cheaper to test and it is what makes I4 attributable.

---

## 3. Quality gates

A change is not done until all of these pass. Run them; do not assume.

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Run them with `--locked`, as CI does. The `RUSTDOCFLAGS` prefix is not decoration: the lints
table only denies `broken_intra_doc_links`, so without it a local `cargo doc` enforces less than
CI does and missing docs slip through.

Lints are configured in the workspace `Cargo.toml`, not with per-file `#![allow]`:

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unreachable_pub = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"     # slicing a poem by index is how you panic on unicode
float_cmp = "deny"
todo = "deny"
dbg_macro = "deny"
```

Test code may `unwrap` and index; put
`#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing))]`
at the crate root rather than sprinkling allows — a panic in a test is a test failure, which is
what it should be. Any *other* `#[allow]` needs a comment saying why, on the same line.

**The one crate exempt from `unsafe_code = "forbid"`** is `tree-sitter-prosody`, whose entire
Rust content is the `extern "C"` declaration binding the generated parser. It opts out by not
setting `[lints] workspace = true`. No hand-written logic belongs in that crate; if it grows any,
the logic goes in `prosody-core` instead.

`todo!()` and `unimplemented!()` are banned in committed code. A stub returns a typed error or
an empty result — a panic in a live-reload loop kills the session.

---

## 4. Determinism rules (these are how I1 actually holds)

Nondeterminism in Rust is quiet. Treat all of the following as bugs, and check for them when
reviewing a pass:

- **No `HashMap`/`HashSet` iteration.** Use `BTreeMap`/`BTreeSet`, or `IndexMap`, anywhere the
  contents influence output ordering. `HashMap` is fine for pure lookup that never gets iterated
  — but prefer `BTreeMap` and don't spend thought on it at poem scale.
- **Sort before you emit.** Any `Vec` built from a set or from parallel work gets an explicit,
  total sort key before it reaches the emitter. Ties broken by `Span`.
- **One RNG type, seeded explicitly:** `rand_chacha::ChaCha8Rng` (reproducible across platforms
  and versions; `StdRng` is explicitly not). Never `thread_rng`, never `rand::random`.
- **Stable hashing for seeds:** a fixed algorithm (`blake3` or `sha2`), never `DefaultHasher`,
  whose output is not stable across releases.
- **No floats in identity or control flow.** Floats are fine as *values* carried to the emitter.
  They are never a map key, never compared with `==`, never summed in an order-dependent way that
  changes a branch. Emit them via a fixed formatting helper (`fmt_param`, fixed decimal places)
  so text output is byte-stable.
- **No `SystemTime`, no `Instant`, no env, no locale** anywhere in `prosody-core`.
- **No parallelism** in the analysis pipeline until there's a measured reason. Poems are small.

---

## 5. Types

The mapping is the whole project; encode it in types so wrong mappings don't compile.

- **Newtype every unit.** `SyllableCount`, `FootCount`, `Beat`, `Subdivision`, `Semitone`,
  `ScaleDegree`, `Bpm`, `Velocity`, `StanzaIdx`. No bare `usize`/`f32` crossing a pass boundary.
  A function taking `(usize, usize, f32)` is a defect report waiting to happen.
- **A unit's span covers its content, not its delimiters.** A line's span is the extent of its
  tokens; a stanza's is the extent of its lines. Tree-sitter attributes a trailing newline
  differently depending on whether another stanza follows, so a raw node range would make a
  stanza's extent — and therefore its seed — depend on what comes *after* it. Appending a stanza
  would silently re-voice the previous one. Derive spans from content.
- **`Span { start: usize, end: usize }` is mandatory on every IR node**, not optional, not added
  later. This is the only real defence against I4 quietly failing (`SPEC.md` §8). Use
  `Span::new` only where `start <= end` is structurally guaranteed; when the bounds come from
  arithmetic, use `Span::try_new` so a reversed-argument bug surfaces instead of collapsing into
  an innocuous zero-length token.
- **A unit's seed is derived from a span, never from a loose string.** `Seed::for_unit` takes
  `(Domain, Span, &str)` deliberately: a bare `&str` parameter cannot tell one stanza from the
  whole poem, so misuse would stay perfectly deterministic (I1 intact) while breaking I2, and no
  test of the function itself could detect it.
- **Make illegal states unrepresentable.** Rhyme classes are an equivalence relation — represent
  them as a union-find or an interned `RhymeClassId`, not as a `String` compared repeatedly.
  Roles are an enum, not a bool `is_foundation`.
- **A guess must be constructible only as a guess.** `Pronunciation::from_dictionary` and
  `Pronunciation::guessed` are separate constructors precisely so that labelling a guess as a
  dictionary fact cannot be written by accident. Never add a constructor taking a `Provenance`
  argument.
- **Voice identity is not a seed.** Seeds avalanche: a one-character edit yields an unrelated
  stream. That is right for `SPEC.md` §4.3's refrain row and wrong for its morphological row,
  where "burn / burning / burnt" must stay *one* voice with modulated parameters. Voice identity
  comes from the repetition pass that groups related units (a `VoiceId`); the seed supplies only
  the residual jitter within an already-established voice. Deriving a `VoiceId` from a `Seed`
  turns the spec's highest-value mapping into three unrelated instruments.
- **Parse, don't validate.** The style header becomes a `Style` struct with validated ranges at
  the IO boundary; downstream code never re-checks `tempo_range`.
- **Trait boundaries the spec demands now, even with one impl:**
  `trait Phonemiser` (CMUdict today, eSpeak-NG/per-language later — `SPEC.md` §9) and
  `trait Backend` (Strudel text today, OSC later). Design them against the *second* implementation
  even though only the first exists.

---

## 6. Errors

- `prosody-core` / `prosody-emit`: `thiserror`, one error enum per crate, every variant carrying
  the `Span` it relates to. Poem errors must point at the poem.
- `prosody-cli`: `anyhow`, with context added at each IO edge.
- Never `panic!` on user input. A malformed poem, an OOV word, an unparseable header — all are
  `Err` or a documented fallback, never a crash.
- Fallbacks must be *visible*: if the phonemiser guesses via letter-to-sound, the debug view says
  so. Silent degradation is how the "random generator with a text-shaped seed" failure mode
  (`SPEC.md` §1) sneaks in.

---

## 7. Testing

Test the mapping, not the plumbing. A test that asserts "the function returns something" is
worse than no test — it costs maintenance and catches nothing.

**Required test kinds:**

1. **Property tests (`proptest`)** for I1–I4. These are written *before* the feature they guard.
   `SPEC.md` §8 lists the first three; add the I4 span-resolution property alongside them.
2. **Golden corpus + snapshots (`insta`).** `tests/corpus/*.poem` with committed snapshot output.
   Include: strict iambic pentameter, free verse, a villanelle, a refrain-heavy lyric, a one-line
   poem, a poem with an OOV/nonsense word, and a non-English poem (documenting the §9 limitation).
   Review every snapshot diff by reading it — accepting an `insta` diff without reading it is the
   main way a mapping silently rots.
3. **Anything approximate gets a measured floor.** A ruleset that produces confident nonsense
   passes every unit test written for it, because those tests came from the same rules. Grade it
   against data nobody involved chose, print the numbers, and fail the build when they regress —
   see `prosody-lts/tests/accuracy.rs`. Raise the floors when the thing improves; a floor far
   below the truth has stopped being a test.
4. **The §8 acceptance test, first.** A couplet; swap one word for a phonetically/metrically
   different synonym; assert (a) a specific named parameter changes in a specific voice, and
   (b) all other voices are byte-identical. This is the project's reason to exist — it comes
   before any backend polish.
5. **Unit tests colocated** in `#[cfg(test)] mod tests` for each analysis pass, with hand-worked
   expected values (e.g. a known stress vector), not values copied from the implementation's
   output.

Run tests after writing them and before reporting done — including when the change looked
mechanical.

---

## 8. Debug annotation view

Not an optional nicety — `SPEC.md` §8 makes it the enforcement mechanism for I4. Build it in
step with the passes, not after. It renders the poem with each token annotated by: assigned role,
beat position, pitch class, modulation depth, seed, and which pass decided each. Every new
mapping adds its contribution to this view in the same change that adds the mapping.

---

## 9. Build order

Follow `SPEC.md` §7 in order. It is sequenced so each step is auditionable. In particular:

- Tree-sitter with **incremental reparse from day one** — retrofitting it breaks I2.
- **Exact repetition ships before** anaphora/epistrophe/morphological.
- Strudel/Tidal **text emit before any DSP**. If the mapping is bad, hearing it early is the point.

Don't skip ahead to synthesis because it's more interesting. If a step feels blocked, say what's
blocking it rather than routing around it.

---

## 10. Working style for this repo

- **Trace before you tune.** If a change makes output "sound better" but you can't name the token
  span responsible, it's the failure mode, not a feature.
- **Ranges are palette, mappings are structure.** Tuning a number range is legitimate style work
  (`SPEC.md` §9). Changing *what* a poetic feature maps to is a spec change — raise it, don't do
  it in passing.
- **No new dependency** without saying what it replaces and why hand-rolling is worse. Audio and
  NLP crates in particular pull in nondeterminism and heavy trees.
- Keep functions small enough that a single pass fits on a screen; the passes are the unit of
  review.
- Comment *why* a mapping was chosen, at the mapping site, with a `SPEC.md` section reference.
  Don't comment what the code does.
