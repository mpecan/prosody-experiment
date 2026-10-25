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
  s("<[[bd bd] [bd bd] [bd bd] [bd bd]]@4 ~@4 ~@4 ~@4 …>").gain("<[[0.333 1.000] …]@4 …>")…,
  note("<~@4 ~@4 [~ 36 …]@4 ~@4 …>").sound("triangle")…
)
  .cpm(121.000)
```

Each `stack` layer is one voice; each `<…>` element is one **bar of the piece** — a line, or a loop of
earlier lines under it — and its `@4` says it lasts four beats; each step inside
it is one **foot**; each bracketed group inside that is the **syllables** of that foot. A cycle is
one beat, so `.cpm` is the tempo in beats a minute and a bar of three beats is shorter than a bar
of four (see [the pulse](#the-pulse)).

---

## Hearing it

Three ways. The first is the one to start with.

### Path A — the local web page (easiest)

```sh
npm --prefix web ci && npm --prefix web run build   # the page, once (node from mise.toml)
cargo run -p prosody-web
# prosody: http://127.0.0.1:4747 — ^C to stop
```

The page is its own program in `web/` — Preact and TypeScript, built by Vite into `web/dist`, which
`prosody-web` serves beside the compiler's answers. Working on the page itself, run
`npm --prefix web run dev` with `prosody-web` still running: Vite serves the page with hot reload
and sends every question to the server. `npm --prefix web run check` typechecks it, runs its tests
(Vitest) and builds it.

Open it. You get the poem on the left with every palette control under it, the generated pattern
and the annotation on the right, and Play/Stop at the top. It recompiles as you type — about a
third of a second after you stop — and if it is already playing it switches to the new version
without stopping, so you can rewrite a line and hear the difference immediately.

The poem you write in is also the poem you watch. While it plays, it lights up syllable by syllable
as each one sounds, in its layer's colour — pulse, ornament, bass, rhyme note, chords — at full
strength where its line is heard anew and dimmer where it is only looping (held, refrain, call, a
yielding loop, the outro), underlined where the syllable also plays a rhyme or bass note. A long
poem scrolls to follow the line being heard, unless the caret is in it. The reading has the other channel:
a pale wash behind the line being read aloud and a brighter one behind the word being said (where
the browser's voice reports its words), so the voice and the music stay legible where they meet —
the music colours the letters, the voice lights what lies behind them. Keep typing while it plays:
the verse goes plain for the moment it takes to recompile, then lights again.

Everything you set lives in **folders along the bottom edge**, the same on a phone and on a desk:
the **Roll** (Strudel's own piano roll, `@strudel/draw`, showing a set number of bars — poem lines —
with faint lines where one bar hands over to the next), **Sound** (the palette), **Mappings** (A and
B), **Reading** and **Under the hood** (the pattern, what each word became, and the score). A tab
opens its folder over the lower part of the page, another tab swaps it, the open one's tab closes
it; the page remembers which was open. The page opens with a welcome — what *prosody* means and what
this experiment does — which stays closed once closed (**about** reopens it); it follows the
system's light or dark setting, or the reader's choice (the ◐ button). A neural voice is never
downloaded unasked: the first time one is chosen the page says how big it is and asks, then shows
its download with a progress bar, Hide and Cancel. Until then, and by default, the recorded
readings read.

The lighting is `SPEC.md` §8's traceability made visible: every lit syllable is the span the sound
came from. The page gets it from `/trace` (`prosody_emit::trace`), a silent pattern of syllable ids
built by the same walk of bars and beats as the music and on the same clock, so the light lands
where the sound does. Each layer in the pattern carries `.color(...)` for the roll, which Strudel's
drawing reads and its sound ignores; `pianoroll()` on strudel.cc uses it too.

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
as one note or as a seventh chord built in the current scale; neither moves an event. With a side's
harmony mapping on, that side's rhyme words play as single notes over the chord lane whatever the
voicing, and the voicing shapes only the chords. **Ride
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
reading, the pulse, the stack, the function words, the long stanzas), each a selector of named
variants — always including `off`, except the pulse, the stack, the function words and the long
stanzas, which no piece can be without
— with one menu per setting,
A and B. The held and refrain lanes, the stanza period and the outro are `off`/`on`; the bass is
`off`, `height` (its default), `syllables` or `feet` (see [the walking bass](#the-walking-bass));
the harmony is `off` (its default), `rhyme`, `cadence` or `both` (see
[the harmony layer](#the-harmony-layer)); the reading is `off`, `arrives`, `stanza` or `breath` (its default); the pulse
is `elastic` or `steady` (its default; see [the pulse](#the-pulse)); the stack is `crossing` (its
default) or `straight` (see [stacked lines](#stacked-lines-crossing-or-straight)); the function
words are `full` (their default) or `light` (see [function words](#function-words-full-or-light));
the long stanzas are `gradual` or `erode` (their default; see
[long stanzas erode](#long-stanzas-erode)).
A starts with every mapping at its default (marked "(default)" in the menu), B with every one at its
first variant — `off`, for the pulse `elastic`, for the stack `crossing` and for the function
words `full`, the way the page played before — except the long stanzas, which B starts at `erode`
as A does: `gradual` is the quadratic arrangement erosion replaced ("Patterns" under it is 1,314
bars and a 4 MB pattern, which costs every edit seconds more to compile, score and prepare), so
choose it when you want to hear it; a menu not at its default
has a bold border. The **hearing A | B**
switch at the top swaps which one plays, without stopping, so you can hear exactly what a mapping
adds. The **Score A vs B** tab measures both settings side by side (see [Measuring it](#measuring-it))
with the difference between them, under the machine, jazz or interest profile. The page remembers
your two settings.

Under the mappings is **Reading — the poem spoken over the music**: who reads, in what language and
voice, at what pace and level, and the duck. Whether and when lines are read is the `reading`
mapping above. Under `breath`, the default, each stanza is read through, in order; at the fitted
pace one line a bar (at the natural pace, below, a line after the last); it
starts in the bar where the music first plays its first line — or one silent bar after the previous
stanza's last line, if that is later — so lines within a stanza are always a bar apart, two stanzas
always have at least one silent bar between them, and a stanza is never read before the music has
begun it. In The Tyger: stanza 1 in bars 1–4, stanza 2 in bars 6–9 (it arrives in bar 5, the
breath), stanza 3 in 21–24, and so on, each where it arrives. A stanza that would run past the
piece's last bar reads the lines that fit and loses the rest — no example does with the outro on;
Pied Beauty with `outro` off loses its last line, "Praise him." — and says so: the reading's
pattern lists every line it does not read and why, and the status bar says `not read: line 11, …`
while it plays. Under `arrives`,
each line is spoken once, in the bar where it is first heard — its stanza stated, dropping in as a
refrain stanza, or replacing a bar of the old loop — and never where it only loops, is held,
refrains in its lane or repeats in the outro. Under `stanza`, a stanza's lines are read one a bar
from the first bar any of them is heard in, so a stanza that transforms a loop (one new line a
pass, five bars apart in The Tyger under `arrives`) is read straight through while the music is
still turning into it, with no pause after the stanza before it. Under `off`, nothing is read.
Strudel's own clock fires each line, so it stays on the bar; a line still being spoken when the
next is due is cut off. The speaking rate is fitted so the line fills most of its bar at the piece's tempo, within
0.6–1.6 — at the **fitted** pace. The default pace is **natural**: each line at the voice's own
speed, a little faster or slower line by line (0.92–1.08, from the line's own seed), the next after
a pause from how the line ends (0.25 s running on, 0.45 s at a comma, 0.8 s at a full stop, 1.5 s
at a stanza's end), and a stanza starting where the music begins it — or, if the last stanza's
voice ran on, on the first bar after one whole silent bar. Switch **Pace** to compare by ear. Pick
an **engine**: Kokoro-82M (the default — 92 MB, downloaded on your first play, on WebGPU where your
browser has it, WebAssembly otherwise), Piper (many languages, Slovenian among them; about 60 MB a
voice), Kitten TTS nano (24 MB, WebGPU only), **recorded readings** (the example poems spoken
ahead of time, a few kB a line, in four voices — see [Recorded readings](#recorded-readings); a line
you edit has no recording and is read live, which the status bar says) or your browser's own voice (instant, and what reads
while a model downloads, or if one fails to load or stops working — the status bar says so in red;
choose the engine again to retry). Each download shows its progress in the status bar
and is kept by the browser; every line is rendered ahead after each compile, the line due next
first, and started exactly on the music's clock. A line the reading leaves out — the piece came
round, or ended, before its turn — is said in the status bar. At the fitted pace a rendered line too long for its bar is rendered again,
faster, to fit. An example poem sets its own **language** — Prešeren's Zdravljica is read in
Slovenian, which only Piper (`sl_SI-artur-medium`) and perhaps your browser can speak. The page
remembers the engine, pace, level and a voice per engine and language; untick **duck** to stop the
music dipping while a stanza is read. Add `?device=wasm` to the page's address to keep every
engine off WebGPU. The Pattern tab shows the cue pattern under the music's, and the
browser console logs every line as it is cued and started: which bar, which bytes, which rate or
pace, which engine, and how late if a render was. Picking a
preset or switching A/B while playing starts the piece again from bar 1, so the reading begins at
the first line. A live edit keeps playing from where it is, and the reading carries on from there.
If Strudel's scheduler ever falls behind — it skips a tick it reaches too late, and every note and
cue in that tick is lost, saying so only on the console — the status bar says so in red:
`Strudel fell behind: 58 tick(s) not played, their notes unheard; not read: line 10`.
Nothing the page does should cause that: it parses a new pattern's mini-notation ahead, a few
milliseconds at a time between the scheduler's ticks, and remembers what it has parsed, so a live
edit to "Patterns" while it plays — about 1,200 strings, a second to parse — swaps in the new
version without a gap (an edit changes only the strings of the voices it touches).
It is a
candidate mapping and a page-only one — the CLI's pattern and strudel.cc know nothing of it
(`SPEC.md` §4.7).

It binds to `127.0.0.1` only, and deliberately: it compiles arbitrary text from a request body,
which is exactly right on your own machine and exactly wrong on a network. Playback loads Strudel,
the drum-machine samples (the map strudel.cc uses) and the General MIDI soundfonts from CDNs, so
the first Play needs an internet connection; each instrument a pattern uses is loaded before it
starts. The page's output goes through a gentle compressor and then a soft clipper that cannot
exceed 0.95, so a dense bar bends rather than clips.

### Path A′ — the same page, without a server

The page also builds as static files, with the whole compiler inside it as WebAssembly:

```sh
./scripts/build-site.sh                  # into target/site: the page, the compiler as WebAssembly
python3 -m http.server -d target/site    # or any static host; then open http://localhost:8000
npm --prefix web run e2e                 # the built site in Chromium, end to end
```

It is the same page with the same controls, and it answers itself: every request the served page
sends to `prosody-web` goes to `prosody_bg.wasm` instead (`crates/prosody-wasm`, bound to the
page by `wasm-bindgen`), running in two Web Workers — one for what plays, one for the scores — so
compiling and scoring never hold up the music. It can be put on any
static host (GitHub Pages, a bucket), and since it compiles in the visitor's browser, nothing on
the host runs arbitrary input. The compiler is about 5 MB, about 1 MB compressed — nearly all of
it the pronouncing dictionary — and the first compile takes a moment while the dictionary loads.
Opening `index.html` straight from disk does not work: browsers refuse to fetch a file beside a
`file://` page. The page's data — option lists, presets, mappings — is `site.json` beside it, written
by `prosody-web --site`. The reading's neural voices run from `speech-worker.mjs` beside the page, a
module worker that loads each engine from a CDN, so they work the same on a static host
(Cloudflare Pages included); the host needs no special headers. The recorded readings are copied in too, as
`readings/` beside the page.

