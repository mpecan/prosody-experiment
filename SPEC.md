# Prosody-as-Score — Context Dump for Experimental Build

**Status:** exploratory prototype. Nothing here is settled; the invariants in §6 are the only parts worth defending hard.

---

## 1. The idea in one paragraph

A poem is the source code for an electronic music track. Not a prompt, not a lyric sheet — a **score**. The text's own properties (meter, stress, phonetics, rhyme, repetition, lexical rarity) are parsed and compiled into layered musical structure. A style header supplies palette and constraint only. The design goal that everything else serves: **editing a line for poetic reasons must simultaneously be a musically meaningful edit.** If the two constraints ever pull apart, the design is wrong.

The failure mode to design against: a random generator with a text-shaped seed. Output that sounds fine but where no specific audible feature can be traced to a specific word.

---

## 2. Prior art (checked — none of it occupies this exact space)

**Poem-as-program esolangs.** Rockstar (programs are 80s rock lyrics), Poetic and its successor Bespoke (brainfuck instructions encoded in word lengths), Shakespeare (source looks like a play), Coem, Sukanya Aneja's `in:verse`. All output text or graphics. The *music* case is essentially unclaimed.

**Readable live-coding languages.** ixi lang deliberately moved from `object.method(arg)` to verb-noun-adjective (`shift jarret 2`) for lay legibility. Mercury is built entirely around human-readability with a 30-line editor cap. Both are readable code, not writable poetry — you can't compose a stanza in either.

**Vocable synthesis — the closest hit.** Alex McLean & Geraint Wiggins (ICMC 2008, NIME 2009). Vocable words drive physical-modelling synthesis, drawing on tabla *bols* and Scottish *canntaireachd*. McLean's stated goal was typing "krapplesnaffle" and having it become sound placed directly into a live-coded rhythm. Relevant lineage note: **TidalCycles itself began as a sound-poetry analysis tool** — rhythmic continuation in Schwitters' *Ursonate*, inspired by Bernard Bel's Bol Processor. Tidal's ancestor was a poem parser.

**Text-score tradition (human-compiled).** Fluxus event scores, Oliveros *Sonic Meditations*, Stockhausen *Aus den sieben Tagen*.

**Gap statement.** *Organised Sound* (2023), "Live Coding Poetry: The narrative of code in a hybrid musical/poetic context" — concludes the intersection of live coding and code poetry "seems to have not yet been thoroughly explored."

Read McLean's NIME09 paper (`slab.org/writing/nime09.pdf`) before designing the phonetic layer.

---

## 3. Architecture

```
poem.txt
   │
   ├─ style header (TOML frontmatter) ──────────┐
   │                                            │  palette + constraint only
   ▼                                            │
Tree-sitter parse → CST                         │
   │                                            │
   ▼                                            │
Analysis passes (pure, deterministic)           │
   ├─ tokenise + phonemise (CMUdict)            │
   ├─ stress / foot detection                   │
   ├─ rhyme classification                      │
   ├─ repetition & morphological similarity     │
   └─ surprisal / distinctiveness               │
   │                                            │
   ▼                                            │
Role assignment per stanza (§4.1)               │
   │                                            │
   ▼                                            ▼
Voice IR  ←──────── per-unit seeded PRNG ── style resolution
   │
   ▼
Backend: Strudel/Tidal emit  |  OSC → SuperCollider  |  direct DSP
```

---

## 4. Mappings

### 4.1 Role assignment — derive it, don't position it

Do **not** hardcode "stanza 1 = base loop". Compute per-stanza metrical variance (stddev of syllable count and of foot count across its lines):

- **low variance → periodic → foundation layer** (drums, bass, ostinato)
- **high variance → aperiodic → flourish layer** (fills, stabs, one-shots, ornament)

This is the load-bearing decision. It means writing a tight regular stanza *is* writing a tight loop, and writing a ragged one *is* writing a fill. Stanza breaks (blank lines) are the unit boundary; consider a second tier where a line-internal caesura splits a voice.

