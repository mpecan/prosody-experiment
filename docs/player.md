# The player — design

*Draft, 2026-10-31. Status: steps 1 (the Performance), 2 (first sound), 3 (everything the text
asks for), 4 (the reading, and the Prosody player the only one) and 5 for Android (the native
player, with the screen off and lock-screen controls; iOS later) built. The open questions of §8
are decided (§8). What building step 1 changed in this design is in §9, step 2 in §10, step 3 in
§11, with the measurements against Strudel, step 4 in §12 and step 5 in §13.*

Prosody plays a poem today by writing Strudel text and handing it to Strudel's runtime
(`@strudel/web`, its scheduler and the `superdough` synth). This document designs a player of
our own: the third backend `SPEC.md` §3 already names ("direct DSP"), arriving where §7 puts it,
after the mappings hold up.

**Strudel stays the language Prosody writes.** The pattern on the page, the strudel.cc workflow
and share links are unchanged. What changes is what makes the sound in the app and on the page.

## 1. Why

Two measured problems and one licence problem.

1. **Strudel drops notes.** Profiled on a Pixel 10 Pro XL (debug build, screen on, 2026-10-31):
   Strudel's scheduler skipped 2 ticks in 57 s at the default audio buffer and 6 in 50 s at the
   `playback` buffer; Chromium reported no audio glitches in either. The scheduler queries the
   pattern from a 100 ms page timer with about 100 ms of slack, measured against the audio clock;
   a tick that arrives late is discarded whole (`skip query: too late`), and with it 50 ms of
   notes. Page frames take 55–70 ms while playing (mostly style recalculation for the word
   highlighting, not script), and starting playback (speech, samples) adds more. A larger audio
   buffer makes the audio clock advance in bigger, irregular steps and the skips get *worse*,
   so there is no cheap fix inside Strudel.
2. **Every note is fetched just in time from the busiest thread on the page.** Prosody does not
   need that: the compiler knows every note of the piece, with its time, before playback starts.
3. **Strudel's runtime is AGPL-3.0.** Fine on the web and on Google Play with source and
   notices; a risk on Apple's App Store. A player of our own is MIT, like the rest of Prosody.

## 2. Shape

```
                    compile worker (wasm)                      render worker (wasm)              audio thread
poem ─► Score ─┬─► Strudel text ─────────► page, strudel.cc
               └─► Performance ─────────────────────────────► prosody-synth renders stems ──► AudioWorklet:
                   (every note, timed,                         ~0.5 s ahead, in blocks         queue → stem mix (mute,
                    resolved, with span)                       ▲                               solo, faders) → master
                                                               │ samples (drums, soundfonts,   → output
                                                               │ speech PCM)                   │
                                                main thread ───┘                               │ playhead (frames)
                                                (UI, highlighting, roll) ◄─────────────────────┘
```

Three decisions carry the design:

- **A resolved note list, not a pattern.** A new backend, `Performance`, turns the `Score` into
  every note of the piece: onset and duration in seconds, sound, pitch, every parameter already
  computed, and the token `Span` it came from (I4). Nothing is decided at play time.
- **Render ahead in a worker, play from a queue.** The synth runs in a dedicated worker, not on
  the page's thread and not on the audio thread. It keeps about half a second of audio ahead.
  The AudioWorklet only mixes finished blocks, so a busy page cannot make a note late, and the
  audio thread's own work is a few multiplies per sample.
- **One synth crate, pure Rust.** `prosody-synth` turns a `Performance` and decoded samples into
  PCM. No IO, no clock, no randomness except seeded ChaCha8, so I1 extends from text to audio:
  the same poem, style and samples give the same samples out. The same crate runs in the
  browser (wasm), in the CLI (`prosody render`, to WAV) and, later, natively in the app.

## 3. The Performance backend

Lives in `prosody-emit` (`perform.rs`), implements `Backend` with `Output = Performance`.

```rust
pub struct Performance {                      // times are seconds, never frames
    pub seconds: f64,                         // length of the piece, tails excluded
    pub bars: Vec<BarMark>,                   // start time, slot, for the roll and for swaps
    pub notes: Vec<Note>,                     // sorted by (onset, span.start, lane)
    pub cues: Vec<Cue>,                       // reading: line, bar, both paces
    pub marks: Vec<Mark>,                     // what lights up, and when (today's trace)
}
pub struct Note {
    pub span: Span,
    pub channel: Channel,                     // Drums | Bass | Melody | Chords (mute/solo stems)
    pub lane: Lane,                           // the colour/role the roll draws
    pub onset: Seconds, pub length: Seconds,  // after swing and nudge
    pub sound: Sound,                         // Drum{bank, piece} | Font{name, variant} | Wave(Sine|Saw|Square|Triangle)
    pub pitch: Option<Midi>,                  // fractional for slant rhymes
    pub gain: f32,                            // gain × velocity
    pub voice: VoiceParams,                   // attack, release, lpf, crush, vowel, shape
    pub sends: Sends,                         // room, delay
}
```

What the Strudel emitter writes, the Performance must mean, note for note. The inventory of what
the text asks for is small (checked 2026-10-31): drum samples from five banks, 28 soundfont
instruments, four oscillator waves, MIDI pitches, and `gain velocity nudge lpf attack release
room roomsize delay delaytime delayfeedback crush vowel shape compressor postgain`, with
`swingBy` and `cpm`. No randomness and no continuous signals: every value is already a literal
per event.

**One source of the numbers.** Today `strudel.rs` computes each value as it prints it (`gain`
from stress velocity and density, `lpf` from sharpness, and so on). Those computations move into
`perform.rs` as functions of an event and the style, and the Strudel emitter calls them and
prints. Two backends that each work out "how loud is this note" would drift; one function that
both use cannot. The Strudel text's bytes must not change in this move — the corpus snapshots
are the check.

**Time.** The arrangement's slots, weights and grouping become seconds through the same clock
helpers the emitter uses (`weights`, `starts`, `cycles_per_minute`), so steady and elastic pulse
fall out of one code path. Swing follows Strudel's documented `swingBy(x, n)`: each cycle is cut
into *n* slices and events in the second half of a slice are delayed by *x* of a half-slice.
Nudge delays a note by its seconds. Times are computed as exact rationals and converted to
seconds once, at the end.

