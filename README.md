# prosody

**A poem is the source code for an electronic music track.** Not a prompt, not a lyric sheet — a
*score*. The text's own properties are parsed and compiled into layered musical structure: meter
becomes rhythm, rhyme becomes melody, repetition becomes arrangement, and lexical rarity becomes
timbre.

The design goal everything else serves: **editing a line for poetic reasons must simultaneously be
a musically meaningful edit.** Tighten a stanza's metre and you tighten a loop. Add a refrain and
that instrument literally returns. Swap one word for a rarer synonym and that syllable gets
brighter — and nothing else in the piece moves.

The failure mode it is built to avoid is a random generator with a text-shaped seed: output that
sounds fine but where no audible feature traces to any specific word. Every event this produces
resolves to a byte range in your poem, and `--explain` will show you which.

See [`SPEC.md`](SPEC.md) for the design, [`PLAN.md`](PLAN.md) for the build log and the open
questions, and [`CLAUDE.md`](CLAUDE.md) for the engineering rules.

---

## Quick start

Needs Rust 1.96.1 (pinned in `rust-toolchain.toml`; `rustup` will fetch it automatically).

```sh
git clone <this repo> && cd prosody-experiment
cargo build --release

# compile a poem to Strudel pattern text
cargo run -p prosody-cli -- examples/dickinson-because.poem
```

You get something like:

```js
// compiled from a poem by prosody
// 36 bars from 12 lines, 12 voices, 255 events
//   bank = tr909
//   root = c
//   scale = minor
stack(
  s("<[[bd bd] [bd bd] [bd bd] [bd bd]] ~ ~ ~ …>").gain("<[[0.333 1.000] …>")…,
  note("<~ ~ [~ 36] ~ …>").sound("triangle")…
)
  .cpm(40.333)
```

Each `stack` layer is one voice; each `<…>` element is one **bar of the piece** — a line, or a loop of
earlier lines under it; each step inside
it is one **foot**; each bracketed group inside that is the **syllables** of that foot.

---

## Hearing it

Three ways. The first is the one to start with.

### Path A — the local web page (easiest)

```sh
cargo run -p prosody-web
# prosody: http://127.0.0.1:4747 — ^C to stop
```

Open it. You get the poem on the left with every palette control under it, the generated pattern
and the annotation on the right, and Play/Stop at the top. It recompiles as you type — about a
third of a second after you stop — and if it is already playing it switches to the new version
without stopping, so you can rewrite a line and hear the difference immediately.

The palette controls are the whole of `SPEC.md` §5: scale, root, tempo range, swing, sample bank,
the bass and melody instruments, and the mix. Changing one of them changes what you hear and
provably not what happens — the page recompiles and the bar, voice and event counts stay put,
which is invariant I3 in front of you.

The **Sound** menu picks a named palette for the instruments and the mix, leaving scale, root,
tempo and bank alone (except **jazz**, below): **default** (electric piano over acoustic bass, a modest room, a little echo
on the melody, light compression, a small human stray), **warm** (vibraphone over fretless bass, a
long room, more echo, a lower high cut, a touch of drive, more stray) and **raw** (the palette as
it was before any of this: triangle oscillators, dry, on the grid) and **jazz** (Rhodes playing
each rhyme word as the seventh chord on its degree, an upright bass, the ride keeping time and the
snare comping on the sampled acoustic R-8 kit, triplet swing, dorian, a warm room — the one
palette that also sets the scale, swing and bank). Move any control and it reads *custom*. **Kit**
picks kick-and-hat or ride-and-snare for the two drum layers, and **Voicing** plays each rhyme word
as one note or as a seventh chord built in the current scale; neither moves an event. **Ride
stand-in** picks what keeps time under the ride kit on a bank without one (open hat or crash), and
**Bass octave** and **Melody octave** move each role's register by whole octaves. **Levels follow
density** (in the mix) turns down the bars where several lines sound at once — a stanza looping
under the next, a held line, a call — so dense poems stop leaning on the page's soft clipper;
warm and jazz set it to 0.5. The mix controls: reverb, room size, echo on the melody, high cut, drive, compression,
humanise, and a level each for the drums, the bass and the melody. **Humanise** makes each
syllable a few milliseconds late and a little softer, drawn from that line's own seed — so the
same poem always strays the same way, and editing one stanza never changes another's feel.

