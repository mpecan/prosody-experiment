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
tempo and bank alone: **default** (electric piano over acoustic bass, a modest room, a little echo
on the melody, light compression, a small human stray), **warm** (vibraphone over fretless bass, a
long room, more echo, a lower high cut, a touch of drive, more stray) and **raw** (the palette as
it was before any of this: triangle oscillators, dry, on the grid). Move any control and it reads
*custom*. The mix controls: reverb, room size, echo on the melody, high cut, drive, compression,
humanise, and a level each for the drums, the bass and the melody. **Humanise** makes each
syllable a few milliseconds late and a little softer, drawn from that line's own seed — so the
same poem always strays the same way, and editing one stanza never changes another's feel.

The **example poems…** menu at the top loads any poem in `examples/`, together with the palette its
header asks for (the defaults when it has none).

Under the palette is **Mappings — changes what happens**: the candidate mappings still under
review (the held and refrain lanes, the bass, the stanza period, the outro), each with a checkbox
for two settings, A and B. A starts with everything on, B with everything off. The **hearing A | B**
switch at the top swaps which one plays, without stopping, so you can hear exactly what a mapping
adds. The **Score A vs B** tab measures both settings side by side (see [Measuring it](#measuring-it))
with the difference between them, under the machine or the interest profile. The page remembers
your two settings.

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
# 121 bpm, 85 messages
#     0.000  /prosody/note  0.000 "foundation" 0 0.333 -1 0.000 0.688 0.000 1.000
#     0.500  /prosody/note  0.500 "foundation" 0 1.000 -1 0.000 0.688 0.000 1.000
```

Onsets are in **beats**, counted from the start of the piece. The tempo is sent once, separately,
because a tempo is palette and the rhythm is not.

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
the number the tuning cycle in [`PLAN.md`](PLAN.md) works to raise.

`--without held,refrain,bass,period,outro` (any of them) leaves candidate mappings out, for the
pattern and for `--score` alike; `./scripts/cycle.sh machine bass` scores every example without the
bass.

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
Banks: `tr909`, `tr808`, `tr707`, `linn`, `casio`.
Synths: `karplus`, `fm`, `subtractive`, `physical`, `sine`, `sawtooth`, `square`, `triangle`.
`synth` sets both pitched instruments; in Strudel the four model names play the recorded
instrument nearest the model (Karplus–Strong: acoustic bass and nylon guitar; FM: DX-style synth
bass and electric piano; physical: acoustic bass and marimba), and the waveforms stay oscillators.

The rest of the header, every key optional:

| key | what | range | default |
|---|---|---|---|
| `preset` | a named palette to start from — `default`, `warm`, `raw`; applied before every other key | | `default` |
| `bass`, `melody` | the instrument for each pitched role: a synth name above, or a GM soundfont — `gm_acoustic_bass`, `gm_electric_bass_finger`, `gm_electric_bass_pick`, `gm_fretless_bass`, `gm_synth_bass_1`, `gm_synth_bass_2`, `gm_epiano1`, `gm_epiano2`, `gm_piano`, `gm_vibraphone`, `gm_marimba`, `gm_celesta`, `gm_music_box`, `gm_kalimba`, `gm_glockenspiel`, `gm_acoustic_guitar_nylon`, `gm_orchestral_harp`, `gm_pad_warm`, `gm_string_ensemble_1` | | `gm_acoustic_bass`, `gm_epiano1` |
| `reverb` | reverb send (the drums take half, plus §4.4's rise with distinctiveness; the bass three tenths) | 0–1 | 0.25 |
| `room_size` | reverb length, seconds | 0.1–10 | 2.0 |
| `delay` | echo on the melody, a dotted eighth behind | 0–1 | 0.15 |
| `high_cut` | the top of §4.4's brightness range, Hz — scales it, never clips it | 1000–20000 | 4400 |
| `drive` | soft saturation on drums and bass | 0–1 | 0 |
| `compress` | compression on every layer | 0–1 | 0.30 |
| `humanise` | how late (up to 30 ms) and how much softer (up to 30%) each syllable strays, from its line's seed | 0–1 | 0.20 |
| `drums_gain`, `bass_gain`, `melody_gain` | each role's level — never zero, so a role cannot be muted | 0.1–2 | 0.28, 0.32, 0.75 |

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
