// Playing the piece with Strudel: starting it, parsing ahead, the master stage, loading the sounds a
// pattern uses, and Play and Stop.
import { closeLoader, loaderEngine, openLoader } from '../loader';
import { piece, summaryOf } from '../piece';
import { cuesOf, ensureEngine, prerender, quietReading, speech } from '../reading';
import { beat, hearing, playing, readingNote, readingSettings, settings, status } from '../store';
import { barsOf, clearRoll, fitRoll, lightUp, startDrawing, watch } from '../watch';
import { audioContext, evaluate, strudel, type Pattern, type Repl } from './globals';

// Where the sounds come from. The drum machines are the same sample map strudel.cc loads; without
// it every `.bank(...)` asks for a sample nobody registered and the drums are silent.
const SAMPLES = 'https://strudel.b-cdn.net';
const SOUNDFONTS = 'https://unpkg.com/@strudel/soundfonts@1.3.0/dist/index.mjs';

let strudelReady = false;
/** The running Strudel, once started. */
export let repl: Repl | null = null;

async function startStrudel(): Promise<void> {
  const started = await strudel.initStrudel({
    prebake: () => Promise.all([
      strudel.samples(`${SAMPLES}/tidal-drum-machines.json`, `${SAMPLES}/tidal-drum-machines/machines/`, {
        prebake: true,
        tag: 'drum-machines',
      }),
      // Imported only now: the import map answers its Strudel imports from the globals that
      // `initStrudel` has just created.
      (import(/* @vite-ignore */ SOUNDFONTS) as Promise<{ registerSoundfonts: () => unknown }>)
        .then((soundfonts) => soundfonts.registerSoundfonts()),
    ]),
  });
  // The pattern says its tempo with `.cpm`, which in `@strudel/core` assumes one cycle a second;
  // the scheduler starts at half that. strudel.cc's REPL rewrites `.cpm` against its scheduler,
  // but this page evaluates with a plain `eval`, so it sets the scheduler instead — without this
  // every piece played at half its tempo. `reading::SCHEDULER_CPS` must say the same number.
  started.setCps(1);
  repl = started;
  await startDrawing();
  // `initStrudel` loads Strudel's effect worklets (crush, shape, …) on the *next* mousedown, but
  // it is called from inside the Play click, after that click's mousedown — so the first play ran
  // without them and every drum layer, which carries `.crush`, failed to build. Load them now.
  await strudel.initAudio();
  honourNudge();
  rememberParses();
}

// ---- Parsing ahead ----
//
// Evaluating a pattern is almost all Strudel parsing its mini-notation strings: "Patterns" has
// about 1,200, and parsing them takes about a second, during which the scheduler cannot tick — so
// a live edit while it plays used to skip a second of music. Two things keep the page's thread
// free. Strudel turns every string into a pattern through the one parser `setStringParser` names
// (@strudel/core 1.2.6, pattern.mjs `reify`; `initStrudel` sets it to `mini`), so the page sets
// a parser that remembers what it has parsed: Strudel's patterns are immutable values, the same
// string always parses to the same pattern, and an edit to one stanza leaves the strings of every
// other voice byte-identical (I2), so after an edit almost every string is one already parsed.
// And before each evaluation the page parses the strings it has not seen, a few milliseconds of
// them at a time between the scheduler's ticks, so the evaluation itself only looks them up.
// Entries no evaluation has asked for in the last PARSES_KEPT plays are let go.
const parsed = new Map<string, { pattern: Pattern; play: number }>();
let parsePlay = 0;
const PARSES_KEPT = 3;      // the piece playing, the other side, and the one before
const PARSE_SLICE_MS = 8;   // well inside the scheduler's 50 ms tick
function rememberParses(): void {
  strudel.setStringParser((text) => {
    let entry = parsed.get(text);
    if (!entry) {
      entry = { pattern: strudel.mini(text), play: parsePlay };
      parsed.set(text, entry);
    }
    entry.play = parsePlay;
    return entry.pattern;
  });
}
export async function parseAhead(...codes: string[]): Promise<void> {
  parsePlay += 1;
  let slice = performance.now();
  for (const code of codes) {
    // Every string literal: the mini-notation of `note("…")`, `s("…")` and the rest, and a few
    // plain names (`.bank("…")`), which parse to themselves. One that does not parse is left for
    // the evaluation to refuse, with Strudel's own message.
    for (const [, text = ''] of code.matchAll(/"([^"\\\n]*)"/g)) {
      const entry = parsed.get(text);
      if (entry) { entry.play = parsePlay; continue; }
      try { parsed.set(text, { pattern: strudel.mini(text), play: parsePlay }); } catch { /* left for the evaluation */ }
      if (performance.now() - slice > PARSE_SLICE_MS) {
        await new Promise((resolve) => setTimeout(resolve, 0));
        slice = performance.now();
      }
    }
  }
  for (const [text, entry] of parsed) if (entry.play <= parsePlay - PARSES_KEPT) parsed.delete(text);
}

