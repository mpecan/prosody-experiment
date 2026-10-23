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

*Observed 2026-10-22 (the off-rhythm trace; no mapping changed).* Two consequences of this mapping,
heard as "off-rhythm beats" and now named. **Cross-rhythm:** a foot of three syllables divides its
beat in three, one of two in two, so when lines with different feet sound in one bar (§4.6's held,
refrain and call lanes stack them) the beat carries 3 against 2 — Theocritus's dactylic line-ends
("Sicily", "Persephone") against its iambic tetrameter in 30% of its beats, The Tyger's "In the
forests" against "Tyger Tyger," in 3%. `prosody-score`'s jazz profile measures it (`cross_rhythm`).
**The elastic beat was in the Strudel backend:** a cycle was a slot at one `.cpm` for the
piece (tempo ÷ the integer mean bar length), so a bar of four beats among bars of three played its
beats a third faster — Theocritus's four-beat bars ran at 161 bpm against its stated 121. The OSC
backend and the scorer kept the beat constant; the page plays the Strudel text.

**The pulse** (*decided 2026-10-23 by the user: a steady beat*). Each bar lasts as long as its
beats, so the beat is the same speed throughout — "foot → beat" with every beat one length. It is
structure (when events fall), so it is a selector, `Mapping::Pulse`, not palette:

- **`steady`** (the default). In the Strudel pattern a cycle is a beat: the arrangement is one
  `<…>` whose bars carry mini-notation weights (`<[…]@3 […]@4>` — in `@strudel/mini` 1.2.6 a
  `polymeter_slowcat` of a `timeCat`, slowed by the weights' sum, so a bar of weight *w* lasts
  exactly *w* cycles), and `.cpm` is the tempo. Integers throughout; no rounded tempo.
- **`elastic`** — the pattern as it was, kept to compare against: a cycle is a bar.

Swing (§5) slices a cycle, so under `steady` it is `.swingBy(x, 1)` — every beat swung, in a bar of
three or of four alike; under `elastic` it keeps the mean bar length as before. The page's trace
and the reading's cue pattern are built on the same weighted bars and the same `.cpm`, a spoken
line's rate is fitted to the bar it is spoken in, and the trace hands the page each bar's start so
the roll's bar lines fall where the bars change. The OSC schedule is in beats and has always been
steady; it does not apply `elastic`, and says so in its header and on `--send`. The scorer reads the
score, which the pulse does not change, so it measures both variants alike — it already modelled a
steady beat. Against I2 the steady pulse is an improvement: under `elastic` a bar's length in
seconds depended on the whole poem's mean bar length, so an edit to stanza *n* that changed it
re-timed stanza *m*; under `steady` a bar's length depends only on its own beats (and the tempo,
which `tempo_range` already fits to the poem's density, as before).

**Stacked lines** (*decided 2026-10-23 by the user: crossing stays the default, straightening is a
variant to compare*). Where §4.6 stacks a line under the bar's host — held, refrain, call, a loop
yielding or carrying on under the other layer — "syllable → subdivision within the foot" lets the
two divide one beat differently, three against two. It is structure, so a selector,
`Mapping::Stack`, with no `off` (a stacked line always divides its beats somehow; its first
variant is what preceded it):

- **`crossing`** (the default) — every line keeps its own feet wherever it sounds; the cross-rhythm
  above.