**The parity test** is what keeps the two backends honest. A Vitest suite evaluates each corpus
pattern with `@strudel/core` (already a dev dependency; tests are not distributed), queries the
whole piece, and compares the haps with the Performance for the same poem: same onsets, sounds,
pitches and parameters, to 1e-6. Any change to either side that the other does not follow fails.

## 4. The synth (`prosody-synth`)

A new crate, pure, depends only on `prosody-core` types if any. Hand-rolled DSP, no new
dependency (CLAUDE.md §10): every piece below is a page of standard, published technique.

**Clean room.** `superdough` is AGPL; translating its code into this MIT crate would make the
crate a derivative of it. We do not read its source to write the synth. We implement from
published DSP (RBJ biquad cookbook, PolyBLEP, Jot/Schroeder reverbs, the SoundFont 2 spec) and
match Strudel's *sound* by measuring its output — rendering the same notes in both and comparing
by ear and by spectrum — never its code.

Per note:

| Stage | What | Notes |
|---|---|---|
| Source | Sampler or oscillator | Sampler: drums play the file once; soundfonts pick the zone for the key, pitch by `2^((note − root)/12)` with cubic interpolation, honour the zone's loop. Oscillators: sine, and PolyBLEP saw, square, triangle (no aliasing) |
| Envelope | attack, hold for the note's length, release | linear attack, exponential release; drums ignore the hold |
| Filter | `lpf` | 2-pole low-pass, RBJ biquad, Q fixed to whatever matches Strudel by measurement |
| Colour | `vowel`, `crush`, `shape` | five-formant band-pass bank per vowel (published formant tables); bit-depth quantiser; soft-clip waveshaper with makeup gain |
| Level | `gain`, `postgain` | |
| Sends | `room`, `delay` | per note, into its channel's buses |

Per channel (drums, bass, melody, chords, speech): a bus with the channel's compressor (when the
style asks for one), its own delay (`delaytime`, feedback 0.35) and its own reverb. Reverb is a
feedback delay network with RT60 from `roomsize`; if that does not get close enough by ear to
Strudel's room, a partitioned convolution with a seeded, exponentially decaying noise response is
the fallback (costlier, same sound family). Reverb and delay are linear, so running one per
channel sounds the same as one shared bus, and it is what lets mute and solo silence a channel's
tail too.

The output of the render is **five stereo stems**. The master stage moves into the worklet:
stem gains (mute, solo) → speech duck → compressor → ¼ gain → soft clip, the chain the page
builds today in `strudel/index.ts`, which is our own code and ports directly.

Voices are allocated from a fixed pool (no allocation on the render path); a note that would
exceed the pool steals the quietest releasing voice, and the count of stolen voices is reported
(CLAUDE.md §6: a fallback is visible).

## 5. Running it in the browser and the app

**Render worker.** Loads `prosody-synth` (wasm), receives the Performance and the decoded
samples, and renders 2,048-frame blocks while it is less than ~0.5 s ahead of the playhead. Blocks
go straight to the worklet over a `MessageChannel` as transferable buffers; the main thread is
not on that path.

**AudioWorklet.** Holds a queue of stem blocks, mixes them with the live stem gains, runs the
master stage, outputs, and posts the frame count it has played every ~50 ms. If the queue runs
dry it plays silence and counts an underrun, which the status line reports the way it reports
dropped ticks today.

**Samples.** Decoded on the main thread with `decodeAudioData` (FLAC drums, the soundfont zones,
speech) and transferred to the worker once. Only the sounds a Performance uses are loaded.

**Live edits.** A new Performance is handed to the worker with the bar it takes over from: the
first bar boundary after what the worker has already rendered. Notes already sounding keep
ringing (releases and reverb tails are the synth's state, not the Performance's); notes from that
bar on come from the new Performance. Position is kept by **bar, not seconds**, so an edit that
changes the tempo continues from the same bar. Latency of an edit: the render lead plus at most
one bar — no worse than now, where the page waits for a cycle anyway. Presets and A/B switches
still start from the top.

**Mute and solo become instant** (a stem gain in the worklet), instead of today's query-time
rewrite. The mixer's faders remain style controls that recompile (they are part of the style,
I3).

**The page's clock.** Everything the page draws comes from the Performance and the playhead:

| Today (Strudel) | With the player |
|---|---|
| `Drawer` over the scheduler's haps | `requestAnimationFrame` loop; playhead = frames played ÷ rate − output latency |
| `roll.ts` reads `hap.whole`, `hap.value.color/note/muted` | the Performance's notes, by binary search on onset |
| word lighting queries the trace pattern | `Performance.marks` by time; the trace pattern is no longer evaluated |
| meters from active haps | notes active at the playhead |
| dropped-tick report via `console.log` interception | underrun count from the worklet |

**The reading.** Cues become part of the Performance (line, bar, both paces). The TTS engines are
unchanged; their PCM goes to the render worker, which places each line on the speech stem and
writes the duck as gain automation, both from the same timeline as the music. The natural-pace
chaining (`reading/index.ts`, `naturalCue` and friends) moves into the worker unchanged at first,
as it needs each line's rendered length. The browser's built-in voice cannot be placed in a
stream; it stays timed off the playhead, as unsynchronised as it is today, and says so.

**In the app.** The page and the player are the same as on the web; the worklet is inside the
WebView. Later (phase 5), the same crate can run natively in the Tauri shell — `oboe`/AAudio on
Android, Core Audio on iOS — which is also how playback survives the screen turning off and gets
lock-screen controls. The crate boundary is drawn so that move needs no change to it. *(Step 5
built it for Android, through `cpal` rather than `oboe`, with the render worker's logic moved into a
crate both run: §13.)*

## 6. Tests

| Kind | What it pins |
|---|---|
| Property (proptest) on the Performance | I1 bytes; I2 an edited stanza leaves other stanzas' notes identical; I3 a style change keeps the note structure; I4 every note's span resolves in the source |
| Parity (Vitest, `@strudel/core`) | the Performance plays what the Strudel text says, over the corpus |
| Unit, hand-worked (`prosody-synth`) | filter −3 dB at cutoff; envelope values at known times; oscillator period; sampler pitch ratio; reverb RT60 within tolerance; crush levels |
| Golden audio (insta) | a hash of the rendered PCM of each corpus poem, drums and oscillators only (deterministic everywhere); diffs reviewed by listening to the CLI render |
| e2e (Playwright) | play while the test blocks the page's thread for 500 ms: zero underruns, the playhead keeps moving. That is the exact failure measured on the Pixel |
| Device | the Pixel profile rerun: 60 s of play with the UI busy, zero underruns, zero dropped notes |
| Listening | the page keeps a "Strudel / Prosody" player switch until the two are close enough |