`./scripts/check-wasm.sh` builds both and checks that the browser build answers every view of every
example, under several palettes and mapping choices, byte for byte as the native server does —
invariant I1 across targets. It needs node. Building needs the `wasm32-unknown-unknown` target and
the `llvm-tools` component, which `rust-toolchain.toml` asks rustup for, and the `wasm-bindgen`
CLI at exactly the crate's version, which the script installs into `target/tools` the first time
(leaving any other `wasm-bindgen` on your PATH alone).

**Deploying it.** The static page is published at [prosody.rocks](https://prosody.rocks) as an
assets-only Cloudflare Worker (`wrangler.jsonc`): no server code, since every request is answered in
the visitor's browser. `./scripts/deploy.sh` checks that the browser build answers as the native one
(`check-wasm.sh`), rebuilds `target/site` from empty and runs `wrangler deploy`; `--dry-run` does
everything but the upload. Wrangler comes from mise (`mise.toml`) and must be logged in to the
account that holds the `prosody.rocks` zone (`mise exec -- wrangler login`). The custom domain makes
its own DNS record and certificate on the first deploy. `www.prosody.rocks` is a second custom domain that
only ever redirects: a tiny worker in front of the files (`deploy/worker.js`) sends `http://` and
`www` to `https://prosody.rocks` for good (301), path kept, and every https answer carries a
one-year HSTS header; `deploy/check-worker.mjs` checks this before each deploy.
`mise exec -- wrangler dev` serves the built site locally through the same runtime.

### Recorded readings

`examples/readings/` holds every line of every example poem spoken ahead of time — each line any
variant of the reading can speak, at its own natural pace — in four voices: Kokoro `af_heart`
(American, female), Kokoro `bm_george` (British, male) and Piper `en_GB-cori-medium` (British,
female) for the English poems, Piper `sl_SI-artur-medium` for Prešeren. One MP3 a line and voice
(mono, 32 kbit/s), named by the voice, a hash of the line's exact text and the pace it was
spoken at (`kokoro-af_heart/<key>-0.950.mp3`), and `manifest.json`
listing them; choose **recorded readings** as the engine, and the voice menu lists the voices that
have recordings of the poem in front of you. `prosody-web` serves them from the checkout
(`--readings DIR` for another place), and `./scripts/build-site.sh` copies them into the site.

They are made on your own machine by `scripts/readings/`, a small Node package kept out of the
Rust workspace, which runs the page's own `speech-worker.mjs` in headless Chromium — so a
recording is the same voice, model and phonemiser as the live engine — and encodes with ffmpeg:

```sh
./scripts/build-site.sh                       # the compiler as WebAssembly, which the tool asks
cd scripts/readings && npm ci                 # playwright-core 1.64.0, the only dependency
npx playwright-core install chromium          # once, if Chromium 1248 is not already there
node render.mjs                               # renders what is missing, removes what nothing wants
node render.mjs --poem blake-tyger --voice kokoro:bm_george --force   # just those, again
node render.mjs --check                       # renders nothing; fails if anything is missing or stale
npm test                                      # the tool's own tests
```

It needs node 20 or later, ffmpeg built with libmp3lame (`brew install ffmpeg`), and the network
(the engines' code from jsDelivr, the models from Hugging Face; Kokoro's full-precision model is
326 MB, kept in `scripts/readings/.profile/`). Re-render after changing an example poem, or anything
that changes which lines the reading speaks or their pace: `cargo test` checks the committed set
against the compiler and fails until you do. A pace changed with no text changed is a new file
name, so `node render.mjs` renders the lines again at their new pace and removes the old audio,
rather than keeping it under the new pace. Adding a voice is a line in `scripts/readings/lib.mjs`
`VOICES` — check its licence first (below).

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
takes `off`, `arrives`, `stanza` or `breath` (default `breath`), `pulse` takes `elastic` or `steady`
(default `steady`), `stack` takes `crossing` or `straight` (default `crossing`), `function-words`
takes `full` or `light` (default `full`), `long-stanzas` takes `gradual` or `erode` (default
`erode`) — for the pattern
and for `--score` alike;
an unknown name or variant
is an error that names it. `--without held,refrain,…` is shorthand for setting those to `off`
(`--without pulse` is `pulse=elastic`, `--without stack` is `stack=crossing`,
`--without function-words` is `function-words=full` and `--without long-stanzas` is
`long-stanzas=gradual`: none of them has an `off`).
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
  In a stanza's split last bar under `harmony = cadence` (or `both`) a foot from the bar's half
  walks on the turnaround — the bar's half, as the chord lane changes there, even when the line
  ends early in a bar its stanza's longest line sets.

Under a walk a rhyme word plays its melody note *and* its bass note, and only one walk sounds a
bar: a line held, refrained or called under another keeps its drums and melody but not its walk,
so two walks never clash a semitone apart. The pattern's header names the
walk (`walking bass (mapping bass = feet): …`), and `--explain` adds a `bass` block under each
foundation line saying why each note is what it is (`step down: AH is mid, so the third of IV deg
3 would repeat the last note; the next foot's ER is not higher`), lists the bars it walks in and
the ones it is silent in, and shows each syllable's own bass note in the pitch column. The OSC
schedule sends a bass note where there is no melody note, at the bass note's accent, and counts
the ones it cannot send.

### The pulse

*Decided: `steady` is the default* (`SPEC.md` §4.2, `--set pulse=…`, or the `pulse` menu on the
page). How long a bar of the Strudel pattern lasts.

- **`steady`** — a bar lasts as many beats as it holds, so the beat is the same speed all the way
  through. A cycle is a beat: the bars are weighted with mini-notation's `@` (`<[…]@3 […]@4>`, a
  bar of weight *w* lasting *w* cycles), and `.cpm` is the tempo itself. Swing is
  `.swingBy(x, 1)`, so it swings every beat whatever the length of its bar.