- **`straight`** — in each beat where a stacked line's grid and the host's would cross (neither
  subdivision divides the other, measured as `prosody-score`'s `cross_rhythm` measures them: the
  least common multiple of the onsets' reduced denominators), the stacked line's syllables are
  fitted to the host's subdivision of that beat, each still its own event, traced to its own word
  (I4). *The rule, "the first takes the long":* with more syllables than the host has parts, the
  first take a part each and the rest share the last part equally — three over two is `[a@2 b c]`,
  onsets 0, ½, ¾; with fewer, the first holds the parts the line has no syllable for — two over
  three is `[a@2 b]`, onsets 0 and ⅔, a swung pair. The syllable opening the beat keeps the host's
  downbeat whatever its stress (stress stays the gain), and the crowding falls at the end of the
  beat, where the next onset already marks time. The result lies on the host's grid or a
  subdivision of it, so it never crosses the host. Left alone: the host itself; a beat in which the
  host is silent (nothing to fit to); a placement off the slot's grid — a loop squeezed into a
  shorter bar or stretched across a longer one, whose beats fall between the host's; and two
  stacked lines with each other where neither crosses the host.

The fit is decided in the core, slot by slot (`prosody_core::stack`, through `Score::placed_in`),
not in an emitter: the Strudel pattern and the page's trace (one walk), the OSC schedule and the
scorer all read each event's place in its beat from it, so the scorer measures what is played. A
syllable can now last more than one part of its beat (`Subdivision::parts`), which the Strudel
pattern writes with mini-notation's `@` inside the beat (checked against `@strudel/mini` 1.2.6:
`[bd@2 bd bd]` sounds at 0, ½ and ¾ of its beat). The score's events are unchanged — only where a
line sounds under another does that line's beat shape its timing. **Disclosed against I2:** under
`straight` a stacked line's onsets within a beat depend on its host's feet, and the host can be in
another stanza (a refrain or call from stanza *m* under stanza *n*'s line), so an edit to stanza
*n* that changes a host's feet re-times stanza *m*'s line *within the beat* where it sounds under
that host. Its events, beats, voice, seed and accents are untouched, and every I2 property is about
those; under `crossing` nothing crosses a line's boundary. The one-word edit test
(`prosody-score`'s sensitivity) reads each line's fitted onsets slot by slot, so it counts that
re-timing as a change outside the edited stanza: *local* falls under `straight` (Theocritus 29 → 18
of 61 edits), while *explained* — and so legibility — does not, because the re-timed line sounds
under the edited one. The pattern's header names `straight` and counts the beats it fitted; the
explain view prints the same count (each fitted beat of a slot once, however many lines were fitted
in it, `Score::straightened_beats`) and lists each line's fitted beats.

**Function words** (*decided 2026-10-23 by the user: full stays the default, light is a variant
to compare*). "Stress → accent" takes the dictionary's stress, and `CMUdict` gives most
one-syllable function words primary stress — "or" `AO1 R`, "thy" `DH AY1`, "what" `W AH1 T` — so
they are struck as hard as the content words beside them, often off the beat: The Tyger's "What
immortal hand or eye" strikes "or" like "hand". Lexical stress is not the line's stress. How hard a
syllable is struck decides which syllables are accents, so this is structure, a selector,
`Mapping::Function` (`function-words`), with no `off` (a word is always struck somehow; its first
variant is what preceded it):

- **`full`** (the default) — every syllable at its dictionary stress, as before.
- **`light`** — a one-syllable function word with primary stress is struck at secondary force
  (2/3, "a light accent") unless the meter's own scansion puts it on a strong position. Nothing is
  re-scanned: a position is strong where its foot's template is stressed (the second syllable of an
  iamb, the first of a trochee, a single's only one), so "of" in "In the forests of the night",
  the strong half of an iamb, keeps its accent. The one foot read further is the spondee, which the
  meter makes exactly where a dictionary stress on a function word meets a content word ("hand
  or"): a beat takes one accent, so the spondee's strong position is its first in a falling line
  (more trochees and dactyls than iambs and anapests), its second in a rising one, and in a line
  with neither the content word's (both function words: the one on the beat). "What immortal hand
  or eye" falls, so "or" is light; "Did he who made the Lamb make thee?" rises, so "Did" and "who"
  are light and the rhyme word "thee" keeps its accent.

Which words are function words is the lexicon's to say (`Lexicon::is_function_word`, §9): a fixed,
written-out English closed class — articles, pronouns with the archaic thou/thee/thy/thine,
possessives, conjunctions, short prepositions, auxiliaries — in `prosody-lexicon`, never in the
core, and a second language's lexicon brings its own. Left out on purpose: negatives, quantifiers,
the adverbs that double as conjunctions (`so`, `when`, `where`), stressed particles (`up`, `out`),
`there`, and `art` (The Tyger's "what art" is a noun). A word of more than one syllable keeps its
dictionary pattern. Prešeren's Slovenian matches none of the English list, so `light` changes
nothing in it — the §9 limitation, visible.

It is decided once, in the lowering, right after the rhythm and before anything reads a velocity,
so every consumer reads the same value: the Strudel gains, the OSC velocities, the scorer's accents
(primary force), and the bass — the height bass plays primary-stressed foundation syllables, so a
light function word loses its bass note with its accent (The Tyger: "or", "Could", "thy", … no
longer carry one). No event moves, is added or removed. Locality holds (I2): the decision reads a
syllable's own word and its own line's feet, nothing else. The pattern's header names `light` and
counts the syllables (`function-words = light: … in 24 syllable(s)` for The Tyger), as do the OSC
schedule and the explain view, whose rows credit those gains to `function-words`.

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
  like a chorus dropping in. A real *chord progression* would be the other natural overrule; the
  harmony layer below now exists as a candidate, but it does not overrule anything — its chords
  follow the foundation's loops.

Bar-locked: each bar of the piece takes its length from the stanza of the line arriving in it — the
stanza's *period*, its longest line — so a stanza is a loop of one length and the meter changes only
where the stanza does (*machine cycle 2, pending review*; before, a bar took its own line's length).
A line that fits the bar keeps one beat per foot and rests for the remainder; a longer loop is
squeezed into the bar.

**The outro** (*machine cycle 3b, pending review*). When the poem runs out nothing has stopped the
loops, so they play one more pass — the last stanza again, with the other layer and every lane
under it — and then the piece ends. A one-stanza poem therefore loops once.

**The call lane** (§4.1's second tier, decided the same day). The half-line before a line's first
internal caesura — its *call*: "Tyger Tyger," — is lifted out once its line has been heard, and
loops in a third lane under everything until the next line with a caesura is first heard. A refrain
stanza stops it with the other loops. Caesurae are sparse, so each one leaves a figure that
persists; this is what lets a poem whose stanzas share one layer sound more than one thing at once.

**The held lane** (decided the same day). A line whose rhyme is answered later in its own stanza
keeps sounding until the answer arrives, and stops there: an open rhyme is a suspension, and its
answer resolves it. ABAB holds each line under the next; ABBA holds the A line under both Bs. An
answer must come within the stanza, so a class of one, or a rhyme answered stanzas later, is never
held.

**The bass** (*machine cycle, pending review*). Every primary-stressed syllable of a foundation
line is pitched by its vowel's height: open → root, mid → third degree, close → fifth. Close vowels
carry a higher intrinsic pitch, so the riff follows the vowels; a rhyme word keeps §4.5's note.

**The walking bass** (*the jazz plan's step 4, two candidates approved for comparison; neither is
the default*). `Mapping::Bass` is a selector: `off`, **`height`** (the default — the bass
above), and the proposal's two walks. A bass note is its own field of an event, beside the
melody's note, so under a walk a rhyme word sounds both; `height` keeps its old rule and leaves a
rhyme word to the melody, so the default piece is unchanged.

- **`syllables` (W1) — the syllables walk.** Every syllable of a foundation line sounds a note,
  traced to its word (I4). A primary-stressed syllable keeps its height note. The last syllable
  before a stressed one is a **chromatic approach**: a semitone below that stressed note. Any other
  unstressed syllable between two stressed ones is a **passing tone**, a scale step at a time from
  the first stressed note towards the second, holding one step short of it (between two equal notes,
  the upper neighbour). *Choices the proposal left open, made here:* an unstressed syllable before
  the line's first stressed one climbs into it by step (the **lead-in**), one after the last steps
  down from it (the **tail**), so every syllable has a note; secondary stress counts as unstressed
  (the height bass follows the primary accents only); and a line with no primary-stressed syllable
  has nothing to walk between, so no bass.
- **`feet` (W2) — the feet walk.** One note per foot, on its first syllable — on the beat — held
  for the beat and traced to the whole foot (I4, coarser than a syllable). The foot's stressed vowel
  (its most stressed syllable's, the first on a tie) chooses the **chord tone** by height: root,
  third or fifth of the bar's chord when the harmony layer gave the bar one, of the scale (degree 0)
  otherwise. Its accent is that syllable's. *The proposal's direction rule, as built:* chord tones
  sit in one octave in height order, so a higher next vowel already makes the move to it go up;
  where the foot's tone would repeat the walk's last note, it **steps** instead — up if the next
  foot's vowel is higher, down if it is lower, the same, or there is no next foot. A walk never
  looks past its line.
- **One walk a bar.** A line sounding under another — held, refrain, call — keeps its drums and
  its melody, but under a walk only the bar's foundation-layer line (the one its chord comes from,
  `Score::sounds_bass`) sounds its bass. Two or three walks stacked in one bar landed their passing
  and approach tones a semitone apart (The Tyger under `syllables`: 59 of 257 bass onsets), where
  two `height` notes are chord tones and only ever agree — so `height` still sounds under every
  line, as before. A line's explain view says where it walks and where it is silent.

Spans: a W1 note's is its syllable's word, a W2 note's its foot. Register is palette: the root of a
W2 chord is folded into one octave of the scale, as the chord lane folds it, and a step below the
root sounds below it. **Disclosed against I2:** a walk stays inside its line, so an edit elsewhere
reaches a line's bass only through what it already read — its stanza's layer (§4.1), which of its
words are rhyme words (`height` only), and under W2 with the harmony on, its bar's chord, whose
function follows the stanza's rhyme pattern and whose root is §4.5's poem-wide numbering. The I2
property is scoped to that: when the stanza's layer and rhyme pattern are unchanged, which
syllables carry a note, on which step, by which rule, are unchanged; the root is not compared. The
OSC schedule sends a bass note as the message's pitch where there is no melody note, struck at the
bass note's own accent (under W2, its foot's stressed syllable's, as the Strudel backend strikes
it), and says how many bass notes under rhyme words it cannot send (a message has one pitch). One
message is both the syllable's drum hit and its pitched note, so it carries two accents: `gain`,
the drum's, is always the syllable's own stress (§4.2) — a walk never moves it — and a last
argument, the note's.

**The refrain lane** — §4.3's "literal loop of that voice" at line scale. A refrain line, once
heard, loops in its own lane until the next refrain line is first heard and takes over. No lane
doubles a voice already sounding in the bar. A refrain stanza stops both lanes with every loop.

**The harmony layer** (*sound and voice step 5, candidate approved for comparison, off by
default*). Every bar of a **foundation** stanza gets one chord, held for the bar, in a chord lane of
its own — the one exception being the turnaround's bar under `cadence` and `both`, below, which
holds two, one after the other. Its span is the rhyme word of the line the bar hosts — the line's
last pronounceable word (I4). The lowering decides which bars have a chord, on which degree, with which function; the
emitter voices it (palette, §5). A chord is a fact about its line, and sounds wherever the
arrangement plays that line in the foundation layer — stated, arriving, yielding, looping under a
flourish host, repeating in the outro — and never in the call, held or refrain lanes, so a bar has
at most one chord. Three variants of `Mapping::Harmony`, from `PLAN.md`'s proposal:

- **`rhyme` (H1) — the rhyme scheme is the harmonic rhythm.** A line's chord is I on its rhyme
  class's degree (§4.5's numbering, so the chord's root is the rhyme note's degree): ABAB →
  i–ii–i–ii, AABB → i–i–ii–ii. A line that rhymes with nothing has no degree, so its bar has no
  chord.
- **`cadence` (H2) — rhyme closure is tension and release.** A line whose rhyme is answered later
  in its stanza (the held lane's test) is open: V of its degree (*d*+4). Its answer closes: I on the
  degree. A line that is both — the middle A of AAA — is open, since its class is still unresolved.
  A line with no rhyme within its stanza is IV of the stanza's key (*k*+3), the key being the degree
  of the stanza's first in-stanza rhyme, or 0. The stanza's last bar (of two or more) is **split**
  (the user's decision, `PLAN.md` "the cadence's ending"): for its first half, the chord those
  rules give it — I when the line closes a rhyme, so ABAB, AABB and ABBA land on I, and IV when it
  has no rhyme in its stanza (decided: the user's rule is "its own chord first", and IV–V into the
  loop's first chord is a cadence too, so an unanswered last line keeps its IV rather than a whole
  bar of V); from the bar's half, the **turnaround**: V of the stanza's own first chord. The chord
  it resolves to is the loop's first bar, played next on every pass. The half is the bar's, in
  time: two beats into a four-beat bar, one and a half into a three-beat one. A one-line stanza has
  no turnaround (its last bar is its first), nor does a last line with no pronounceable word, nor a
  stanza none of whose earlier lines has a chord to lead back to. **The turnaround's span is the
  last line's rhyme word**, the line whose bar it shares: every chord of a bar traces to the line
  that bar plays, and the first line, which gives the turnaround its root, is named in its reason
  (`Reason::Turnaround { onto }`) rather than in its span.
- **`both`** — H2 where it applies (open and closing lines, the turnaround), H1 in every other bar;
  so a line rhymed only across stanzas takes I on its degree, and an unrhymed line no chord — a
  stanza's last line that rhymes with nothing rests for the first half of its bar, then turns
  round.

Under the walking bass's W2 (a note a foot on the chord's tones), a foot of the split bar walks on
the chord the lane is sounding where it begins: the bar's own chord before the **bar's** half
(`2 · beat < beats`, `beats` the bar's), the turnaround from it. The bar is the one the line is
stated in — under the stanza period its stanza's longest line, so a last line a foot shorter than
the rest still turns round where the lane does, not at its own half; without the period the line's
own length, and a line stretched under another host keeps its half where that bar's is. A walk is
decided once per line, so one case is left: under the period, a stanza's last line looping under
a flourish stanza whose period is not its own's can turn round a foot away from the lane. None of
the shipped poems reaches it: under cadence and both every W2 foot of a split bar walks on the
lane's chord, 57 of them under another host. Fixing it means a walk per slot, not per line.

A flourish stanza carries no harmony. Degrees are counted like a rhyme class's and may run past
the scale's length (V of degree 5 is degree 9); the emitter folds a root into one octave. The
proposal's turnaround resolved onto the *next* stanza's first chord, which would make stanza *n*'s
last bar depend on stanza *n*+1's first rhyme; this one loops back onto its own stanza, so it never
crosses a stanza boundary. **Disclosed against I2:** a chord still depends on §4.5's poem-wide
degree numbering (as the melody's note does), and on the rhyme analysis, whose classes are
poem-wide — an edit elsewhere can give a line a partner in another stanza (H1 then gives it a
chord) or merge two of a stanza's classes through a third word. The I2 property for the harmony is
scoped to exactly that: when a stanza's rhyme pattern and layer are unchanged by an edit to another
stanza, its harmony — which bars have chords, which function by which rule, which rests and why,
and which bar is split — is unchanged. The split adds no dependency: the turnaround still reads
only its own stanza. The OSC schedule has no chord message, and says how many chords it leaves out
and how many of those are turnarounds.

**The melody over the harmony** (*the user's decision, 2026-10-23*). With any harmony variant on,
each rhyme word's note (§4.5) sounds as a **single note** on top of the chord lane, whatever
`voicing` (§5) says; the voicing then shapes only the chord lane. Under the jazz palette the melody
and the chord lane had both played four-tone chords, which was thick. With the harmony off the
voicing works as before. The single note is the lowest tone the chord had — the event's own degree
— at the single note's level (the whole `melody_gain`, not a share of it). This is realisation, not
a new mapping: the emitter reads it from the score (whether the harmony mapping ran, which the
score records), never from the style, and no event is added, moved or removed (I3; a property
checks that under the harmony `note` and `seventh` differ only in the chord lane). It asks whether
the mapping is on, not whether any bar has a chord, because the second is a fact about the whole
poem: an edit giving one stanza the poem's first chord would revoice every other stanza's melody,
which I2 forbids. So a poem under the harmony with no chord anywhere (every bar resting) still has
single notes. The pattern header (`melody on … as single notes over the chords`) and the explain
view (`rhyme notes   single notes over the chords …`) say so.
*Open (register):* "on top" is implemented as a layer — the single notes sound above the chord
lane in the arrangement — not as a register. The chord lane stacks its tones from an octave below
the melody's octave, so a seventh's upper tones reach into it (in C dorian the lane spans MIDI 48 to
69 and the melody starts at 60): under the jazz palette, Wilde's melody often sounds inside the
chord, sometimes in unison with a chord tone (melody 60 against `[53,57,60,63]`). Moving the chord
lane down, or the melody up, under the harmony would be palette (register moves no event), but
whether "on top" was meant as register is the user's to decide.

**Auditioning the candidates.** Every mapping above marked *pending review*, the held and refrain
lanes, and §4.7's reading are *selectors* (`prosody_core::mapping`): each has a fixed, ordered list
of named variants, always including `off` (the pulse and the stack excepted, below), and a default. A `Mappings` value chooses one variant
per mapping, and only a variant the mapping lists can be chosen. Today the held and refrain
lanes, the stanza period and the outro are `off`/`on`, default `on`; the bass is
`off`/`height`/`syllables`/`feet`, default `height`; the harmony is `off`/`rhyme`/`cadence`/`both`,
default `off`; the reading is `off`/`arrives`/`stanza`, default `arrives`; the pulse (§4.2) is
`elastic`/`steady`, default `steady` — a mapping without `off`, because a piece cannot be
without a pulse: its first variant is what preceded it, `elastic`, and leaving it out means that;
the stack (§4.2) is `crossing`/`straight`, default `crossing`, likewise without `off` — leaving it
out means `crossing`.
They are
chosen with `--set name=variant,…` on the CLI (`--without LIST` is shorthand for `name=off`),
`set=name:variant,…` (and `off=`) on the web endpoints, and one menu per mapping per side in the
page's A/B table, so the piece can be heard and measured under each choice. This is a separate
argument to the lowering, not part of the style: it changes structure, and I3 keeps the style out
of structure. Every invariant holds within one choice. Every default is the piece as specified,
and the default everywhere — except the harmony, a candidate whose default is `off`, so a piece
that does not choose it is the piece it was; likewise the bass's walks are variants beside its
default, `height`. Left out, each falls back to what preceded it: no held or refrain lane,
no bass, a bar as long as its own line with everything under it stretched or squeezed to fit, and
a piece that ends as the last stanza arrives. A mapping joins the list when a design iteration
proposes it, so the comparison stays available as long as the question is open.

This is structure, read from the poem, not a style parameter — which answers §9's "looping as an
explicit style parameter" question: it is not one, and I3 is untouched. It does make *duration*
poem-wide: how long a stanza sounds depends on when the next stanza of its layer arrives.

### 4.7 The reading — the poem spoken where each line arrives

*Candidate, page-side, pending review (sound and voice, step 2).* A prototype to learn whether
spoken lines over the music work. Each line is spoken once, by the browser's speech synthesiser,
in the slot where it is **first heard as a host** — `Stated`, `Arriving` or `Overruling` (§4.6) —
and never where it loops, yields, calls, holds, refrains in its lane or repeats in the outro. So
the reading follows the poem in order, and every utterance is the source text under one line's
span (I4). A refrain stanza is new lines of the poem, so it is read again where it drops in.

This **is a new mapping** — it adds sound events, the spoken words — and so it is structure, not
palette: no style field can move a cue (I3). It is decided entirely in the emitter and the page,
from the arrangement the core already made, and changes nothing the core produces:
`prosody_emit::reading` plans it as a separate artefact (a cue pattern and the lines it speaks),
not part of the Strudel pattern text the CLI prints. It is a candidate selector in
`prosody_core::mapping`'s list all the same — `Mapping::Reading`, chosen per side on the page like
the others — so it can be compared and switched off with the same controls. The lowering never
reads it. Its variants (*sound and voice, step 4, candidate*):

- **`arrives`** (default) — each line once, in the slot where it is first heard as a host, as
  above.
- **`stanza`** — a stanza's lines are read one per bar, in order, from the stanza's first hosting
  slot (the earliest slot `arrives` would read any of its lines in), instead of each line waiting
  for its own arrival. A stanza that transforms a running loop brings one new line in per pass,
  so under `arrives` its lines are read a loop's length apart (five bars in The Tyger); under
  `stanza` it is read straight through while the music is still turning into it. Cues never run
  backwards or share a slot: a stanza that would start before the previous one has been read
  waits for it, and a line that would fall after the piece's last bar is not read. It decides
  from the same arrival slots as `arrives` (which already depend on earlier stanzas, §4.6's
  "duration is poem-wide"); the wait would add a dependence on the previous stanza's length, but
  only where a stanza starts before the previous one has been read, which today's arrangement —
  one stanza stated or transformed at a time — never does. Cues are not voices, so I2's voice
  property is untouched either way.
- **`off`** — no cues.

The cue pattern's first comment names the variant (`reading = stanza`); the explain view lists it
when it is not the default. *How fast* a line is read is a
parameter, fitted from the palette's tempo: the rate at which the line's syllables, at four a
second at rate 1, fill 85% of its bar, bounded to 0.6–1.6. The voice is the listener's choice. A
light duck (the music to 0.6 while a line is spoken) is page-side and optional.

Two things it does not promise. The audio of the speech is the browser's, so it is not
bit-identical across machines — the *cues* are (I1). And speech is not on the grid: it starts on
the bar's first beat and runs at its own pace; a line still speaking when the next is due is cut
off rather than allowed to drift.

"In order" holds from the first bar. Strudel's scheduler keeps its cycle count when it is handed a
new pattern, so a change while playing would start the new piece partway through. The page
therefore restarts the clock at bar 1 when the poem is replaced by a preset or the listener
switches A/B, and reads from the first line. A live edit keeps the clock running, because the
writer is listening in place: the reading carries on from the bar the clock has reached, and any
line before that bar waits until the arrangement loops round. A line already queued or being
spoken from the previous version is dropped when the new one starts. The cue pattern's comments
escape line terminators and control characters (a pasted U+2028 would otherwise end the comment
early), and the page evaluates the cue pattern separately, so a reading it cannot cue never stops
the music.

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

*Extended 2026-10-22 (sound and voice, step 1).* The palette also names an instrument for each
pitched role — `bass` and `melody`, a synth name above or a General MIDI soundfont (`gm_epiano1`,
`gm_acoustic_bass`, …); `synth` still sets both — and a mix, every amount range-checked at the
boundary: `reverb`, `room_size`, `delay` (melody only), `high_cut` (the top of §4.4's brightness
range, scaled rather than clipped), `drive`, `compress`, `humanise`, and a level per role
(`drums_gain`, `bass_gain`, `melody_gain`). `preset = "default" | "warm" | "raw"` starts from a
named palette. A role's level is never zero (`0.1..=2`): a muted role would leave its events in
the score and inaudible, which I4 forbids as surely as a fully closed filter. The register a role
plays in was at first fixed in the Strudel backend (bass MIDI 36, melody 60); it is now palette —
see the step 4.2 extension below.

`humanise` is the one palette value that draws randomness, and it obeys I2: each syllable's stray
(up to 30 ms late, up to 30% softer, scaled by the amount) comes from its *line's* voice seed under
its own child stream, restarted per bar, so a line strays identically wherever it sounds and an
edit elsewhere cannot move it. It changes when an event sounds by milliseconds and how loud; it
cannot add, remove or reorder an event.

*Extended 2026-10-22 (sound and voice, step 3 — jazz, the palette part).* Two more palette keys.
`kit = "machine" | "ride"` names which drum carries each layer of §4.1: kick and hat, or a ride
keeping the time on the foundation and the snare comping on the flourish. `voicing = "note" |
"seventh"` voices each rhyme note (§4.5) as one tone or as the seventh chord on its degree — the
degree with every other degree of the style's scale above it, *d*, *d*+2, *d*+4, *d*+6, sharing the
melody's level between its tones. This is palette and not harmony: each chord is the one event of
one rhyme word, at its onset, with its gain, cutoff and stray; it is built from that event's degree
and the scale alone, never from a neighbouring chord, stanza or bass note, so it cannot express a
progression (I3; the argument is on `prosody_emit::Voicing`, and a property test checks that the
lowest tone of every chord is the note `note` voicing plays and that nothing else moves). A
harmony layer — a progression derived from the poem — would be a §4 mapping, and is a proposal in
`PLAN.md`, not part of this. A fourth preset, `jazz`, is the first that sets more than the sound
and the mix: Rhodes sevenths over an upright bass, the ride kit on the sampled acoustic `r8` bank
(no brushed kit exists in the sample map), triplet swing `0.33`, dorian, a warmer room. Its
scale, swing and bank are palette like the rest; it simply moves notes where the other presets
never do, and any header key still overrides it. A bank without a ride (the TR-808 and TR-707
samples have none) keeps time on its open hat under `kit = "ride"`, and the pattern's header says
so: a missing sample would otherwise silence the whole foundation, which no palette may do. In a
scale without seven notes, `seventh` still stacks every other degree — fourths in
`pentatonic_minor` — and the header names it as that rather than as a seventh. *Since
2026-10-23, under any harmony variant the melody is single notes whatever the voicing, which then
shapes only the chord lane (§4.6, the melody over the harmony).*

*Extended 2026-10-22 (sound and voice, step 4.2 — palette the user asked to be able to change).*
Three more keys, every one palette:

- **`bass_octave`** (`1..=3`, default `2`, MIDI 36) and **`melody_octave`** (`3..=5`, default `4`,
  MIDI 60) — the octave each pitched role plays in, in scientific pitch notation (MIDI of its C =
  12 × (octave + 1)). Whole numbers only. They move every note of a role by octaves and no event;
  the ranges meet at octave 3 and never cross, so the melody is never under the bass. The Strudel
  backend reads them; OSC still sends every pitch from 36 (it does not split the roles), and
  says so: a schedule whose header moves either key, or `density_level`, carries a
  `# not applied here: …` line, and `--send` prints the same on stderr.
- **`ride_fallback = "open_hat" | "crash"`** (default `open_hat`) — what keeps time under
  `kit = "ride"` on a bank with no ride sample (TR-808, TR-707). The stand-in strikes exactly where
  the ride would; the `plays:` line of the pattern's header names it ("crash in place of the ride,
  which RolandTR808 does not have"). A bank with a ride ignores it.
- **`density_level`** (`0..=1`, default `0`, today's fixed levels) — levels that follow density.
  Every gain in a bar is multiplied by *n*^−`density_level`, where *n* is the number of lines
  sounding in that bar (every placement of the arrangement that plays at least one event): at
  `0.5` a bar of *n* lines is divided by √*n* (the power of *n* uncorrelated lines held at one
  line's), at `1` by *n* (their summed peak, when their hits coincide). A bar of one line is never
  changed. This is the one palette value that *reads* the score's structure; it changes only gain
  values — no event, onset or pattern step — and the I3 property test varies it and checks that
  the output differs from `0` in `.gain` values alone, each no louder. **Disclosed against I2:** a
  line's level in a bar it shares depends on how many other lines share it, so with
  `density_level > 0` an edit to stanza *n* that changes how many lines sound in a bar (a stanza
  that loops longer) changes the *level* of stanza *m*'s line in that bar. Its events and its voice
  are untouched — I2's property tests are about the score and still hold — but the emitted gains
  of a shared bar are not stanza-local. At the default `0` nothing crosses. The `warm` and `jazz`
  presets set `0.5`; `default` and `raw` keep `0`.

The pattern header lists these four keys only when they differ from their default, so a pattern
that does not use them reads byte for byte as before; when `density_level` is set, the header also
names the factor for each line count the piece has. The explain view always lists them.

*Extended 2026-10-22 (sound and voice, step 5 — the harmony layer's palette).* Two more keys,
both palette: **`chord_sound`** (any instrument `bass` and `melody` take; `gm_pad_warm` by default,
`gm_epiano1` under `warm` and `jazz`, `triangle` under `raw`) and **`chord_gain`** (`0.1..=2`;
0.40, `warm` 0.35, `raw` 1, `jazz` 0.45 — estimates, not measured mixes). They colour the chord
lane of §4.6's harmony layer and nothing else. The chord itself is voiced by `voicing`: under
`note` the triad on the chord's root (one tone would be no chord), under `seventh` the seventh
chord, every other degree of the style's scale either way, an octave below `melody_octave`, its
level shared between the tones like the melody's and times the bar's `density_level` factor, with a
soft attack and release. No palette value can add, move or remove a chord; an I3 property checks
it. The pattern header lists the two keys, and every line's chord, only when the piece has a chord.

The chord's numerals (I, IV, V) are counted in seven steps — V is four degrees above its root, IV
three — so they name a dominant and a subdominant only in a seven-note scale. Under
`pentatonic_minor`, `whole_tone` or `chromatic` the same steps land elsewhere (in five notes "V"
folds onto the degree just below the root), and the header says so (`… has 5 notes, not 7, so the
numerals below count steps of it, not functions`) and calls the chords "three-tone chords on every
other degree" rather than triads. Which bars have a chord and on which degree stays the poem's.

The root sounds folded into one octave of the scale; the scorer, which reads the score before any
palette, cannot know that fold, so a chord's root is left out of its pitch entropy (the melody's
alone) — counting the unfolded roots credited the harmony with variety nobody hears. A chord still
counts as pitched and as an instrument of its own.

*Fixed 2026-10-22 (step 5 review): swing is applied to the bar.* `swingBy(x, n)` slices a cycle
into `n`; `.cpm` is `_fast(cpm / 60)` (`@strudel/core` 1.2.6), after which a cycle is the
scheduler's second, not a bar. The pattern chained `.cpm(c).swingBy(x, n)`, so swing sliced the
second — onsets moved to irregular times, and a held chord could be skipped or struck twice. It is
now `.swingBy(x, n).cpm(c)`: the bar's beats are swung, then the bar is set to the tempo.
*Since the steady pulse (§4.2) a cycle is a beat, so `n` is 1 and every beat is swung whatever its
bar's length; under `elastic` `n` is still the mean bar length, which slices a bar of another
length across its beats.*

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
- **Very short poems** produce very little structure. Define minimum viable input, or make looping/development an explicit style parameter (which arguably violates I3 — think it through). *Looping is now structure (§4.6), not style. A single-stanza poem loops once, in the outro (machine cycle 3b, pending review).*
- **Aesthetic risk.** The mapping can be perfectly principled and still sound bad. Budget time for tuning the *ranges* (which is palette work, and legitimately belongs in the style layer).

---

## 10. Stack suggestion

Rust. Tree-sitter for the grammar with incremental reparse. Pure analysis passes over an immutable CST, all IO at the edges. Strudel/Tidal text emit as the first backend for zero-cost auditioning; OSC → SuperCollider once the mappings hold up. CLI first, single responsibility — parse in, pattern out — with the live-reload watcher as a separate concern.

Open question worth deciding early: whether the CST → Voice IR lowering is one pass or several. Several is easier to test and to attribute (I4), and cheap enough at poem scale.