## 7. Order of work

Each phase ends gated and auditionable (`SPEC.md` §7's rule).

1. **Performance.** Move the value computations out of `strudel.rs`, add the backend, the
   property tests and the parity test. No audio yet; the Strudel snapshots must not change.
2. **First sound.** `prosody-synth` with sampler, oscillators, envelope, low-pass and gain;
   `prosody render` to WAV; the worker and worklet; the page plays drums, bass and melody through
   it behind the player switch, with lighting and roll from the playhead.
3. **Everything the text asks for.** Room, delay, crush, vowel, shape, compressor, swing, nudge,
   chords, the master stage, stems and instant mute/solo. Matched to Strudel by measurement.
4. **The reading** through the player. Then the app build stops shipping Strudel's runtime
   (the page still shows and copies the Strudel text, and links to strudel.cc).
5. **Native audio in the app** (optional): background playback and lock-screen controls.

## 8. Open questions

*Decided by the user, 2026-10-31: the recommended answer to each of 1–3.* After step 4 Strudel's
runtime leaves both the app and the web build (the page still shows and copies the text and links
to strudel.cc; `@strudel/core` may stay a dev dependency for the parity test). "Matching" is
identical notes by the parity test and a timbre close to Strudel's by measurement, not sample for
sample; the reverb may differ. The 28 soundfonts are converted to our own FLAC zones and JSON at
build time if the total stays under about 40 MB, and otherwise stay webaudiofont files, with the
reason written down.

1. **The website after phase 4.** Remove Strudel's runtime from the web build too (one sound
   everywhere, smaller page), or keep it as an option? Recommendation: remove it; keep "open in
   strudel.cc".
2. **How close to Strudel is close enough.** Proposal: identical notes (the parity test), and a
   timbre that is close by ear but not sample-identical — the reverb especially will differ.
   Strudel text then becomes "the same piece, played by Strudel".
3. **Soundfont files.** Keep the webaudiofont files as they are (decoded by the browser, small),
   or convert the 28 licensed instruments to our own FLAC zones at build time (lossless,
   identical decoding everywhere, so golden audio covers them too, and it fits the offline
   download; larger). Recommendation: convert, if the size stays reasonable.
4. **`SPEC.md`.** No mapping changes; the Performance is realisation, where the style may be
   read (I3). The spec's §3 diagram already shows the backend. A short §-note when phase 1 lands.
   *(Added with step 1.)*

## 9. What building step 1 changed

Step 1 is in `crates/prosody-emit/src/perform.rs` (the per-event values both backends call, and the
backend), `crates/prosody-web` (`POST /performance`, JSON, the same in the WebAssembly build) and
`web/test/parity.test.ts`. Every corpus snapshot of the Strudel text is byte-identical. Where it
departs from §3 above, and why:

- **Swing follows Strudel's documentation, not Strudel's output, and the parity test names the
  difference.** Measured with `@strudel/core` 1.2.6 (its haps, not its source): `swingBy(x, n)`
  decides what to delay by where the delayed copy lands as well as where the event began, so an
  event in a slice's first half whose copy lands in the second half sounds twice, and an event in
  the second half whose copy crosses into the next slice never sounds — `s("a b c d").swingBy(0.5, 1)`
  plays a, b, b, c. Copying that would let a palette value add and remove notes (I3) and play notes
  the score does not have (I4), so the Performance delays each event in a slice's second half by
  `x` half-slices and plays it once. The parity test works out from the Performance exactly which
  notes Strudel doubles and drops, and requires those and nothing else to differ; a fixture at
  swing 0.9 exercises it against Strudel. At the palettes' swing (at most 0.33) under the steady
  pulse this needs five syllables to a beat or a stacked line's fitting; under the elastic pulse a
  cycle's slices need not fall on beats, so it can happen at any swing. **Decision for the user:**
  the Strudel text keeps `swingBy` (its bytes may not change in step 1); a later step could write
  swing so Strudel plays it as documented (for example per beat, with `late` on the second-half
  steps only), which changes the text of every swung piece.
- **A note carries more than §3 listed:** its `slot` and its `grid` position in cycles before swing
  (structure, which no palette moves: what the I3 property compares, and what step 2's bar-by-bar
  live edits need), and the `postgain` and compressor the text sets per layer. Parameters the text
  leaves unset (the melody's attack, the drums' release) are `None`, not a default, so the player
  decides those defaults in one place.
- **Values are the text's, to its three decimals.** The Performance carries each value as the text
  prints it (`quantised`), and its onsets as exact fractions of seconds (the printed `.cpm` in
  thousandths, the printed nudge in milliseconds), converted to `f64` only when read. Unrounded
  values would differ from what Strudel reads by up to half a thousandth and fail the 1e-6 parity.
- **Cycles and seconds are two types.** A note's `grid` is `Cycles` (structure) and its exact onset
  `ExactSeconds`; only the piece's clock turns one into the other, so a place in the pattern cannot
  be read as a time (`CLAUDE.md` §5). Each bar carries both its `cycle` and its `at`, so a bar's
  notes can be checked to fall inside it on either clock. A compressor's threshold is `Decibels`.
- **Cues are added by `Performance::read(&Reading)`**, not by the backend: a backend is given the
  score and the style, and a reading needs the poem's lines and the chosen mappings. The route does
  both. A line the reading speaks at neither pace has no cue; it is in the Performance's `unread`
  or `natural_unread` with the reading's reason, as `/reading` lists it, so when the player reads
  from the Performance (§5, step 4) the page can still say what it loses (`CLAUDE.md` §6). Marks
  are one per drum note, at its onset before the nudge, as the trace pattern lights them.
- **The backend is `Perform`** (`Backend<Output = Performance>`); the note's `Lane` is the colour
  enum the emitter already had (`pulse`, `ornament`, `bass`, `rhyme`, `chords`), now public.