// Strudel's sampler starts a sample `nudge` seconds late; its oscillators and soundfonts ignore
// the field. The palette's humanise writes `.nudge` on every layer, so every other sound is
// wrapped to start that much later too — one meaning of `nudge` for the whole piece.
function honourNudge(): void {
  for (const [name, sound] of Object.entries(strudel.soundMap.get())) {
    if (sound.data?.type === 'sample' || sound.data?.nudged) continue;
    const trigger = sound.onTrigger;
    strudel.registerSound(name, (time, value, ...rest) =>
      trigger(time + (Number(value?.nudge) || 0), value, ...rest), { ...sound.data, nudged: true });
  }
}

// ---- The master ----
//
// Everything Strudel plays, summed, through two fixed stages that are page-side and not palette.
// Strudel compresses each event's own chain; nothing in it watches the total, and a villanelle
// sounds many lines at once.
//
// 1. A gentle compressor, the glue. It cannot be the ceiling: its 2 ms attack lets a drum onset
//    through above its threshold, and Chrome adds makeup gain after it (a steady 0.5 sine comes out
//    at 0.61), so a DynamicsCompressor alone measured peaks of 1.18 on the Wilde villanelle.
// 2. A soft clipper, which is. A WaveShaper clamps any input beyond its curve's ends to those ends,
//    so its output can never exceed the curve's largest value: transparent below CLIP_KNEE,
//    bending smoothly towards CLIP_CEILING above it, and flat at the ceiling for anything that
//    arrives up to CLIP_RANGE (and beyond — clamped). No oversampling, because its interpolation
//    filter can overshoot the curve and the point of this stage is that nothing does.
export const CLIP_KNEE = 0.6, CLIP_CEILING = 0.95, CLIP_RANGE = 4;
export function softClipCurve(points = 4097): Float32Array<ArrayBuffer> {
  const curve = new Float32Array(points);
  const bend = CLIP_CEILING - CLIP_KNEE;
  for (let i = 0; i < points; i++) {
    const v = (i / (points - 1) * 2 - 1) * CLIP_RANGE;
    const a = Math.abs(v);
    curve[i] = Math.sign(v) * (a <= CLIP_KNEE ? a : CLIP_KNEE + bend * Math.tanh((a - CLIP_KNEE) / bend));
  }
  return curve;
}
let limited: GainNode | null = null;
function limitTheMaster(): void {
  const gain = strudel.getSuperdoughAudioController().output?.destinationGain;
  if (!gain || limited === gain) return;
  const ctx = audioContext();
  const glue = new DynamicsCompressorNode(ctx, {
    threshold: -6, knee: 6, ratio: 8, attack: 0.003, release: 0.15,
  });
  // The shaper's curve spans −1…1; scaling by 1/CLIP_RANGE first lets it cover ±CLIP_RANGE.
  const into = new GainNode(ctx, { gain: 1 / CLIP_RANGE });
  const clip = new WaveShaperNode(ctx, { curve: softClipCurve(), oversample: 'none' });
  // The reading's duck goes first, so the glue and the ceiling see the ducked sum.
  speech.duckGain = new GainNode(ctx, { gain: 1 });
  gain.disconnect();
  gain.connect(speech.duckGain).connect(glue).connect(into).connect(clip).connect(ctx.destination);
  // A rendered voice joins the music at the glue, after the duck: through the same master stage,
  // at the reading's own level.
  speech.bus?.disconnect();
  speech.bus = new GainNode(ctx, { gain: readingSettings.value.level });
  speech.bus.connect(glue);
  limited = gain;
}

