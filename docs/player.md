# The player — design

*Draft, 2026-10-31. Status: step 1 (the Performance) built; steps 2–5 not started. The open
questions of §8 are decided (§8). What building step 1 changed in this design is in §9.*

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
lock-screen controls. The crate boundary is drawn so that move needs no change to it.

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
- **Cues are added by `Performance::read(&Reading)`**, not by the backend: a backend is given the
  score and the style, and a reading needs the poem's lines and the chosen mappings. The route does
  both. Marks are one per drum note, at its onset before the nudge, as the trace pattern lights them.
- **The backend is `Perform`** (`Backend<Output = Performance>`); the note's `Lane` is the colour
  enum the emitter already had (`pulse`, `ornament`, `bass`, `rhyme`, `chords`), now public.
- **The parity test reads committed fixtures.** It needs Strudel (in Vitest) and the compiler (in
  Rust), so `crates/prosody-web/tests/parity.rs` makes each piece's pattern and Performance through
  the page's own requests and fails when a committed fixture under `web/test/parity/` is stale
  (`PROSODY_BLESS=1` rewrites them). Thirteen fixtures, about 1.6 MB: every corpus poem under the
  defaults, and settings that reach every parameter and placement. `@strudel/core`, `@strudel/mini`
  and `@strudel/transpiler` 1.2.6 are now direct dev dependencies (they were already installed under
  `@strudel/web`; nothing new is downloaded), and Vitest inlines them so `@kabelsalat/web` resolves
  to its ES build. Strudel reports `.lpf` as `cutoff` in a hap's value.
- **The text and the Performance share the walk, not only the numbers.** `strudel::lines` builds
  each lane's steps once; the text writes them (`Line::write`) and the Performance times them
  (`Line::timed`), so a note cannot fall anywhere its pattern step does not.