- **The parity test reads committed fixtures.** It needs Strudel (in Vitest) and the compiler (in
  Rust), so `crates/prosody-web/tests/parity.rs` makes each piece's pattern and Performance through
  the page's own requests and fails when a committed fixture under `web/test/parity/` is stale
  (`PROSODY_BLESS=1` rewrites them). Sixteen fixtures, about 2.4 MB: every corpus poem under the
  defaults, and settings chosen to reach the parameters and placements the defaults do not — among
  them chords at a density level below one, and a long stanza (a poem of the test's own, since no
  corpus stanza is longer than four lines) eroding under an arrival and handed over gradually with
  no outro. Not every combination is compared with Strudel; the rest is covered only by the Rust
  properties, and a test checks that each case still reaches what it was added for.
  `@strudel/core`, `@strudel/mini`
  and `@strudel/transpiler` 1.2.6 are now direct dev dependencies (they were already installed under
  `@strudel/web`; nothing new is downloaded), and Vitest inlines them so `@kabelsalat/web` resolves
  to its ES build. Strudel reports `.lpf` as `cutoff` in a hap's value.
- **The text and the Performance share the walk, not only the numbers.** `strudel::lines` builds
  each lane's steps once; the text writes them (`Line::write`) and the Performance times them
  (`Line::timed`), so a note cannot fall anywhere its pattern step does not.

## 10. What building step 2 changed

Step 2 is `crates/prosody-synth` (the synth), `crates/prosody-synth-wasm` (it, for the render
worker), `prosody render` in the CLI, and `web/src/player` (the worker, the worklet, the page's
clock) behind the Mixer's **Player** switch, Strudel still the default. Where it departs from §4–§7
above, and why:

- **The synth depends on no other crate of the workspace**, not even `prosody-core` (§4 allowed
  that much). A synth note carries a `tag` — its index in the Performance's notes — and through it
  its `Span`, so every voice is still attributable (I4: `Renderer::sounding_tags`) while the crate
  builds for the browser on its own: 105 kB of WebAssembly, where the compiler's is 5.3 MB. Two
  thin conversions feed it: the CLI from the `Performance` struct, the page from the Performance's
  JSON as a flat array of twelve numbers a note (the layout is in `prosody-synth-wasm`'s docs, and
  a test on each side pins it). Defaults for what the text leaves unset are decided once, in the
  synth (`DEFAULT_ATTACK`, `DEFAULT_RELEASE`).
- **I1 reaches audio by computing its own transcendental functions.** `f64::sin` and `exp2` call
  the platform's maths library, whose last bit differs between macOS, Linux and WebAssembly; the
  synth's `math` module uses only `+`, `*`, `/` and exact rounding. Scheduling is in whole frames:
  a note's time becomes a frame once, when the piece is made, and where it starts, where the piece
  loops and where an edit takes over are integer arithmetic. Golden hashes pin three synthetic
  scenes (`prosody-synth/tests/golden.rs`) and every corpus poem rendered with oscillators
  (`prosody-cli/tests/render.rs`). Whether the WebAssembly build gives the same bits as the native
  one is designed for, not yet tested.
- **The pool steals the voice nearest its end, not the quietest.** "Quietest" is a float compared
  in control flow (`CLAUDE.md` §4); frames left before a releasing voice (or a drum playing out)
  ends is a count, and near enough the same voice. With nothing releasing, the oldest goes. Stolen
  voices and notes whose sound was never loaded are counted and said in the status line.
- **The CLI renders the oscillators only.** The drums are FLAC and the soundfont zones MP3; the page
  has the browser decode both, and decoding either in Rust needs a decoder crate that `CLAUDE.md`
  §10 asks to be argued for. `prosody render` names every note it leaves out. With the default
  palette that is all of them — a header of `synth = "subtractive"` puts bass and melody on
  oscillators — so the soundfont conversion of §8's decision 3 (our own lossless zones) is where
  this should be settled; it was not done in this step. The page still fetches the licensed
  webaudiofont files from the host its Strudel uses, reads their zones as data (never runs them),
  and decodes only the zones the piece's keys need. The zones' loop points were set on the original
  recordings, and an MP3 encoder pads its output by a few frames, so a loop may be a few frames off;
  the synth checks only that a loop lies inside its sample.
- **A zone is chosen by its place in the file, not by when it was loaded.** The page loads a
  font's zones as pieces need them, so the order they reach the synth depends on what played
  before. Where two zones could play a key (overlapping ranges, or a key equally far from two), the
  first *loaded* would make the same poem sound a different recording after a different edit
  history (I1). The page sends each zone's index in its file, and the synth takes the first in the
  file, which is the zone `zonesFor` loaded for that key.
- **A looped note is interpolated as it plays.** The cubic reads two frames either side of the
  read position; near a loop's end those come from the loop's start, and once the note has gone
  round, those before the start come from its end. Read from the buffer instead, every pass of a
  note off its root key dipped or overshot at the seam.
- **Stems and instant mute/solo arrived now, not in step 3.** The worklet mixes the four stems
  anyway, so mute and solo are a stem gain from the first sound. "Instant" is a 5 ms glide
  (256 frames): a gain switched between two samples is a step in a sounding wave, a click, where
  Strudel's mute, applied as each event is scheduled, has none. The master stage here is the soft
  clip alone, the page's own curve (also in `prosody_synth::master` for a file); the glue
  compressor and the reading's duck come with step 3 and step 4. The chords play too: they are the
  same sampler.
- **To be matched by measurement in step 3:** the low-pass is fixed at the Butterworth Q (−3 dB at
  the cutoff, unit-tested); the default attack is 1 ms and release 10 ms; gain and postgain are
  linear factors. None of these has been compared with Strudel's output yet.
- **A restart cuts, a live edit hands over.** A live edit is handed to the worker as a swap: the
  renderer takes over at the first bar boundary *at or after* its frontier (a boundary exactly at
  the frontier has not been rendered yet), from the same bar of the new piece, counted round a piece
  with fewer bars; notes already sounding ring on. A preset or an A/B switch starts from the top as
  Strudel's `hush` does: the voices stop and the worklet's queue is dropped — faded out over its
  next 5 ms, not cut mid-wave — so the new piece is heard within a block rather than after the
  half-second lead. A Play still loading when Stop is pressed, or the player is switched, gives up
  rather than start sound after the Stop; the worker ignores a hand-over with nothing playing.
  During a hand-over the roll's bars and the lit words stay the old piece's (each piece keeps the
  text and bars it was compiled with) until the new one takes over at its bar. Each start of play is a generation;
  the worker sends its flush down the same channel as its blocks, so nothing rendered before a stop
  can arrive after it.