### 4.2 Meter → rhythm

- foot → beat
- stress → accent / velocity
- syllable → subdivision within the foot
- line → bar (or phrase)

Consequence worth embracing rather than correcting: iambic pentameter yields five beats. Poetry naturally produces odd meters. Don't quantise to 4/4.

### 4.3 Repetition → structure (highest-value pass)

Repetition is not one thing. Distinguish:

| Figure | Detection | Musical result |
|---|---|---|
| Refrain | whole line repeats | literal loop of that voice |
| Anaphora | line-initial n-gram repeats | downbeat-anchored ostinato |
| Epistrophe | line-final n-gram repeats | cadence figure / turnaround |
| Alliteration / assonance | onset or nucleus phoneme match in window | timbral continuity — same sample bank |
| **Morphological near-repetition** | stem match + phoneme edit distance | **same voice, varied parameters** |

The last row is the one that earns the project. "burn / burning / burnt" → same voice, modulated envelope, filter, transposition. Exact repetition gives the loop; near-repetition gives the variation on the loop. That is most of electronic music's grammar.

Suggested: `stem_match && phoneme_levenshtein <= k` → variation edge, with distance driving modulation depth.

### 4.4 Distinctiveness → sharpness

Needs a real metric or it degenerates into vibes. Use:

```
surprisal(word) = α · lexical_rarity(word)        // static unigram table, offline
                + β · phoneme_set_distance(word, local_window)
```

Drives transient sharpness, brightness/cutoff, attack time, dry/wet. Fully offline, small tables, deterministic. No model required.

### 4.5 Rhyme → melody

Treat rhyme as an **equivalence relation**. Rhyme class → scale degree, assigned in order of first appearance from the style's scale.

- ABAB → alternating cadences
- AABB → paired resolution
- villanelle → a two-note obsession (which is what a villanelle already is)

Then the elegant part: **perfect rhyme → exact pitch; slant rhyme → detune or neighbour tone.** Rhyme precision maps to pitch precision. Score rhyme quality continuously (nucleus + coda phoneme match) rather than boolean.

### 4.6 Arrangement — stanzas loop until something takes their place

*Decided 2026-10-08, after the scorer measured every poem as a single line of sound.*

A stanza, once stated, **keeps looping** until something stops it. One loop per layer (§4.1):

- **A competing stanza** — the next stanza of the *same* layer — transforms the running loop into
  itself, one bar per pass: `|B1 A2 A3 A4|B1 B2 A3 A4|B1 B2 B3 A4|B1 B2 B3 B4|`. The other layer's
  loop keeps playing underneath.
- **A refrain stanza** — every line a refrain — overrules: all loops stop and it is stated alone,
  like a chorus dropping in. A real *chord progression* would be the other natural overrule, and
  needs a harmony layer that does not exist yet.

Bar-locked: each bar of the piece takes its length from the line arriving in it, and whatever
loops underneath is fitted to that bar. The piece ends when the last stanza has fully arrived.