// Every sound a pattern asks for, loaded before it starts: a soundfont or sample still loading
// when its first note is due is skipped, so the first bars would play with holes in them. Each is
// triggered once at zero gain, through Strudel's own path, which loads and caches it.
const warmed = new Set<string>();
/** The sounds a pattern asks for: each note of each pitched sound, each drum of each bank. */
export function soundsOf(code: string): Record<string, unknown>[] {
  const wanted: Record<string, unknown>[] = [];
  for (const line of code.split('\n')) {
    const pattern = (name: string) => (line.match(new RegExp(`(?:^|[.\\s])${name}\\("([^"]*)"\\)`)) || [])[1];
    const tokens = (text: string) => text.replace(/[<>[\],]/g, ' ').split(/\s+/)
      .map((t) => t.split('@')[0] ?? '').filter((t) => t && t !== '~');
    const notes = pattern('note');
    if (notes !== undefined) {
      const sound = pattern('sound');
      for (const note of new Set(tokens(notes))) wanted.push({ s: sound, note: Number(note) });
    }
    const drums = line.trimStart().startsWith('s(') ? pattern('s') : undefined;
    if (drums !== undefined) {
      const bank = pattern('bank');
      for (const s of new Set(tokens(drums))) wanted.push({ s, bank });
    }
  }
  return wanted;
}
async function warmUp(code: string): Promise<void> {
  const ctx = audioContext();
  await Promise.all(soundsOf(code).map((value) => {
    const key = JSON.stringify(value);
    if (warmed.has(key)) return null;
    warmed.add(key);
    return strudel.superdough({ ...value, gain: 0 }, ctx.currentTime + 0.05, 0.05).catch(() => warmed.delete(key));
  }));
}

// ---- Falling behind ----
//
// What the scheduler dropped. Strudel's scheduler (the cyclist bundled in @strudel/web 1.3.0)
// queries the pattern a tick at a time; a tick it reaches too late it skips whole, saying only
// `console.log("skip query: too late")` — every note and every reading cue in that tick is lost,
// and nothing on the page would know. It has set `lastBegin`/`lastEnd` to the skipped span just
// before it logs, so the watcher asks the cue pattern which lines fell in it. Both are said in the
// status line (CLAUDE.md §6: a fallback is visible), once a second at most.
interface Behind {
  ticks: number;
  lines: Set<number>;
  cues: Pattern | null;
  said: string;
  timer: ReturnType<typeof setTimeout> | null;
}
let behind: Behind | null = null;   // for the pattern now playing
const pageLog = console.log.bind(console);
console.log = (...args: unknown[]) => {
  if (args[0] === 'skip query: too late' && behind) fellBehind(behind);
  pageLog(...args);
};
function fellBehind(record: Behind): void {
  record.ticks += 1;
  const scheduler = repl?.scheduler;
  if (record.cues && scheduler) {
    try {
      for (const hap of record.cues.queryArc(scheduler.lastBegin, scheduler.lastEnd)) {
        if (hap.hasOnset()) record.lines.add(Number(hap.value?.n) + 1);
      }
    } catch { /* the lines are not known; the ticks still are */ }
  }
  reportSoon(record);
}
// A new pattern's record takes over what the old one had not yet said — ticks skipped while the
// new pattern was being prepared still count against the old record, and its report would
// otherwise be dropped with it.
function behindFrom(previous: Behind | null, cues: Pattern | null): Behind {
  const record: Behind = { ticks: 0, lines: new Set(), cues, said: '', timer: null };
  if (previous?.timer) {
    clearTimeout(previous.timer);
    record.ticks = previous.ticks;
    record.lines = previous.lines;
    reportSoon(record);
  }
  return record;
}
function reportSoon(record: Behind): void {
  record.timer ??= setTimeout(() => {
    record.timer = null;
    if (record !== behind) return;
    const lines = [...record.lines].sort((a, b) => a - b);
    const lost = ' · Strudel fell behind: ' + record.ticks + ' tick(s) not played, their notes unheard'
      + (lines.length ? '; not read: line ' + lines.join(', ') : '');
    console.warn('[scheduler]' + lost);
    status(record.said + lost, true);
  }, 1000);
}

/** After a change of pace while it plays: the cues the watcher asks are the new pace's. */
export function followPace(): void {
  if (behind) behind.cues = speech.cues?.[readingSettings.value.pace === 'fitted' ? 'fitted' : 'natural'] ?? null;
}

// ---- Play and stop ----