- **The page's clock** is the worklet's count of frames played, moved on by the audio clock since
  the worklet said it, never further than it had queued, less the output's latency. Notes and marks
  are found by binary search on onset; a mark's object is kept while it stays lit, so a frame that
  changes nothing restyles nothing (the measured cost on the Pixel was style recalculation).
- **The reading is not played by the Prosody player yet** (step 4); the status line says so while
  it plays.
- **The player needs its synth beside the page.** Its synth (`synth.js`, `synth_bg.wasm`) is
  generated, not written: `scripts/build-synth.sh` puts it in `web/public` (ignored by git), so the
  page Vite builds into `web/dist` — which `prosody-web` serves, and `build-site.sh` copies — and the
  page `npm run dev` serves both carry it; without it the player says it could not start. *(Step 4's
  review: until then only the static build had it, and once the Prosody player was the only one the
  served page could not play at all; `prosody-web` now also serves `.wasm` and `.flac`.)*
- **The e2e** blocks the page's thread for 500 ms while the Prosody player plays and requires zero
  underruns, a playhead 0.75 s further on after 0.9 s, and output that is not silence (the worklet
  reports its peak). It passes in headless Chromium; the Pixel profile (§6, "Device") is still to
  rerun.


## 11. What building step 3 changed

Step 3 is in `crates/prosody-synth` (`effects`, `formant`, `dynamics`, `delay`, `reverb`, `bus`, the
voice's colour, the linear envelope, the master), `crates/prosody-synth-wasm` (a note's record is
now 22 numbers), `web/src/player` (five stems and the master in the worklet, soundfonts from beside
the page) and `scripts/render-soundfonts.mjs` with its output in `web/public/soundfonts`. Where it
departs from §3–§7 above, and why:

- **Matched by measurement, never by source.** Every number below came from playing single notes
  through Strudel's public API (`superdough`) in headless Chromium, recording what came out of its
  output gain before any page master, and playing the same note through our synth in the same page;
  Chrome's own `DynamicsCompressorNode` was measured offline the same way. The harness lives
  outside the repository (it drives a built site); the numbers are recorded here so a later change
  can be checked against them. Strudel's source was not read.

  | What | Strudel | Ours | How it was matched |
  |---|---|---|---|
  | level, gain 1 sine / soundfont / drum | 0.300 / 0.282 (zone peak 0.933) / file's own (0.891) | the same, to 0.001 | `SOURCE_LEVEL` 0.3 for oscillators and soundfonts, 1 for drums; gain, velocity and postgain linear |
  | envelope, nothing set (oscillator) | full at once, a straight line to 0.6 over 50 ms, 10 ms straight release | identical at 1, 25, 100 ms and the end | `DEFAULT_DECAY`, `DEFAULT_SUSTAIN`; with attack or release set it holds at 1 (measured too) |
  | attack 0.05 / release 0.6 | straight lines | identical per period (0.0104, 0.1602, 0.300; ends at 599.9 ms) | release is linear, not the exponential §4 proposed |
  | sawtooth, square fundamental | 0.848 of the sine's | 0.848 (harmonics 1–20 within 0.06 dB) | `Wave::level`: the Gibbs overshoot normalised away, as Web Audio's oscillator does |
  | low-pass | +2.0 dB at 0.77·fc, +1 dB at fc, −12 dB/octave | within 0.06 dB to the 20th harmonic at 400, 1000, 3000 Hz | Q 1.122 (Web Audio's 1 dB default), not Butterworth |
  | crush 4 / 8 bits on 0.144 | 3 / 37 levels, top 0.125 / 0.1406 | the same | step 2^(1 − bits), rounded, on the note at its level; postgain after (0.0625) |
  | shape 0.1–0.5, five cases | — | fundamental and 3rd harmonic within 0.01 dB | `(1 + k)x/(1 + k|x|)`, `k = 2s/(1 − s)` (musicdsp.org), after the envelope, before postgain |
  | vowel a e i o u, rms on a 110 Hz saw | 0.0461 0.0688 0.0108 0.0333 0.0121 | 0.0453 0.0683 0.0118 0.0324 0.0134 (−0.2…+0.9 dB) | five parallel band-passes, started from the Csound tenor table and fitted to Strudel's response on noise and on the saw (1.2–1.6 dB rms) |
  | room size 1 / 2.5 / 5: RT60 | 1.05 / 2.59 / 5.00 s | 0.99 / 2.57 / 5.08 s | `roomsize` is the RT60; an 8-line FDN |
  | room: tail start re dry burst | −14.5 / −16.3 / −16.9 dB; half send −6 dB | −14.6 / −16.5 / −16.7 dB; half −6 dB | `WET` 0.079 |
  | room: 50–100 ms after the burst | −18.9 / −25.0 / −16.2 dB | −15.1 / −14.6 / −14.4 dB | **differs**: Strudel's tail builds up over ~100 ms, the FDN's is dense from ~30 ms (decision 2: the reverb may differ) |
  | delay 0.5, 0.25 s, feedback 0.35 | echoes 0.150, 0.0525, 0.0184, both sides | the same to 4 places | a feedback line returning only its repeats |
  | compressor −24 dB 7:1 / −8 dB 3:1, steady | 0.3779 / 0.2104 | 0.3731 / 0.2086 (−0.11 / −0.07 dB) | Chrome's node with a 10 dB knee, 3 ms attack, 20 ms release reproduced Strudel's to 0.0001; ours fitted to Chrome over 24 attack/release pairs (0.17 dB rms) |
  | compressor below threshold (makeup) | 0.1160 | 0.1109 (−0.39 dB) | Web Audio's makeup from a quadratic knee; Chrome's knee bends 0.4 dB further |
  | compressor, first 9 ms of a note | 0.31 (rises over ~25 ms) | 0.54 | **differs**: Strudel makes a fresh compressor per note, whose makeup comes in over its first 25 ms; ours is the channel's, already settled |
  | soundfonts: piano C4, bass C2, e-piano E4 and C4 with attack, warm pad C3 | — | residual −41, −61, −46, −48, −37 dB; band powers within 0.2 dB below 3.2 kHz | the same recordings (our FLAC zones), the same zone, the same envelope |

- **Soundfonts are our own FLAC zones** (decision 3). The 28 licensed webaudiofont files became
  282 distinct recordings (35 shared between instruments, stored once by content hash) and 28 JSON
  files: 31.1 MB, under the 40 MB the user set, against 8.4 MB of MP3 inside the original files.
  `web/public/soundfonts/SOURCES.md` records the commit they came from and the banks' licences, as
  the drums' does. The page decodes each FLAC — and each drum — at its own rate in an
  `OfflineAudioContext` of that rate, so no browser resamples it and every browser hands the synth
  the same samples; the synth resamples to the output itself. The Strudel player still fetches the
  webaudiofont files until step 4 removes it.
- **A soundfont key plays the zone holding the key below it**, as Strudel does (measured: at the
  lowest key of every zone's range Strudel sounds the zone beneath, to −46 dB residual, at four
  boundaries in three instruments; elsewhere the zone holding the key). The SoundFont 2 range is
  inclusive; following it would give one key in five or six a different recording, pitched the
  other way. Which recording sounds is timbre, not structure, so the player matches the text
  (`Instrument::zone`, `samples.ts` `zonesFor`).
- **The compressor is the channel's**, as §4 designed, but driven by the voices before their
  `postgain` and applied after it and to their sends — where Strudel's per-note compressor sits
  (its compressed output halves with postgain ½). With notes apart the two agree (table); with
  notes overlapping, the loudest sets the reduction for all, and a note's first 25 ms are up to
  5 dB louder than Strudel's (table). Emulating a fresh Chrome compressor per note would cost a
  compressor a voice for a quirk of the browser.
- **A bus belongs to the piece that sets it up** (review remediation). The compressor comes from
  the style, so a note that asks for none takes it away, and a stop drops it with the tails: kept,
  it went on compressing the next piece, and a piece's sound depended on what had played before it
  (I1). A note that changes its bus changes it from its own frame: the renderer finishes the buses
  up to each starting note, so a hand-over sounds the same at the page's block size and the CLI's.
  A changed setting keeps what is sounding, as §5 asks of tails: the reverb resizes with its tail
  in it, the compressor takes its new curve from the reduction it had reached, and an echo whose
  time (any tempo change) or feedback changes lets the old line ring out its repeats beside the new
  one instead of cutting them. Old voices still sounding send into the new line.
- **Five stems and the duck.** The fifth stem is the reading's, silent until step 4; a block now ends
  with a duck lane (the music's level under the reading, frame by frame, 1 throughout until step 4),
  written by the renderer because the reading's timeline will be the renderer's. The master — in the
  worklet for the speakers, `prosody_synth::master` for a file — is the page's Strudel chain in its
  order: stem gains, the duck on the music only, the glue compressor (the page's settings, our
  compressor), the soft clip. The page's ¼ gain existed only because a `WaveShaperNode`'s curve spans
  ±1 while ours covered ±4; on the sample itself the two cancel. Mute and solo stay the 5 ms glide of
  step 2; the reading's stem answers to the Mixer's voice channel.
- **Swing and nudge** needed nothing new: the Performance's onsets already carry both (§9), and the
  measured envelopes start on the frame Strudel's do.
- **Golden audio** gained an `effects` scene (every vowel, two crushes, a shaped and compressed
  bass, a melody into room and echo); the earlier scenes' hashes moved with the measured levels, the
  linear release, the Q and the master's glue, and the corpus renders grew by the 2 s reverb tail.
- **Not done in this step.** The CLI still renders oscillators only: decoding FLAC in Rust needs a
  decoder crate to argue for (`CLAUDE.md` §10), now that every sample is FLAC. The device profile
  (§6, "Device") was not rerun: the Pixel was not connected when this step was built.

## 12. What building step 4 changed

Step 4 is in `crates/prosody-synth` (`speech`: the reading's lines on the fifth stem, and the duck),
`crates/prosody-synth-wasm` (the worker's calls for them), `web/src/player/reader.ts` (the reading's
timing, in the render worker), `web/src/player/speech.ts` (the page's side of it), the worklet's
duck for the browser's voice, and the removal of Strudel's runtime from the page and so from the app.
Where it departs from §5 and §7 above, and why:

- **The synth places lines; the worker decides when.** `Renderer::speak` puts a recorded line (mono
  PCM at its own rate, resampled by the sampler's Catmull–Rom read) on the speech stem from an output
  frame and returns the frame it ends on; `cut_speech` fades every line out over 256 frames from a
  frame and lets go of those placed after it. The duck lane is written from the same placements:
  the music falls towards the duck's level (0.6, the page's `DUCK`) from where a line's speech begins
  with a 0.08 s time constant, and rises back with 0.15 s from 0.05 s after the line ends and its hold
  (the pause before the stanza's next line) — the page's own Strudel-era automation, as a one-pole
  approach frame by frame, so a stanza ducks the music once. A line cut off releases the duck from
  the cut, its hold not kept. A placement holds its recording, so a
  line the worker lets go while it is placed still plays. Hand-worked tests pin each part, and a
  render with no reading is the same bits as before (the golden hashes did not move).
- **The reading's timing is the page's chaining, in the worker, on output frames.** `reader.ts`
  finds the cues of the pace being read that fall in each block before it is rendered — on the piece
  that plays there, round its loop — and places the line. The natural pace's chain is
  `reading/index.ts`'s `naturalCue` and friends, moved unchanged in what they decide (§5 said "at
  first"); "now" is the render frontier, so a line that is late starts where nothing is rendered yet,
  never inside what the worklet has queued. A fitted cue cuts the line before it at its bar, as the
  browser's voice always was.
- **Who reads each line is the page's call (the cast).** Only the page knows which engines have
  loaded and which lines they have rendered, so it sends, per piece, each line's render key — or null
  for the browser's voice, or nothing for a line not read — and sends again whenever that may change
  (a render arrives, a refit is ready, an engine loads or fails, a setting changes). The worker asks
  for the recordings it is cast with and does not hold (`want`), and says which it let go
  (`dropped`: those no piece handed over casts; a Stop keeps them, for the Play after); the page sends a piece's rendered lines *before* the piece, so its first line is in the
  worker when its first block is rendered — asked for after, it waited out the whole half-second lead.
  The e2e checks that the first line is placed on its frame.
- **The browser's voice stays on the page and says it is unsynchronised.** The worker tells the
  page when such a line is due; the page speaks it when its playhead gets there (each animation
  frame), notes "read by the browser voice, off the playhead and not in step with the music", and,
  at the natural pace, tells the worker the playhead frame it ended on so the chain goes on from
  there. Its line is due at the time the chain gives, not held to the render frontier, since nothing
  of it is rendered. Its duck cannot be in the block's lane, so the page asks the worklet for it, and
  the worklet approaches it with the renderer's two time constants.
- **A hand-over cuts the old reading where the new piece takes over, and keeps the breath.** The
  Strudel page cut the reading the moment an edit was evaluated; now the old version's lines play on
  until the new piece's first frame (at most a bar and the lead later), where they fade out and its
  pending lines are dropped. The new piece's chain starts from where the old one's last line ended
  (cut there if it ran over), so its next stanza still waits its silent bar. A change of voice,
  engine, pace or language lets go of everything from the frontier on: what the worklet already holds
  (about half a second) still plays, and the reading picks up at the next cue. The duck switch acts
  from the frontier for rendered lines and at once for the browser's voice; the reading's level and
  its mute are the speech stem's gain, at once.
- **Strudel's runtime is gone from the page and the app** (decision 1). `@strudel/web`,
  `@strudel/draw` and `@strudel/soundfonts` are no longer dependencies; the import map, the globals
  shim, the vendored files, the warm-up, the parse-ahead, the dropped-tick watcher, the `Drawer` and
  the Player switch went with them, and `build-site.sh` removes an older build's `vendor/` so the app
  cannot ship it. `@strudel/core`, `@strudel/mini` and `@strudel/transpiler` stay dev dependencies for
  the parity test alone. The page shows, copies and shares the Strudel text and its strudel.cc
  instructions as before (the drum map at `drums/strudel.json` is unchanged). **The page no longer
  runs any code it compiles**: there is no `eval` left — the player reads the Performance as data.
- **One check went with Strudel.** The e2e that read the shipped `@strudel/soundfonts` table to
  confirm `gm_x:n` named the licensed file guarded the Strudel the page shipped; there is none now,
  and strudel.cc plays whatever its own, current table says, which no pinned test could reach. In its
  place, every soundfont the compiler can name must be beside the page as our zones.
- **The Pattern view still shows the reading's cue pattern** under the music's, as text: nothing
  evaluates it, but it says in Strudel's terms when each line is read.
- **Review remediation.** A line is placed when the block its recording begins in is about to be
  rendered, not as soon as the chain knows when it falls: the natural pace placed a stanza's held
  lines all at once, so a stanza of more than `MAX_PLACED` (64) lines overflowed the synth, and a
  line the synth refuses is no longer told to the page as spoken. A cut releases the duck from the
  cut (after the 0.05 s delay), not after the line's hold, and a line cut before its speech begins
  ducks nothing. A hand-over cuts a line of the browser's voice that is still sounding when the
  playhead reaches the cut, and the Mixer's duck switch reaches that voice's duck at once, as the
  bullet above says. Play loads the reader's saved live engine (Kokoro, Piper, Kitten), as the
  Strudel page did; without it the browser's voice read until a setting changed. `Renderer::speaking`
  went: nothing but tests read it, and its claim to attribute the speech stem did not hold once line
  numbers were reused — the worker attributes each placed line by its `spoken` report.
- **The device profile (§6, "Device") is still to rerun:** the Pixel was not connected when this
  step was built. The rebuilt Android app was checked on an emulator instead: 18 s of The Tyger with
  zero underruns and every line on its cue (`PLAN.md`'s step 4 entry).

## 13. What building step 5 changed

Step 5, for Android (iOS later), is `crates/prosody-player` (the engine), `src-tauri/src/player`
and `src-tauri/src/media.rs` (the native player and its bridge), `PlaybackService.kt` and
`MediaPlugin.kt` in the Android project, and `web/src/player/native.ts` (the page's side). Where it
departs from §5 and §7 above, and why:

- **The render worker's logic moved into a crate both players run.** The native player needs
  everything the render worker did — render ahead, place the reading's lines as their cues come
  round, ask the page for the recordings it lacks, say where each piece plays from — and two copies
  of the reading's chaining (TypeScript in the browser, Rust in the app) would drift, which §3's
  "one source of the numbers" forbids for the notes. So `reader.ts` and the worker's bookkeeping
  became `prosody-player` (`Engine`, `reader`), pure like the synth, and `prosody-synth-wasm` now
  wraps it: the worker only moves the page's messages in and blocks and events out. The page's
  messages are unchanged, except that a piece's cues now travel as records (seven numbers a cue,
  `records.rs`) as its notes already did. The reader's decisions are the same; its times are whole
  output frames rather than seconds compared with a tolerance (`CLAUDE.md` §4), and its tests moved
  with it, the same cases and numbers. The browser's synth grew from 105 kB to 257 kB of
  WebAssembly (the reader, the events' JSON, the strings of a cast). The worklet's mix is also in the
  crate (`Mixer`), for the app's output; the worklet keeps its TypeScript, since a worklet cannot
  wait for a module to load, and the two are pinned by the same hand-worked numbers.
- **The page drives the native player with the worker's messages, over `invoke`, and polls.** In
  the app (`__TAURI_INTERNALS__` present) the page opens the native player instead of the worker and
  the worklet (`player_open`), posts the same messages (`player_post`), sends the worklet's mute,
  solo and duck (`player_mix`), and asks for the output's report and the engine's events every 40 ms
  while it plays, every half second while it does not (`player_poll`; a message posted brings the
  next ask forward). Polled, not pushed: a paused WebView would pile up a call for every report while
  the screen is off; the app keeps what happened (up to 4 096 events, and says how many it let go).
  It also says when it is hidden or shown (`player_view`), for the lead below.
  A buffer cannot cross Android's WebView bridge — Tauri sends every call there as JSON, since that
  WebView cannot read a request body — so samples, records and recorded lines go as base64 of their
  little-endian bytes. The page still decodes the samples (the same FLAC, at its own rate, §11), so
  the app's synth gets the same numbers as the browser's.
- **The output is `cpal`, not `oboe`.** AAudio called directly (or an `AudioTrack` through JNI) is
  foreign functions and raw sample pointers, which this workspace's `unsafe_code = "forbid"` rules
  out; `oboe` is the same plus a C++ build. `cpal` (Apache-2.0) wraps AAudio — and Core Audio, for
  iOS later — in a safe callback over `&mut [f32]`, and runs on the desktop where the app is
  developed. It brings `jni` 0.22 and `ndk-context` beside the `ndk` the WebView already pulled in,
  and needs **API 26** (AAudio): the app's minimum SDK rose from 24 to 26 (Android 8.0).
- **Two threads, as in the browser.** A render thread does the worker's job: a round every 5 ms
  (sooner when a message arrives), rendering 2 048-frame blocks while it is less than its lead ahead
  of the output, sending them down a bounded channel. The audio callback does the worklet's: takes
  what has arrived, reads the stems' gains from atomics, mixes, writes its report to atomics. It
  takes no lock and frees nothing (finished blocks go back to be rendered into), and what the channel
  has no room for waits in the render thread's outbox, in order. **The lead is at least twice the
  output's largest buffer** (`Engine::keep`), since a phone's buffer can grow, and a lead shorter
  than a buffer runs dry inside it. Every frame of lead is also a frame later that a live edit is
  heard (a swap takes over at the first bar after the frontier), so the lead does not stay wide for
  the rest of the session: while the page is shown it is twice the largest buffer *since it was last
  shown*, back to the half second once the buffers are small again; while it is hidden (the screen
  off) it is twice the largest buffer *ever seen*, widened the moment the page hides rather than
  after the buffer has grown and the queue run dry inside it. A lead that narrows lets go of nothing
  already rendered — the engine waits for the output to catch up. If the page's hiding never reaches
  the app, the lead still widens as the buffer grows, as before. An output xrun (cpal's `Xrun`) is
  counted and said with the losses ("the phone's audio output glitched"), not reported as a failure.
- **A failed output is reopened by the render thread.** Any other stream error — on Android a route
  change (headphones or Bluetooth in or out) or the audio server restarting, which cpal reports as
  `DeviceNotAvailable` — ends an AAudio stream's callbacks for good, and AAudio must not be reopened
  from its own callback. The error callback only marks the output lost (and counts one glitch: what
  was in flight is not heard); the render thread, which holds the stream, drops it and opens the
  default output again at the same rate (the engine's, and the rate the page decoded its samples
  at; AAudio resamples), on a new channel to a fresh mixer, and the engine goes on from its frontier
  in the same start of play (`Engine::reopened`: a flush tells the new mixer where its count starts).
  It is reopened only when something plays, and if it will not open the page is told once and it is
  tried again every second. Before, the player stayed silent until the page was reloaded.
- **Nothing playing, the output pauses and the render thread sleeps.** After about a second with
  nothing playing and nothing to send (long past a stop's fade, so Stop then Play does not cycle the
  stream), the render thread pauses the stream and parks until a command wakes it; a Play resumes the
  stream in the same round that renders its first blocks. An open AAudio stream keeps the phone's
  audio path powered, and before API 30 nothing freezes an idle app, so the silence it played after
  the first Play, the 5 ms loop and the page's 40 ms ask all ran for as long as the app was open.
- **The screen off is a foreground service.** Android freezes or kills an app with nothing in the
  foreground; one that plays music says so with a service of type `mediaPlayback` and its media
  notification (`PlaybackService`, platform `MediaSession` and `Notification.MediaStyle`, so no new
  Android library). It holds a partial wake lock while playing, for the render thread, which must run
  between the output's own wake-ups; takes the audio focus (and stops when another player takes it);
  stops when headphones are pulled out; and stops when the app is swiped away. None of it asks the
  user anything: a media session's notification needs no `POST_NOTIFICATIONS`. **Prosody has no
  pause**, so the notification and lock screen show Stop while playing and Play after: Stop (or a
  headset's pause) stops the music in the app at once — the page may be asleep — and the page follows
  when it wakes; Play asks the page to play, since only the page can make the piece. That ask is kept
  for 5 s: an awake page hears it within half a second, and a page asleep (the screen off) does not
  start the music whenever it next wakes, perhaps hours after whoever pressed Play gave up; a stop
  after it cancels it. So with the page asleep the lock screen's Play does nothing (Android shows it
  anyway; hiding it while the page sleeps would need the page to say it sleeps, which a paused page
  cannot). The audio focus is asked for and checked: refused (a phone call is on), the music stops
  at once rather than playing over the call. The service keeps the latest state asked for, so a Stop
  that arrives while Android is still creating the service (after a Play that started it) brings it
  up stopped and gone, not playing with a wake lock and the focus that nothing would release. The
  notification hears of each start and stop while the player is held, so a headset's stop and the
  page's next Play reach it in the order they happened.
- **Debug builds optimise the synth.** Measured on the emulator, a debug app's render thread took
  36% of a core at opt-level 0, and the system's work as the screen went off or came back on starved
  it past its half-second lead (the queue ran dry, 3 to 47 buffers at a time); at opt-level 2 it
  takes about 10%, and in six screen-offs over three minutes the queue did not run dry once.
  `prosody-synth` and `prosody-player` build at opt-level 2 in the dev profile, as `prosody-lts`
  already did.
- **What the browser's voice and the engines need, the page still does.** A line the browser's own
  voice reads is spoken by the page when its playhead reaches it, so with the screen off it is not
  heard, and at the natural pace the chain waits for it (the page says so when it wakes). A neural
  engine renders lines in the page: the lines rendered before the screen went off are in the app and
  go on being read each time round; a line not yet rendered waits.
- **Verified on the emulator, not on the Pixel**, which was not connected (`PLAN.md`'s step 5 entry
  has the runs). With the screen off the app's process kept its AAudio stream started, its wake lock
  held and its media session `PLAYING`; the page, woken, found every line of the reading on its cue
  and the queue never dry, and the playhead within half a second of the wall clock over a minute off
  — except in one run of 3 min 15 s, six screen-offs, where the emulator's output reported two xruns
  and the playhead was 3.5 s behind: the output itself stalling, below the queue; a headset pause dispatched with the screen off stopped
  the music and the session, and the page showed it stopped when woken. The reopened output, the
  idle pause, the narrowing lead and the service's fixes after review are tested on the host (a
  pretend output for the render thread's rounds) and compiled for Android, not yet run on a device:
  a route change on the emulator and on the Pixel is the check still owed.