**The call lane** (§4.1's second tier, decided the same day). The half-line before a line's first
internal caesura — its *call*: "Tyger Tyger," — is lifted out once its line has been heard, and
loops in a third lane under everything until the next line with a caesura is first heard. A refrain
stanza stops it with the other loops. Caesurae are sparse, so each one leaves a figure that
persists; this is what lets a poem whose stanzas share one layer sound more than one thing at once.

This is structure, read from the poem, not a style parameter — which answers §9's "looping as an
explicit style parameter" question: it is not one, and I3 is untouched. It does make *duration*
poem-wide: how long a stanza sounds depends on when the next stanza of its layer arrives.

---

## 5. Style header

TOML frontmatter. Palette and constraint **only**:

```toml
[style]
scale       = "phrygian_dominant"
root        = "D"
tempo_range = [128, 134]
swing       = 0.12
bank        = "tr909"
synth       = "karplus"       # or fm, subtractive, physical
```

Never structure. See invariant I3.

---

## 6. Invariants (defend these)

- **I1 — Determinism.** Same poem + same style ⇒ bit-identical output. Always.
- **I2 — Per-unit seeding.** Seed the PRNG per stanza (or per line) from a hash of *that unit's* content, never globally. A global seed breaks locality: changing one word reshuffles the whole piece. Per-unit seeding + Tree-sitter's dirty-node set means an edit touches only the voices it should — this is what makes it live-codeable rather than render-and-pray.
- **I3 — Style is palette, not structure.** The moment the style block can alter arrangement, the poem stops being the score and becomes an ornament on a preset.
- **I4 — Traceability.** Every audible event must be attributable to a specific token span. If it can't be traced, it shouldn't exist.

---

## 7. Build order

1. **Tree-sitter grammar** — stanza / line / caesura / token / frontmatter. Incremental reparse from day one; retrofitting it is painful.
2. **Phonemiser** — CMUdict lookup with a letter-to-sound fallback for OOV words. See §9 on the language limitation.
3. **Meter + rhyme analysis** — stress vectors, foot segmentation, rhyme equivalence classes.
4. **Repetition passes** — exact, then anaphora/epistrophe, then morphological. Ship exact first.
5. **Voice IR + role assignment.**
6. **Backend emit** — Strudel or Tidal pattern text first. Cheapest possible audition path; no DSP work needed to hear whether the mapping is any good.
7. **Debug annotation view** (see §8).
8. Only then: direct synthesis, OSC, live reload.

---

## 8. First test to write — before any of the fun parts

Take a couplet. Swap one word for a synonym with different phonetics and stress. Confirm you can **hear a specific, explicable change**, and that the change is confined to the expected voices.

Build the debug view alongside it: render the poem with each token annotated by what it became (role, beat position, pitch class, modulation, seed). This will earn its keep within a day and is the only real defence against I4 quietly failing.

Property tests worth having early:
- determinism across runs (I1)
- editing stanza *n* leaves voices from stanza *m ≠ n* byte-identical (I2)
- changing only the style header never changes the event *structure*, only its realisation (I3)

---

## 9. Known problems

- **CMUdict is English-only.** Multilingual poems, Slovenian, Italian, German — all unsupported. Options: eSpeak-NG phonemisation (broad language coverage, permissive licence, but a different phoneme inventory and lower quality), or per-language dictionaries with a pluggable phonemiser trait. Design the trait boundary now even if only English is implemented; retrofitting a phoneme inventory abstraction later is grim.
- **Free verse.** Poems with no regular meter make §4.1 degenerate — everything reads as high-variance flourish with no foundation. Needs a fallback: possibly a global-variance-relative threshold rather than absolute, so the *most* regular stanza in a poem always anchors.
- **Stress detection ambiguity.** CMUdict gives lexical stress, not phrasal stress. "Content" (noun) vs "content" (adj) differ. Accept the error rate initially; do not build a POS tagger in week one.
- **Very short poems** produce very little structure. Define minimum viable input, or make looping/development an explicit style parameter (which arguably violates I3 — think it through). *Looping is now structure (§4.6), not style — but a single-stanza poem still never loops, since nothing follows it.*
- **Aesthetic risk.** The mapping can be perfectly principled and still sound bad. Budget time for tuning the *ranges* (which is palette work, and legitimately belongs in the style layer).

---

## 10. Stack suggestion

Rust. Tree-sitter for the grammar with incremental reparse. Pure analysis passes over an immutable CST, all IO at the edges. Strudel/Tidal text emit as the first backend for zero-cost auditioning; OSC → SuperCollider once the mappings hold up. CLI first, single responsibility — parse in, pattern out — with the live-reload watcher as a separate concern.

Open question worth deciding early: whether the CST → Voice IR lowering is one pass or several. Several is easier to test and to attribute (I4), and cheap enough at poem scale.