The **example poems…** menu at the top loads any poem in `examples/`, together with the palette its
header asks for (the defaults when it has none).

Under the palette is **Mappings — changes what happens**: the candidate mappings still under
review (the held and refrain lanes, the bass, the stanza period, the outro, the harmony, the
reading), each a selector of named variants — always including `off` — with one menu per setting,
A and B. The held and refrain lanes, the stanza period and the outro are `off`/`on`; the bass is
`off`, `height` (its default), `syllables` or `feet` (see [the walking bass](#the-walking-bass));
the harmony is `off` (its default), `rhyme`, `cadence` or `both` (see
[the harmony layer](#the-harmony-layer)); the reading is `off`, `arrives` or `stanza`.
A starts with every mapping at its default (marked "(default)" in the menu), B with every one off; a menu not at its default
has a bold border. The **hearing A | B**
switch at the top swaps which one plays, without stopping, so you can hear exactly what a mapping
adds. The **Score A vs B** tab measures both settings side by side (see [Measuring it](#measuring-it))
with the difference between them, under the machine, jazz or interest profile. The page remembers
your two settings.

Under the mappings is **Reading — the poem spoken over the music**: the voice and the duck. Whether
and when lines are read is the `reading` mapping above. Under `arrives`, the default, each line is
spoken once, in your browser's own voice (`speechSynthesis`), in the bar where it is first heard —
its stanza stated, dropping in as a refrain stanza, or replacing a bar of the old loop — and never
where it only loops, is held, refrains in its lane or repeats in the outro. Under `stanza`, a
stanza's lines are read one a bar from the first bar any of them is heard in, so a stanza that
transforms a loop (one new line a pass, five bars apart in The Tyger) is read straight through
while the music is still turning into it. Under `off`, nothing is read. Strudel's own clock
fires each line, so it stays on the bar; a line still being spoken when the next is due is cut
off. The speaking rate is fitted so the line fills most of its bar at the piece's tempo, within
0.6–1.6. Pick a voice from the menu (the page remembers it), and untick **duck** to stop the music
dipping while a line is read. The Pattern tab shows the cue pattern under the music's, and the
browser console logs every line as it is cued: which bar, which bytes, which rate. Picking a
preset or switching A/B while playing starts the piece again from bar 1, so the reading begins at
the first line. A live edit keeps playing from where it is, and the reading carries on from there.
It is a
candidate mapping and a page-only one — the CLI's pattern and strudel.cc know nothing of it
(`SPEC.md` §4.7).

It binds to `127.0.0.1` only, and deliberately: it compiles arbitrary text from a request body,
which is exactly right on your own machine and exactly wrong on a network. Playback loads Strudel,
the drum-machine samples (the map strudel.cc uses) and the General MIDI soundfonts from CDNs, so
the first Play needs an internet connection; each instrument a pattern uses is loaded before it
starts. The page's output goes through a gentle compressor and then a soft clipper that cannot
exceed 0.95, so a dense bar bends rather than clips.

### Path B — Strudel, by hand

1. Compile a poem and copy the output:
   ```sh
   cargo run -p prosody-cli -- examples/blake-tyger.poem | pbcopy   # macOS
   cargo run -p prosody-cli -- examples/blake-tyger.poem | xclip -sel clip   # Linux
   ```
2. Open **<https://strudel.cc>**.
3. Select everything in the editor and paste over it.
4. Press **Ctrl+Enter** (or the ▶ button). The first run downloads the drum samples, so give it a
   second.
   strudel.cc registers the same drum machines and GM soundfonts this page loads, so the
   instruments sound the same. One difference: strudel.cc honours `.nudge` only for samples, so
   there the humanise stray moves the drums and not the bass or melody (their softening still
   applies); the local page applies it to every sound.

That is the whole loop. Edit the poem, recompile, paste again — or keep a terminal open with
`--watch` and paste whenever you like something.

### Path C — SuperCollider, with live reload

This is the one that re-plays as you type. It needs
[SuperCollider](https://supercollider.github.io/) installed.

1. Open [`examples/prosody.scd`](examples/prosody.scd) in the SuperCollider IDE.
2. Select all and press **Cmd+Enter** (macOS) or **Ctrl+Enter** (Linux/Windows). It boots the
   audio server, defines three synths, and starts listening on port 57120. You should see
   `prosody: listening on 57120` in the post window.
3. In a terminal:
   ```sh
   cargo run -p prosody-cli -- --watch --send 127.0.0.1:57120 examples/blake-tyger.poem
   ```
4. Now edit `examples/blake-tyger.poem` in any editor and save. It recompiles and replays on every
   save.

`--send` implies `--osc`. Without a receiver you can still inspect the schedule as text:

```sh
cargo run -p prosody-cli -- --osc examples/dickinson-because.poem
# 121 bpm, 408 messages
#     0.000  /prosody/note  0.000 "foundation" 0 0.333 -1 0.000 0.688 0.000 1.000 0.333
#     0.500  /prosody/note  0.500 "foundation" 0 1.000 36 0.000 0.688 0.000 1.000 1.000
```

A note's arguments are `beats layer voice gain note detune sharpness family sustain noteGain`.
`gain` is the drum's accent, always the syllable's own stress; `noteGain` is the pitched note's,
which differs under `bass = feet`: the foot's note rides on its first syllable but is struck at
its stressed one. `examples/prosody.scd` reads both.

Onsets are in **beats**, counted from the start of the piece. The tempo is sent once, separately,
because a tempo is palette and the rhythm is not.

The schedule sends every pitch from MIDI 36 and every velocity unscaled, so `bass_octave`,
`melody_octave` and `density_level` do not reach it. A header that sets one gets a
`# not applied here: …` line under the first, and `--send` says the same on stderr. It has no chord
message either: under the harmony mapping the schedule's text says `# not scheduled here: N chords
of the harmony layer`, and `--send` says so on stderr. A message has one pitch, so under a walking
bass a rhyme word's message carries the melody's note, and the text says `# not scheduled here: N
bass notes under rhyme words`.

---

## Seeing what it did

The most useful command in the project. `--explain` renders your poem with every token annotated
by what it became, and by *which pass decided it*:

```sh
cargo run -p prosody-cli -- --explain examples/dickinson-because.poem
```

```
  Because I could not stop for Death —
  ────────────────────────────────────
  bar 0  foundation (regularity)  voice 0  seed 30b40122
    token          beat       gain    pitch   modulation sharp   bank   from
    Because        0.0.0/2    0.33    —       —         0.69    K      meter,surprisal
                   0.0.1/2    1.00                                     meter
    I              0.1.0/2    1.00    —       f1+0      0.19    —      meter,sound,surprisal
```

Read it as: this line is bar 0, plays the *foundation* layer because its stanza is metrically
regular, and is voice 0. "Because" is two syllables — the first on beat 0 at a third of full force,
the second on the offbeat at full force — and its brightness came from the surprisal pass.

If a token shows `no pronunciation` it is silent and the dictionary could not say it. If it ever
shows `attribution lost`, that is a bug: an event exists that traces to nothing.

---

## Measuring it

```sh
cargo run -p prosody-cli -- --score examples/wilde-theocritus.poem
./examples/run.sh --score      # every example
```

prints how the piece measures, as `name value` lines a script can read:

- **interest**: layering, recurrence, how much is pitched, melodic entropy and syncopation, each
  judged against a band rather than maximised. People like moderate complexity, so overshooting
  scores worse too. The bands are provisional.
- **legibility**: every word is swapped for a probe word in turn and the poem recompiled. *audible*
  is how often that edit was heard in its own bar; *explained* is how often everything it changed
  elsewhere had a reason (a shared refrain, rhyme, word family or figure). *local* is the strict
  version, with no change outside the stanza at all.
- **overall** = interest × legibility, so a better sound cannot be bought by ignoring the poem.

It is the measuring half of a tuning loop. The baselines for every shipped poem are committed in
`crates/prosody-score/tests/snapshots/`, so a mapping change shows what it did to them.

`--profile machine` judges against repetitive machine music instead (Kraftwerk, Daft Punk,
psytrance): denser layers, more repetition, short riffs, plus a steady **pulse** and **steadiness**
of bar length. `./scripts/cycle.sh` prints that profile for every example with the mean overall —
the number the tuning cycle in [`PLAN.md`](PLAN.md) works to raise. `--profile jazz` (and
`./scripts/cycle.sh jazz`, and the page's Score tab) judges against small-group jazz: less
recurrence, more scale degrees in use, more accents off the beat, the time still kept — and one
metric of its own, **cross_rhythm**: the share of beats in which two lines sounding together divide
the beat incompatibly (an anapaest's three syllables against an iamb's two, or a loop squeezed into
a shorter bar), so one line's onsets fall off the other's grid. Band 0.1–0.4; the machine profile
does not measure it. It measures the score, before the palette, so swing is neither rewarded nor
penalised — a swung piece scores as its straight twin does. Its bands are reasoned guesses, like the
machine profile's.

`--set name=variant,…` chooses a candidate mapping's variant — `held`, `refrain`, `period` and
`outro` take `off` or `on` (default `on`), `bass` takes `off`, `height`, `syllables` or `feet`
(default `height`), `harmony` takes `off`, `rhyme`, `cadence` or `both` (default `off`), `reading`
takes `off`, `arrives` or `stanza` (default `arrives`) — for the pattern and for `--score` alike;
an unknown name or variant
is an error that names it. `--without held,refrain,…` is shorthand for setting those to `off`.
`--explain` lists every choice that is not a default. The reading is not in the CLI's pattern, so
`reading=…` changes only that list. `./scripts/cycle.sh machine bass` scores every example without
the bass, and `./scripts/cycle.sh machine bass=off,period=off` passes its list to `--set` (any list
containing `=` does); `./scripts/cycle.sh jazz harmony=cadence` scores the harmony.

### The walking bass

*Two candidates beside the default* (`SPEC.md` §4.6, `--set bass=…`, or the `bass` menu on the
page). `height`, the default, is the bass as it was: a note on every stressed vowel of a foundation
line, by the vowel's height. The two walks:

- **`syllables`** — every syllable of a foundation line plays a note. A stressed one keeps its
  height note; the syllable just before a stressed one is a semitone below that note (a chromatic
  approach); the unstressed ones between step through the scale from one stressed note towards the
  next. In verse with one unstressed syllable between stresses, that is mostly approach tones.
- **`feet`** — one note on every beat (foot), held for the beat. The foot's stressed vowel chooses
  root, third or fifth — of the bar's chord when the harmony is on, of the scale otherwise — and
  where that would repeat the last note it steps instead, up if the next foot's vowel is higher.

Under a walk a rhyme word plays its melody note *and* its bass note, and only one walk sounds a
bar: a line held, refrained or called under another keeps its drums and melody but not its walk,
so two walks never clash a semitone apart. The pattern's header names the
walk (`walking bass (mapping bass = feet): …`), and `--explain` adds a `bass` block under each
foundation line saying why each note is what it is (`step down: AH is mid, so the third of IV deg
3 would repeat the last note; the next foot's ER is not higher`), lists the bars it walks in and
the ones it is silent in, and shows each syllable's own bass note in the pitch column. The OSC
schedule sends a bass note where there is no melody note, at the bass note's accent, and counts
the ones it cannot send.

### The harmony layer

*A candidate, off by default* (`SPEC.md` §4.6, `--set harmony=…`, or the `harmony` menu on the
page). Every bar of a foundation stanza gets one chord, held for the bar, in a lane of its own; its
span is the rhyme word of the line the bar hosts, and it plays wherever that line sounds in the
foundation layer — stated, looping under a flourish, in the outro — never in the call, held or
refrain lanes.

| variant | the chord of a line's bar |
|---|---|
| `rhyme` (H1) | I on its rhyme class's degree: ABAB → i–ii–i–ii, AABB → i–i–ii–ii; a line that rhymes with nothing has no chord |
| `cadence` (H2) | a rhyme answered later in its stanza → V of its degree; its answer → I on the degree; a line with no rhyme in its stanza → IV of the stanza's key (its first in-stanza rhyme's degree, or 0); the stanza's last bar → V of the stanza's *own* first chord, a turnaround the loop resolves |
| `both` | H2 for open and closing lines and the turnaround, H1 for every other bar |

The pattern's header lists `chord_sound` and `chord_gain` and every line's chord (`chords by line:
1:V/4 2:I/0 …`, `-` a rest) when there is one; `--explain` shows each line's chord, its degree and
the rule that chose it, or why the bar has none.

The numerals are counted in seven steps, so in a scale of another size (`pentatonic_minor`,
`whole_tone`, `chromatic`) V is not a dominant, and the header says so. `--score` counts a chord as
pitched and as an instrument of its own, but leaves its root out of melodic entropy: the root is
folded into one octave by the palette's scale, which the scorer does not see.

---

## Writing a poem

Any plain text file. Punctuation, capitals and em-dashes are all fine — they are parsed and
ignored rather than requiring you to strip them.

Optionally, a TOML header supplying **palette only**:

```toml
+++
[style]
scale       = "phrygian_dominant"
root        = "D"
tempo_range = [124, 132]
swing       = 0.08
bank        = "tr909"
synth       = "karplus"
+++
Tyger Tyger, burning bright,
In the forests of the night;
```

The header can change every sound in the piece and **cannot move a single event**. That is
enforced by the compiler, not by discipline: `Style` lives in a crate the analysis passes cannot
import. `tempo_range` is a window rather than a tempo — the style says the piece lives between 124
and 132, and the poem says where in that window, from how densely its syllables pack into bars.

Scales: `major`, `minor`, `harmonic_minor`, `dorian`, `phrygian`, `phrygian_dominant`, `lydian`,
`mixolydian`, `locrian`, `pentatonic_minor`, `whole_tone`, `chromatic`.
Banks: `tr909`, `tr808`, `tr707`, `linn`, `casio`, `r8` (Roland R-8, sampled acoustic drums).
Synths: `karplus`, `fm`, `subtractive`, `physical`, `sine`, `sawtooth`, `square`, `triangle`.
`synth` sets both pitched instruments; in Strudel the four model names play the recorded
instrument nearest the model (Karplus–Strong: acoustic bass and nylon guitar; FM: DX-style synth
bass and electric piano; physical: acoustic bass and marimba), and the waveforms stay oscillators.

The rest of the header, every key optional:

| key | what | range | default |
|---|---|---|---|
| `preset` | a named palette to start from — `default`, `warm`, `raw`, `jazz` (which also sets `scale = "dorian"`, `swing = 0.33`, `bank = "r8"`); applied before every other key | | `default` |
| `kit` | which drum each layer plays: `machine` (kick on the foundation, hat on the flourish) or `ride` (ride and snare; on `tr808` and `tr707`, which have no ride, the open hat keeps time instead and the pattern's header says so) | | `machine` |
| `ride_fallback` | what keeps time under `kit = "ride"` on a bank with no ride (`tr808`, `tr707`): `open_hat` or `crash`; named on the pattern header's `plays:` line | | `open_hat` |
| `voicing` | each rhyme word as one `note`, or as the `seventh` chord on its degree in the scale (the level is shared between the tones; in a scale without seven notes it stacks every other degree all the same — fourths in `pentatonic_minor`) | | `note` |
| `chord_sound` | the chord lane's instrument (heard only with the harmony mapping on): any name `bass` and `melody` take | | `gm_pad_warm` (`warm`, `jazz`: `gm_epiano1`; `raw`: `triangle`) |
| `bass`, `melody` | the instrument for each pitched role: a synth name above, or a GM soundfont — `gm_acoustic_bass`, `gm_electric_bass_finger`, `gm_electric_bass_pick`, `gm_fretless_bass`, `gm_synth_bass_1`, `gm_synth_bass_2`, `gm_epiano1`, `gm_epiano2`, `gm_piano`, `gm_vibraphone`, `gm_marimba`, `gm_celesta`, `gm_music_box`, `gm_kalimba`, `gm_glockenspiel`, `gm_acoustic_guitar_nylon`, `gm_orchestral_harp`, `gm_pad_warm`, `gm_string_ensemble_1` | | `gm_acoustic_bass`, `gm_epiano1` |
| `reverb` | reverb send (the drums take half, plus §4.4's rise with distinctiveness; the bass three tenths) | 0–1 | 0.25 |
| `room_size` | reverb length, seconds | 0.1–10 | 2.0 |
| `delay` | echo on the melody, a dotted eighth behind | 0–1 | 0.15 |
| `high_cut` | the top of §4.4's brightness range, Hz — scales it, never clips it | 1000–20000 | 4400 |
| `drive` | soft saturation on drums and bass | 0–1 | 0 |
| `compress` | compression on every layer | 0–1 | 0.30 |
| `humanise` | how late (up to 30 ms) and how much softer (up to 30%) each syllable strays, from its line's seed | 0–1 | 0.20 |
| `drums_gain`, `bass_gain`, `melody_gain` | each role's level — never zero, so a role cannot be muted | 0.1–2 | 0.28, 0.32, 0.75 |
| `chord_gain` | the chord lane's level, shared between a chord's tones (a triad under `voicing = "note"`, a seventh chord under `"seventh"`, an octave below the melody) | 0.1–2 | 0.40 (`warm` 0.35, `raw` 1, `jazz` 0.45) |
| `bass_octave`, `melody_octave` | the octave each pitched role plays in, whole numbers (octave 4 starts at middle C, MIDI 60) | 1–3, 3–5 | 2, 4 (MIDI 36, 60) |
| `density_level` | levels follow density: every gain in a bar × *n*^−level, *n* = the lines sounding in that bar (0.5 → ÷√*n*, 1 → ÷*n*; a bar of one line never changes). Only gain values move | 0–1 | 0 (`warm`, `jazz`: 0.5) |

---

## What maps to what

| Poetic feature | Musical result | `SPEC.md` |
|---|---|---|
| Per-stanza metrical regularity | foundation layer (pulse) vs flourish (ornament) | §4.1 |
| A stanza, once stated | keeps looping until the next stanza of its layer transforms it, bar by bar | §4.6 |
| A stanza's longest line | the length of every bar the stanza plays in; shorter lines rest | §4.6 |
| The end of the poem | the loops play one more pass — an outro — and stop | §4.6 |
| A stanza made only of refrains | stops every loop and drops in alone, like a chorus | §4.6 |
| A pause inside a line (`,` `;` `—` …) | the half-line before it loops underneath until the next one | §4.6 |
| A rhyme answered later in its stanza | the line keeps sounding until its answer arrives, then resolves | §4.6 |
| A refrain line | loops in its own lane until the next refrain line takes over | §4.6 |
| A stressed vowel in a foundation line | a bass note by vowel height: open → root, mid → third, close → fifth | §4.6 |
| A foundation line's syllables or feet (candidate, `bass=syllables` / `bass=feet`) | a walking bass: passing and approach tones between the stressed notes, or a chord tone on every beat | §4.6 |
| A foundation line's rhyme (candidate, `harmony`, off by default) | its bar's chord: the class's degree, or V/I/IV by whether the rhyme is open, answered or alone in its stanza, with a turnaround in the last bar | §4.6 |
| Foot → beat, syllable → subdivision, line → bar | rhythm | §4.2 |
| Stress | accent / velocity | §4.2 |
| A line repeated verbatim | *the same instrument* played again | §4.3 |
| Line-initial repetition (anaphora) | anchored to the downbeat | §4.3 |
| Line-final repetition (epistrophe) | a cadence that rings on | §4.3 |
| Alliteration and assonance | shared timbre | §4.3 |
| "burn / burning / burnt" | one voice, modulated by edit distance | §4.3 |
| Word rarity + phonetic novelty | brightness, attack, reverb | §4.4 |
| Rhyme class | scale degree — the melody is the rhyme words | §4.5 |
| Rhyme *precision* | pitch precision: perfect rhyme is exact, slant rhyme detunes | §4.5 |

---

## Examples

Nine public-domain poems in [`examples/`](examples/) — Shakespeare, Blake, Dickinson, Whitman,
Poe, Wilde, Hopkins, Carroll and Prešeren — chosen so that between them they exercise strict
metre, free verse, a real villanelle, a refrain, invented words and a non-English text.
[`examples/README.md`](examples/README.md) says what each one demonstrates, including where the
mappings visibly fail.

```sh
./examples/run.sh              # compile them all
./examples/run.sh --explain    # annotate them all
```

---

## Layout

```
crates/
  prosody-core/       analysis, IR and lowering. No IO, no clock, no env, no fs.
  prosody-cmudict/    the CMU pronouncing dictionary, vendored
  prosody-lts/        guessing pronunciation from spelling, for words it lacks
  prosody-lexicon/    word-rarity table, built from public-domain texts
  prosody-emit/       Style, and the Strudel and OSC backends
  prosody-score/      how interesting a compiled poem is, and whether it still listens to the poem
  prosody-cli/        arguments, files, stdout, the watcher — all IO lives here
  prosody-web/        a local page for writing a poem and hearing it, also all IO
  tree-sitter-prosody/  the grammar
```

The crate boundaries are load-bearing. `prosody-core` cannot reach the filesystem or a style
header because it does not depend on anything that can — which is how two of the four invariants
are enforced rather than merely intended.

---

## State of it

Everything `SPEC.md` asks for is built: all the mappings, both backends, the live-reload watcher,
the debug view, a golden corpus, and property tests for all four invariants. 606 tests.

Things that are known to be wrong, in rough order of how much they would bother a listener:

- **A poem with no alternating layers, pauses, in-stanza rhymes or refrains plays one thing at a
  time.** Stanzas loop, and calls, open rhymes and refrains each get a lane underneath (§4.6), but
  unrhymed one-stanza verse still has nothing to stack.
- **Pitch reaches only about 8% of events.** The melody is the rhyme words and nothing else, so
  most of what you hear is percussion with sparse melodic punctuation.
- **Polysyllabic rhyme is mis-scored.** The rhyme tail starts at the last *primary-stressed* vowel,
  so a real villanelle gets four rhyme classes instead of two — `Persephone`, `Sicily` and `bee`
  come out as three different notes.
- **Metrical variance cannot see order.** It measures spread, not period, so a limerick and a
  Sapphic stanza read as ragged, and no threshold fixes it.
- **A scale degree is numbered poem-wide**, so adding a rhyme early transposes an untouched stanza
  later.

Each is written up in [`PLAN.md`](PLAN.md)'s open questions with what a fix would cost. They are
recorded rather than patched because changing what a poetic feature maps to is a change to the
design, not a bug fix.

---

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

All four must pass. Check the exit code, not the summary text —
[`CLAUDE.md`](CLAUDE.md) §3 explains why that warning is there.