export async function play(): Promise<void> {
  if (!piece.latest) return;
  if (!strudelReady) {
    status('loading Strudel, drum machines and soundfonts…');
    openLoader({
      title: 'Getting the instruments ready',
      text: 'Strudel, the drum machines and the soundfonts: a few MB, once. Then it plays.',
    });
    try {
      await startStrudel();
      strudelReady = true;
    } catch (error) {
      status('could not start Strudel: ' + (error as Error).message, true);
      closeLoader();
      return;
    } finally {
      if (loaderEngine() === null) closeLoader();
    }
  }
  try {
    // The generated code is a `stack(...)` expression built from Strudel's own functions, which
    // `initStrudel` puts in global scope. Mini-notation inside `s("…")` and `note("…")` is parsed
    // by those functions, so no transpiler step is needed.
    const code = piece.latest;
    const side = hearing.value;
    const reading = piece.readings[side];
    const trace = piece.traces[side];
    if (code.length > 100000) status('preparing the pattern…' + (playing.value ? ' · playing' : ''));
    await parseAhead(code, reading?.pattern ?? '', reading?.natural ?? '', trace?.pattern ?? '');
    if (code !== piece.latest) return;   // a newer version arrived while parsing; it plays itself
    const music = evaluate(code);
    const generation = ++speech.generations;
    // The reading is evaluated apart from the music: a cue pattern Strudel refuses costs the
    // reading, never the piece.
    let cues: ReturnType<typeof cuesOf> | null = null;
    let unread = '';
    if (reading && (reading.cues.length || reading.lines.length)) {
      try { cues = cuesOf(reading, generation); } catch (error) {
        unread = ' · the reading could not be cued: ' + (error as Error).message;
        console.warn('[reading] cue pattern refused:', error);
      }
    }
    const pattern = cues ? strudel.stack(music, cues.both) : music;
    // The trace is evaluated but never played: the drawing queries it for which syllable sounds.
    let nextTrace: Pattern | null = null;
    const nextBars = barsOf(trace);
    try { nextTrace = trace ? evaluate(trace.pattern) : null; } catch (error) {
      console.warn('[watch] trace pattern refused:', error);
    }
    limitTheMaster();
    status('loading the instruments this pattern uses…');
    await warmUp(code);
    if (code !== piece.latest) return;   // a newer version arrived while loading; it plays itself
    // From here the old pattern's cues are stale: drop the queued ones and the line being read.
    speech.generation = generation;
    quietReading();
    speech.playing = reading;
    speech.cues = cues;
    // The chosen engine loads now if it has not; until it has, the browser's voice reads.
    ensureEngine();
    prerender();
    if (piece.fromTheTop && playing.value && typeof strudel.hush === 'function') strudel.hush();   // restart at cycle 0
    piece.fromTheTop = false;
    behind = behindFrom(behind, cues?.[readingSettings.value.pace === 'fitted' ? 'fitted' : 'natural'] ?? null);
    pattern.play();
    watch.tracePattern = nextTrace;
    watch.heardBars = nextBars;
    // `play()` hands the scheduler its pattern a moment later (@strudel/web's `initDone`), and the
    // drawer reads the scheduler's pattern as it starts, so it starts — once — after that. On a
    // later play it only looks ahead again, or the roll would show the old piece's notes to come.
    if (watch.drawer) setTimeout(() => {
      if (!repl?.scheduler.pattern || !watch.drawer) return;
      if (watch.drawing) watch.drawer.invalidate(repl.scheduler);
      else { watch.drawer.start(repl.scheduler); watch.drawing = true; }
    }, 60);
    fitRoll();
    playing.value = true;
    showBeat();
    // A line the reading leaves out is said, not left for the listener to miss (SPEC.md §4.7).
    const missing = readingSettings.value.pace === 'fitted' ? reading?.unread : reading?.natural_unread;
    const lost = missing?.length
      ? ' · not read: ' + missing.map((line) => 'line ' + (line.line + 1) + ', ' + line.why).join('; ')
      : '';
    if (lost) console.log('[reading]' + lost);
    const said = side + ' · ' + summaryOf(piece.latest) + ' · playing' + unread + lost;
    status(said, !!unread);
    if (behind) behind.said = said;
  } catch (error) {
    status('Strudel refused the pattern: ' + (error as Error).message, true);
  }
}

export function stop(): void {
  if (typeof strudel.hush === 'function') strudel.hush();
  speech.generation = ++speech.generations;   // nothing the stopped pattern cued is read
  quietReading();
  speech.playing = null;
  speech.cues = null;
  readingNote.value = '';
  behind = null;
  watch.drawer?.stop();
  watch.drawing = false;
  watch.tracePattern = null;
  watch.heardBars = null;
  lightUp(null);
  clearRoll();
  playing.value = false;
  showBeat();
  status('stopped');
}

// ---- The beat on the Play button ----
// Under the steady pulse a cycle is a beat, so the trace's cycles a minute are the tempo; under
// the elastic one there is no single beat to show, and the button stays still.
export function showBeat(): void {
  const trace = piece.traces[hearing.value];
  const steady = (settings.value[hearing.value]?.pulse ?? 'steady') !== 'elastic';
  beat.value = playing.value && steady && trace && trace.cpm > 0 ? 60 / trace.cpm : null;
}