- **`elastic`** — every bar lasts the same time, one cycle, at the tempo over the piece's integer
  mean bar length: the pattern as it was until now. A bar of four beats among bars of three plays
  its beats a third faster — Wilde's Theocritus at 161 bpm against its 121.

The pattern's header says which, and what tempo each bar length is heard at (`pulse = elastic: …
3 beats at 121.000 bpm, 4 beats at 161.333 bpm`); `--explain` names it at the top. The page's roll
draws its bar lines where the trace says the bars begin, so under `steady` they are unevenly
spaced where bar lengths change; the lit poem and the reading's cues are on the same weighted bars,
and each spoken line's rate is fitted to the bar it is spoken in. The OSC schedule is in beats and
was always steady; asked for `elastic` it says it does not apply it. Only Hopkins, Poe and Wilde
mix bar lengths, so only they sound different; every other example plays exactly as before.

### Stacked lines: crossing or straight

*Decided: `crossing` stays the default; `straight` is a variant to hear against it* (`SPEC.md` §4.2,
`--set stack=…`, or the `stack` menu on the page). A line can sound *under* the bar's own line — held
until its rhyme is answered, looping in the refrain or call lane, a loop yielding or carrying on —
and its feet need not be the host's: a dactyl's three syllables in a beat against an iamb's two.

- **`crossing`** — every line divides each beat by its own foot, wherever it sounds: three against
  two where the feet differ. Wilde's Theocritus crosses in 38 of its 126 beats.
- **`straight`** — where a stacked line's beat would cross the host's, its syllables are fitted to
  the host's subdivision of that beat, each still its own event. Three over a pair become a
  long-short-short, `[a@2 b c]` (onsets 0, ½, ¾): the first takes the host's first half and the
  other two share the second. Two over a triplet become a long-short, `[a@2 b]` (0 and ⅔). The host
  never moves, nor a line in a beat where it already agrees with the host, nor a loop squeezed into
  a shorter bar (its beats fall between the host's). Theocritus then crosses in none.

The pattern's header names `straight` and counts the beats it fitted (`stack = straight: … in 38
beat(s)`), and `--explain` lists under each line where it was fitted (Theocritus's "Dost thou
remember Sicily": `straight 2 into 3 at 4.0 · 3 into 2 at 3.1 3.2 4.1 … — fitted to the bar's line
(slot.beat)`). The page's lit poem, the OSC schedule and the scorer
all read the same fitted onsets — the one-word edit test included, so where a line is fitted to a
host in another stanza, an edit to the host counts as reaching it: Theocritus's *local* share falls
from 29 to 18 of 61 edits under `straight`, its *explained* share (and legibility) stays 50 of 61.

### Function words: full or light

*Decided: `full` stays the default; `light` is a variant to hear against it* (`SPEC.md` §4.2,
`--set function-words=…`, or the `function-words` menu on the page). The dictionary stresses most
one-syllable function words — "or", "thy", "what", "and", "of" — as hard as "hand", so The Tyger's
"What immortal hand or eye" strikes "or" at full force, off the beat.

- **`full`** — every syllable at its dictionary stress, as before.
- **`light`** — a one-syllable function word is struck at 2/3 unless the meter's scansion puts it
  on a strong position: "of" in "In the forests of the night" is the strong half of an iamb and
  keeps its accent; "or", in the spondee "hand or" of a falling line, is light. In a rising line a
  spondee's second syllable is the strong one, so "Did he who made the Lamb make thee?" keeps "thee".

The word list is English, fixed and written out, and lives with the English lexicon
(`crates/prosody-lexicon/src/function.rs`, which also says what was left out and why). A light
function word is no longer an accent anywhere: the drum gain, the OSC velocity, the scorer — and the
height bass, which plays accents, so it loses its bass note. The pattern's header and the OSC
schedule name `light` and count the syllables (The Tyger 24, the Sonnet 24, Whitman 18, Dickinson
15, Poe 13, Hopkins 9, Jabberwocky 7, Wilde 4, Prešeren 0 — Slovenian matches no English function
word); `--explain` marks each light row `function-words`.

### Long stanzas erode

*Decided by the user: `erode` is the default; `gradual` is what preceded it* (`SPEC.md` §4.6,
`--set long-stanzas=…`, or the `long-stanzas` menu on the page). A stanza arriving into a running
loop replaces it one line a pass, so a stanza of *n* lines takes about *n* passes of *n* bars —
Lowell's "Patterns" (stanzas of 9 to 22 lines) arranged into 1,331 bars and an 8.6 MB pattern.

- **`erode`** — where either stanza has **more than eight lines**, the old loop is not replaced: it
  keeps looping under the arriving stanza, from its first line, and thins out over **three passes**
  (each pass one statement of the arriving stanza) until it is silent. On pass *p* an eroding line
  of *k* syllables keeps ⌈k·(4−p)/4⌉ of them — three quarters, a half, a quarter. Unstressed
  syllables go first, then secondary and then primary stresses, and the rhyme word — the line's
  last word — last; within each, the earlier syllable first, so what is left of a line is its end.
  The rule reads only the line's own syllables. Stanzas of eight lines or fewer are transformed a
  line a pass, as before, so every other example is the piece it was. "Patterns" is now 284 bars
  and a 0.9 MB pattern.
- **`gradual`** — every arrival is a line-by-line transformation, however long.

A lost syllable is simply absent — from the drums, the melody, the bass, the trace's lit poem, the
OSC schedule and the scorer alike — and every syllable that still sounds keeps its span. The
pattern's header, the OSC schedule and `--explain` count the bars over an eroding loop
(`long-stanzas = erode: 240 bar(s) over an eroding loop` in "Patterns"), and `--explain` shows each
eroding line's thinning (`erodes pass 1 keeps 6 of 7 · pass 2 keeps 4 · pass 3 keeps 2: figured`).
The reading reads the arriving stanza where it is stated, and never reads an eroding line again.

A piece of more than 16 bars is written in groups (`strudel::nested`): runs of rests joined into
one rest as long, then bars bracketed sixteen at a time (`<[a@3 b@4 …]@58 …>`). The same notes at
the same times — every hap of every example's whole loop compared under the page's Strudel — but
Strudel visits every element of a sequence on every query, and the flat 284-bar sequence cost more
per scheduler tick than a tick lasts: the page played about a sixth of "Patterns". Grouped, it
plays all of it (a query costs 3.9 ms instead of 49 ms). The text of the 13 examples over 16 bars
changed with it; nothing they play did.

### The harmony layer

*A candidate, off by default* (`SPEC.md` §4.6, `--set harmony=…`, or the `harmony` menu on the
page). Every bar of a foundation stanza gets one chord, held for the bar, in a lane of its own —
except a stanza's last bar under `cadence` and `both`, which is split in two (below); its span is
the rhyme word of the line the bar hosts, and it plays wherever that line sounds in the foundation
layer — stated, looping under a flourish, in the outro — never in the call, held or refrain
lanes. Each chord lasts its bar's beats, like every other lane's bar.

| variant | the chord of a line's bar |
|---|---|
| `rhyme` (H1) | I on its rhyme class's degree: ABAB → i–ii–i–ii, AABB → i–i–ii–ii; a line that rhymes with nothing has no chord |
| `cadence` (H2) | a rhyme answered later in its stanza → V of its degree; its answer → I on the degree; a line with no rhyme in its stanza → IV of the stanza's key (its first in-stanza rhyme's degree, or 0); the stanza's last bar (of two or more lines) is **split**: its own chord by those rules for the first half — so ABAB, AABB and ABBA land on I — then, from the bar's half, V of the stanza's *own* first chord, a turnaround the loop resolves |
| `both` | H2 for open and closing lines and the turnaround, H1 for every other bar — a last line that rhymes with nothing rests for the first half, then turns round |

The pattern's header lists `chord_sound` and `chord_gain` and every line's chord (`chords by line:
1:V/4 2:V/5 3:I/0 4:I/1+V/8`, `-` a rest, `a+b` a split bar's own chord and its turnaround) when
there is one; `--explain` shows each line's chord, its degree and the rule that chose it, or why
the bar has none, and a split bar's turnaround on a second row (`then, from the bar's half, V deg 8
(V of deg 4) · …`). The OSC schedule sends no chord and says how many it leaves out, and how many
of those are turnarounds.

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
| `voicing` | each rhyme word as one `note`, or as the `seventh` chord on its degree in the scale (the level is shared between the tones; in a scale without seven notes it stacks every other degree all the same — fourths in `pentatonic_minor`). With the harmony mapping on, rhyme words are single notes over the chords whatever this says, and it shapes only the chord lane | | `note` |
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
| A stanza of more than eight lines arriving, or being replaced | the old loop erodes under the new stanza: fewer of its syllables each of three passes, unstressed first, rhyme words last, then silence | §4.6 |
| A stanza's longest line | the length of every bar the stanza plays in; shorter lines rest | §4.6 |
| The end of the poem | the loops play one more pass — an outro — and stop | §4.6 |
| A stanza made only of refrains | stops every loop and drops in alone, like a chorus | §4.6 |
| A pause inside a line (`,` `;` `—` …) | the half-line before it loops underneath until the next one | §4.6 |
| A rhyme answered later in its stanza | the line keeps sounding until its answer arrives, then resolves | §4.6 |
| A refrain line | loops in its own lane until the next refrain line takes over | §4.6 |
| A stressed vowel in a foundation line | a bass note by vowel height: open → root, mid → third, close → fifth | §4.6 |
| A foundation line's syllables or feet (candidate, `bass=syllables` / `bass=feet`) | a walking bass: passing and approach tones between the stressed notes, or a chord tone on every beat | §4.6 |
| A foundation line's rhyme (candidate, `harmony`, off by default) | its bar's chord: the class's degree, or V/I/IV by whether the rhyme is open, answered or alone in its stanza, with a turnaround in the second half of the stanza's last bar | §4.6 |
| Foot → beat, syllable → subdivision, line → bar | rhythm | §4.2 |
| A line stacked under another (candidate, `stack=straight`) | fitted to the host's subdivision where their feet would cross (`[a@2 b c]`) | §4.2, §4.6 |
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
  prosody-web/        what the page asks, answered by a library, plus a local server
  prosody-wasm/       that library built for the browser, for the static page
  tree-sitter-prosody/  the grammar
web/                  the page itself: Preact and TypeScript, built by Vite
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

## Voices and their licences

The reading's neural voices are not part of this repository: the page loads each from a CDN at an
exact version, and its model from Hugging Face, only when you choose it.

| Engine | Code (version, licence) | Model and voices (licence) |
|---|---|---|
| Kokoro | kokoro-js 1.2.1 (Apache-2.0), bundling Transformers.js 3.5.1 (Apache-2.0) and phonemizer.js (Apache-2.0, around espeak-ng, GPL-3.0) | Kokoro-82M v1.0, `onnx-community/Kokoro-82M-v1.0-ONNX` (Apache-2.0), voices included |
| Piper | onnxruntime-web 1.18.0 (MIT); the espeak-ng phonemiser module of @mintplex-labs/piper-tts-web 1.0.5 (MIT) with @diffusionstudio/piper-wasm 1.0.0 (MIT; espeak-ng data, GPL-3.0) | `rhasspy/piper-voices` at revision `5041a7a`; each voice's licence is its `MODEL_CARD`: `en_GB-cori-medium` (the English default) is trained on public-domain LibriVox readings, `sl_SI-artur-medium` (Slovenian) on the ARTUR corpus under CC BY 4.0; several others are non-commercial (`en_US-lessac-*`, Blizzard 2013 licence; `en_US-ryan-*` and `en_US-hfc_*`, CC BY-NC-SA 4.0) — check a voice's card before using what it reads |
| Kitten | kitten-tts-webgpu 0.1.1 (MIT; its phonemiser carries espeak-ng data, GPL-3.0) | Kitten TTS nano 0.8 int8, `KittenML/kitten-tts-nano-0.8-int8` (Apache-2.0) |
| browser | your browser's `speechSynthesis` | your operating system's voices, under its terms |

The **recorded readings** in `examples/readings/` *are* in this repository: speech made with the
voices above from the public-domain example poems, and shipped with the site. Each was chosen for a
licence that allows it:

| Recorded voice | Made with | Licence of the voice |
|---|---|---|
| `kokoro:af_heart`, `kokoro:bm_george` | kokoro-js 1.2.1, Kokoro-82M v1.0 full precision (fp32) on WebGPU | Apache-2.0 (the model and its voices) |
| `piper:en_GB-cori-medium` | onnxruntime-web 1.18.0, `rhasspy/piper-voices` at `5041a7a` | the voices repository is MIT; the voice's `MODEL_CARD` gives its dataset as LibriVox, public domain |
| `piper:sl_SI-artur-medium` | the same | the voices repository is MIT; trained on the ARTUR corpus (`ppisljar/artur_studio_tts`), CC BY 4.0 — attribution: the ARTUR speech corpus, voice trained by ppisljar |

`manifest.json` repeats each voice's licence. The tool's one dependency, playwright-core 1.64.0,
is Apache-2.0, and is used only on the machine that renders.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --no-fail-fast
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

All four must pass. Check the exit code, not the summary text —
[`CLAUDE.md`](CLAUDE.md) §3 explains why that warning is there.

## Contact

Questions, poems that should be in it, ideas: [me@prosody.rocks](mailto:me@prosody.rocks).

## Licence

The code is under the [MIT licence](LICENSE). What it bundles keeps its own terms: the CMU
Pronouncing Dictionary (`crates/prosody-cmudict/data/LICENSE`), the example poems (public domain;
each file names its source), and the voices and models listed under
[Voices and their licences](#voices-and-their-licences).
