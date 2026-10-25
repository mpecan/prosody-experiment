// ---- The reading ----
//
// Where a line is read comes from two cue patterns (`prosody_emit::reading`), both stacked with
// the music so a change of pace needs no new pattern: `pattern`, the fitted pace, a line a bar;
// and `natural`, the natural pace, whose cues are the earliest bar a line — a stanza's first,
// under `breath` — may begin in. Each is `n("<0@3 1@4 ~@3 2@4 …>").cpm(…)`, one element per bar,
// each as long as the music's bar (under the elastic pulse, a cycle a bar), at the music's cycles
// per minute, so Strudel's scheduler fires a cue as its bar begins. Its callback is attached with
// `Pattern.onTrigger(fn, true)` (@strudel/core 1.2.6, pattern.mjs: the `true` makes it the cue's
// only output). Strudel calls it ahead of time — `(hap, currentTime, cps, targetTime)`, in
// audio-context seconds — and `targetTime` is when the bar begins on the audio clock: a rendered
// line is started then, exactly, by an AudioBufferSourceNode; the browser's voice, which cannot be
// scheduled, by a timer that waits out the difference, as Strudel's own `onTriggerTime` does.
//
// A neural engine renders every line ahead of time, after each compile, and keeps what it made by
// engine, voice, speed and text, so a recompile reuses every line it did not change. A line the
// recorded readings have no file of — an edited line, a poem of the writer's own — is read by a
// live engine instead, and said (speakerOf).
import type { Cue, NaturalLine, Reading, SpokenLine } from '../answers';
import { closeLoader, loaderBack, loaderEngine, loaderFailed, loaderProgress, loaderSays, openLoader } from '../loader';
import { piece, showPattern } from '../piece';
import { load, loadJson, save } from '../storage';
import {
  downloadQuestion, duck, hearing, language, playing, readingMenus, readingNote, readingNoteBad,
  readingSettings, type EngineName, type Pace, type ReadingSettings,
} from '../store';
import { audioContext, evaluate, strudel, type Hap, type Pattern } from '../strudel/globals';
import { followPace } from '../strudel';
import { followWords, setSpeaking, showSpoken, speaking, type Speaking } from '../watch';
import {
  ENGINE_INFO, ENGINE_NAMES, browserVoices, engines, failures, listBrowserVoices, type Engine, type Progress, type Rendered,
} from './engines';

export { ENGINE_INFO, engines } from './engines';

/** What the reading is doing now, shared with Play and Stop. */
export const speech = {
  // Which cue patterns are current. Every play() starts a new generation; a cue from an older one —
  // a hap Strudel had already scheduled from the previous pattern, or its pending timer — is
  // dropped, so a line of the previous poem or setting is never read over the new music.
  generation: 0,
  generations: 0,   // handed out one per play(), so two overlapping plays never share one
  /** The reading of the pattern now playing, and its cues. */
  playing: null as Reading | null,
  cues: null as Cues | null,
  duckGain: null as GainNode | null,   // the music, before the master stage
  bus: null as GainNode | null,        // the reading's level, into the master stage
};
export interface Cues {
  fitted: Pattern;
  natural: Pattern;
  both: Pattern;
}

const DUCK = 0.6;      // the music under a spoken line, as a fraction of its level
const LEAD = 0.05;     // the least ahead of now a late line is started, in seconds
const SCHEDULER_CPS = 1;   // reading::SCHEDULER_CPS, and the page's repl.setCps(1)

export const LANGUAGES: Record<string, string> = { en: 'English', sl: 'Slovenian' };

const settingsNow = () => readingSettings.value;
const readings = () => piece.readings;

// ---- The reading's own settings, remembered per browser like the voice picker ----

export const READING_DEFAULTS: ReadingSettings = { engine: 'recorded', pace: 'natural', level: 1, voices: {} };
const ENGINE_IDS: EngineName[] = ['browser', 'kokoro', 'piper', 'kitten', 'recorded'];

/** The reading settings saved, read defensively: anything unknown falls back to the default. */
export function readSaved(saved: unknown, oldVoice: string | null): ReadingSettings {
  const out: ReadingSettings = { ...READING_DEFAULTS, voices: {} };
  if (saved && typeof saved === 'object') {
    const s = saved as Record<string, unknown>;
    out.engine = ENGINE_IDS.includes(s.engine as EngineName) ? s.engine as EngineName : READING_DEFAULTS.engine;
    out.pace = s.pace === 'natural' || s.pace === 'fitted' ? s.pace : READING_DEFAULTS.pace;
    out.level = Number(s.level) >= 0 && Number(s.level) <= 2 ? Number(s.level) : 1;
    out.voices = s.voices && typeof s.voices === 'object' ? s.voices as ReadingSettings['voices'] : {};
  }
  // The voice picker before there were engines: a browser voice.
  if (oldVoice && !out.voices.browser) out.voices.browser = { en: oldVoice };
  return out;
}
export function loadReading(): void {
  readingSettings.value = readSaved(loadJson('prosody.reading'), load('prosody.voice'));
}
function saveReading(): void {
  save('prosody.reading', JSON.stringify(readingSettings.value));
}
function update(change: Partial<ReadingSettings>): void {
  readingSettings.value = { ...readingSettings.value, ...change };
}

// ---- Who reads ----

// The voice the setting asks `engine` to read `lang` with: the saved one if it still lists it,
// otherwise its default, otherwise its first.
export function voiceOf(engine: EngineName, lang = language.value): string | null {
  const offered = engines[engine].voices(lang);
  const saved = settingsNow().voices[engine]?.[lang];
  if (offered.some((v) => v.id === saved)) return saved ?? null;
  // The browser's voice has a default of its own, the system's (`null`: no voice set on the
  // utterance, only its language), which is what nothing, or 'browser default', chosen means.
  if (engine === 'browser') return null;
  const fallback = ENGINE_INFO[engine].defaults[lang];
  if (offered.some((v) => v.id === fallback)) return fallback ?? null;
  return offered[0]?.id ?? null;
}

interface Who {
  engine: EngineName;
  why: string;
  bad?: boolean;
  voice?: string | null;
  fallback?: boolean;
}

// Who reads now, and why not the chosen one when it is not: the chosen engine once it has
// loaded and has a voice in the poem's language; the browser's voice meanwhile, and instead.
export function speaker(): Who {
  const chosen = settingsNow().engine;
  const engine = engines[chosen];
  if (chosen === 'browser') return { engine: 'browser', why: '' };
  const name = ENGINE_INFO[chosen].label;
  if (engine.state === 'failed') return { engine: 'browser', why: `${name} could not load (${engine.error}); reading with the browser voice`, bad: true };
  if (engine.state !== 'ready') return { engine: 'browser', why: `${name} ${engine.state === 'loading' ? 'loading' + (engine.progress ? ': ' + engine.progress : '') : 'not loaded'}; reading with the browser voice meanwhile` };
  const lang = language.value;
  if (!voiceOf(chosen)) return { engine: 'browser', why: `${name} has no ${LANGUAGES[lang] || lang} voice; reading with the browser voice${lang === 'sl' ? ' (Piper has one)' : ''}`, bad: true };
  return { engine: chosen, why: '' };
}

// Who reads the line `text`, in which voice, and why not the chosen one when it is not. As
// speaker(), and for the recorded readings line by line: a line recorded in the chosen voice is
// read from its file; one that is not is read by the live voice that made the recordings, if its
// engine has loaded, and otherwise by the browser's voice (`fallback`, with the reason, which the
// caller says with the line's number).
export function speakerOf(text: string): Who {
  const who = speaker();
  if (who.engine !== 'recorded') return { ...who, voice: who.engine === 'browser' ? null : voiceOf(who.engine) };
  const voice = voiceOf('recorded') ?? '';
  if (engines.recorded.has(voice, text)) return { ...who, voice };
  const [live, liveVoice] = voice.split(':') as [EngineName, string];
  const engine = engines[live] as Engine | undefined;
  if (engine?.render && engine.state === 'ready' && engine.voices(language.value).some((v) => v.id === liveVoice)) {
    return { engine: live, voice: liveVoice, why: `no recording of it; read live by ${ENGINE_INFO[live].label} ${liveVoice}`, fallback: true };
  }
  return { engine: 'browser', voice: null, why: 'no recording of it; read by the browser voice', fallback: true };
}

// The speed a line is rendered at by `engine`: a recording's own (the line's pace, at which it was
// recorded) at either pace; a live engine's, the line's pace or its fitted rate.
export function baseSpeed(engine: EngineName, item: { speed: number; pace: number }): number {
  return engine === 'recorded' ? item.pace : item.speed;
}

// Loads the chosen engine if it is not loaded, showing its progress in the status bar. A failure
// is said and the reading carries on with the browser's voice; choosing the engine again (`retry`)
// retries — any other change of setting leaves a failed engine failed. What an engine says while
// it loads, renders or fails is shown only while it is the one chosen, and its progress while it
// renders only for the voice now chosen: a voice or engine let go must not hold the status bar.
let progressShown = false;   // the reading's note is an engine's progress, cleared when it is done
const chosenEngine = (engine: Engine) => settingsNow().engine === engine.name;
export function ensureEngine(retry = false): void {
  const engine = engines[settingsNow().engine];
  if (engine.state === 'ready' || engine.state === 'loading' || !engine.render) return;
  if (engine.state === 'failed' && !retry) return;
  const name = ENGINE_INFO[engine.name].label;
  let shown = '';
  engine.load((progress: Progress) => {
    if (!chosenEngine(engine)) return;
    if (engine.state === 'ready' && renderingJob && (renderingJob.engine !== engine.name || renderingJob.voice !== voiceOf(engine.name))) return;
    // Said in the status bar as it moves, a whole percent at a time; logged only when it is done.
    const said = `${name}: ${progress.text}${progress.fraction != null ? ' (' + Math.round(progress.fraction * 100) + '%)' : ''}`;
    if (said !== shown) { noteReading(shown = said, false, true); progressShown = true; }
    loaderProgress(engine.name, progress);
  }).then(() => {
    console.log(`[reading] ${engine.name} loaded on ${engine.device}`);
    save('prosody.fetched.' + engine.name, '1');
    showVoices();
    if (!chosenEngine(engine)) return;
    noteReading(`${name} ready (${engine.device === 'files' ? 'from files' : engine.device === 'webgpu' ? 'WebGPU' : 'WebAssembly'})`);
    prerender();
  }, (error: Error & { cancelled?: boolean }) => {
    if (error.cancelled) { showVoices(); return; }
    console.warn(`[reading] ${engine.name} failed to load:`, error);
    loaderFailed(engine.name, name, error);
    showVoices();
    if (!chosenEngine(engine)) return;
    noteReading(`${name} could not load: ${error.message} — reading with the browser voice`, true);
  });
}

// An engine that failed after it had loaded (its worker died): said if it is the one chosen, and
// the reading carries on with the browser's voice — a line waiting for it gives up (its render is
// rejected) and the next is read by the browser.
failures.onFailed = (engine, error) => {
  console.warn(`[reading] ${engine.name} failed:`, error);
  showVoices();
  if (!chosenEngine(engine)) return;
  noteReading(`${ENGINE_INFO[engine.name].label} failed: ${error.message} — reading with the browser voice; choose it again to retry`, true);
};

// ---- Rendering ahead ----

interface Entry {
  buffer?: Rendered;
  promise: Promise<Rendered>;
  resolve: (buffer: Rendered) => void;
  reject: (error: Error) => void;
  direct?: boolean;
}
interface Job {
  key: string;
  engine: EngineName;
  voice: string;
  speed: number;
  text: string;
  entry: Entry;
  room: number | null;
}
const renders = new Map<string, Entry>();   // engine|voice|speed|text → { buffer, promise }
const RENDERS_KEPT = 600;
let renderQueue: Job[] = [];
let rendering = false;
let renderingJob: Job | null = null;     // the job the engine is rendering now
// A pending line is in `renders` exactly while its job is queued or rendering: a job dropped from
// the queue (no longer to be read: the poem, engine, voice or pace changed) takes its entry with it
// and rejects its promise as `dropped`, so asking for that line again queues it afresh rather than
// waiting on a render that will never come.
function dropJob(job: Job): void {
  if (renders.get(job.key) === job.entry) renders.delete(job.key);
  const error = Object.assign(new Error('dropped: no longer to be read (the poem, engine, voice or pace changed)'), { dropped: true });
  job.entry.reject(error);
}
export const renderKey = (engine: string, voice: string, speed: number, text: string) =>
  `${engine}|${voice}|${Number(speed).toFixed(3)}|${text}`;

// The buffer for a line, if it has been rendered; and otherwise asks for it, first in the queue
// when `urgent` (a line due now), and gives the promise of it.
function rendered(engine: EngineName, voice: string, speed: number, text: string): Rendered | null {
  return renders.get(renderKey(engine, voice, speed, text))?.buffer ?? null;
}
function newEntry(): Entry {
  const entry = {} as Entry;
  entry.promise = new Promise<Rendered>((resolve, reject) => { entry.resolve = resolve; entry.reject = reject; });
  entry.promise.catch(() => {});
  return entry;
}
function whenRendered(engine: EngineName, voice: string, speed: number, text: string, urgent = false, room: number | null = null): Promise<Rendered> {
  const key = renderKey(engine, voice, speed, text);
  let entry = renders.get(key);
  // A line asked for with its bar's `room` is fitted to it even if it was first asked for without
  // one: a recording has one key at both paces (its own speed), so one fetched at the natural pace
  // would otherwise be read at the fitted pace unfitted, run past its bar and be cut off.
  if (room) fitWhenRendered(key, entry, { key, engine, voice, speed, text, room });
  if (entry?.buffer) return Promise.resolve(entry.buffer);
  if (entry?.direct) return entry.promise;   // a recording being fetched (below)
  // An entry neither queued nor rendering has lost its job (it cannot, by `dropJob`, but a line
  // waiting on it would wait for good): it is asked for afresh.
  if (entry && renderingJob?.key !== key && !renderQueue.some((job) => job.key === key)) {
    renders.delete(key);
    entry = undefined;
  }
  if (!entry) {
    const fresh = newEntry();
    entry = fresh;
    renders.set(key, fresh);
    const job: Job = { key, engine, voice, speed, text, entry: fresh, room };
    // A recording is a fetch, not a computation: it uses no worker, so it is not queued behind a
    // live engine's render (a slow one would hold it up for seconds), and it is never dropped —
    // it takes milliseconds, and its buffer is wanted again whenever the line comes round.
    if (engine === 'recorded') {
      fresh.direct = true;
      engines.recorded.render!(text, { voice, speed }).then((buffer) => {
        fresh.buffer = buffer;
        fresh.resolve(buffer);
        afterRender(job, buffer);
        trimRenders();
        console.log(`[reading] fetched from the recording, ${voice} at ${Number(speed).toFixed(3)}: ${buffer.duration.toFixed(3)} s in ${((buffer.took ?? 0) / 1000).toFixed(2)} s: ${text}`);
      }, (error: Error) => {
        if (renders.get(key) === fresh) renders.delete(key);
        fresh.reject(error);
        console.warn(`[reading] recorded could not fetch "${text}":`, error);
      });
      return fresh.promise;
    }
    if (urgent) renderQueue.unshift(job); else renderQueue.push(job);
  } else if (urgent) {
    const at = renderQueue.findIndex((job) => job.key === key);
    if (at > 0) renderQueue.unshift(...renderQueue.splice(at, 1));
  }
  void pumpRenders();
  return entry.promise;
}
// Oldest first out: insertion order is the order lines were first asked for.
function trimRenders(): void {
  while (renders.size > RENDERS_KEPT) {
    const oldest = renders.keys().next().value;
    if (oldest === undefined) break;
    renders.delete(oldest);
  }
}
async function pumpRenders(): Promise<void> {
  if (rendering) return;
  rendering = true;
  try {
    while (renderQueue.length) {
      const job = renderQueue.shift()!;
      const engine = engines[job.engine];
      if (engine.state !== 'ready' || !engine.render) { renders.delete(job.key); job.entry.reject(new Error('not loaded')); continue; }
      renderingJob = job;
      try {
        const buffer = await engine.render(job.text, { voice: job.voice, speed: job.speed });
        job.entry.buffer = buffer;
        job.entry.resolve(buffer);
        // A voice's download shown while it rendered has done its work.
        if (progressShown) { progressShown = false; noteReading(speaker().why, speaker().bad); }
        afterRender(job, buffer);
        console.log(`[reading] rendered with ${job.engine} ${job.voice} at ${Number(job.speed).toFixed(3)}: ${buffer.duration.toFixed(3)} s in ${((buffer.took ?? 0) / 1000).toFixed(2)} s: ${job.text}`);
      } catch (error) {
        renders.delete(job.key);
        job.entry.reject(error as Error);
        console.warn(`[reading] ${job.engine} could not render "${job.text}":`, error);
      } finally {
        renderingJob = null;
      }
    }
  } finally {
    rendering = false;
  }
  trimRenders();
}

interface ToRead {
  text: string;
  speed: number;
  pace: number;
  room?: number;
  cue?: Cue;
}
// Every line of a reading in reading order: what this pace reads at the speed it reads it — the
// line's pace, or its fitted rate (and the faster one it was refitted to, if it was).
export function linesToRead(reading: Reading | null, pace: Pace = settingsNow().pace): ToRead[] {
  if (!reading) return [];
  return pace === 'fitted'
    ? reading.cues.map((cue) => ({ text: cue.text, speed: cue.rate, pace: cue.pace, room: slotLength(reading, cue.slot) * reading.fill, cue }))
    : reading.lines.map((line) => ({ text: line.text, speed: line.pace, pace: line.pace }));
}
/** `lines` from `at` round to the one before it: the order the lines will be wanted in. */
export function fromWhere<T>(lines: T[], at: number): T[] {
  const from = Math.min(Math.max(0, at), lines.length);
  return lines.slice(from).concat(lines.slice(0, from));
}
// The same, from the line the reading of the playing piece is at round to the one before it: the
// order the lines will be wanted in. Any other reading from its first line.
let fittedAt: { reading: Reading; index: number } | null = null;   // the fitted cue after the one last cued
function upcoming(reading: Reading | null): ToRead[] {
  const lines = linesToRead(reading);
  let at = 0;
  if (settingsNow().pace === 'natural' && chain?.reading === reading) at = chain.next;
  if (settingsNow().pace === 'fitted' && fittedAt?.reading === reading) at = fittedAt.index;
  return fromWhere(lines, at);
}

// ---- Fitting a rendered line to its bar ----
//
// A fitted cue's `rate` is fitted for a voice that says SYLLABLES_PER_SECOND at rate 1 (reading.rs);
// a neural voice has a speed of its own, so a line rendered at that rate can still run past its
// bar and be cut off at the next cue. Such a line is rendered once more, faster by just as much
// as it ran over `fill` of its bar (reading.rs FILL, sent in /reading), no faster than
// REFIT_FASTEST, and that render is the one read once it is ready; until then the first is, and
// a line that runs past its bar is said. Only ever faster: a line that ends early leaves the rest
// of its bar silent, as the browser's voice does.
const REFIT_FASTEST = 2.0;    // past this a voice is gabbling; the line is cut off instead
const refits = new Map<string, number>();     // renderKey at the cue's rate → the speed that fits it to its bar
const speechOf = (buffer: Rendered) => buffer.duration - 2 * (buffer.lead || 0);   // the trim margin each side
/** The speed that fits `seconds` of speech into `room`, if it must be faster than `speed`. */
export function refitSpeed(speed: number, seconds: number, room: number): number | null {
  if (seconds <= room + 0.02) return null;
  const faster = Math.min(REFIT_FASTEST, speed * seconds / room);
  return faster <= speed + 0.0005 ? null : faster;
}
function slotLength(reading: Reading, slot: number): number {
  const bars = barSeconds(reading);
  return (bars[slot + 1] ?? bars[slot] ?? 0) - (bars[slot] ?? 0);
}
function afterRender(job: Omit<Job, 'entry'>, buffer: Rendered): void {
  if (!job.room) return;
  const speed = refitSpeed(job.speed, speechOf(buffer), job.room);
  if (speed === null) return;
  refits.set(job.key, speed);
  console.log(`[reading] fitted: ${speechOf(buffer).toFixed(3)} s of speech for ${job.room.toFixed(3)} s of bar; rendering again at ${speed.toFixed(3)}: ${job.text}`);
  void whenRendered(job.engine, job.voice, speed, job.text, true);
}
// For a line asked for with its bar's room (`job`) that is already rendered, being fetched, or
// queued without a room: fitted once its buffer is there, unless it has been already.
function fitWhenRendered(key: string, entry: Entry | undefined, job: Omit<Job, 'entry'>): void {
  if (!entry || refits.has(key)) return;
  if (entry.buffer) { afterRender(job, entry.buffer); return; }
  if (entry.direct) { entry.promise.then((buffer) => { if (!refits.has(key)) afterRender(job, buffer); }, () => {}); return; }
  const queued = renderingJob?.key === key ? renderingJob : renderQueue.find((queuedJob) => queuedJob.key === key);
  if (queued && !queued.room) queued.room = job.room;
}
// The speed a fitted cue is rendered at for this engine and voice: refitted if it was too long.
function fittedSpeed(engine: EngineName, voice: string, cue: Cue, base = cue.rate): number {
  const refit = refits.get(renderKey(engine, voice, base, cue.text));
  return refit !== undefined && rendered(engine, voice, refit, cue.text) ? refit : base;
}
// Asks for every line the reading may want, in the order it will want them: the playing piece's
// from where its reading is, then the side heard, then the other. The queue then holds exactly
// those, in that order — a queued line nobody will read (of a poem no longer loaded, or another
// engine, voice or pace) is dropped — so a line due next is never stuck behind one of the top of
// the poem, or of the last poem. Called again whenever the reading moves on (advance, fittedCue).
// Each line is asked of whoever reads it (speakerOf): under the recorded readings, a line with no
// recording is rendered ahead by the live voice standing in for it, if that has loaded.
export function prerender(): void {
  const other = hearing.value === 'A' ? 'B' : 'A';
  const wanted: Omit<Job, 'entry'>[] = [];
  const keys = new Set<string>();
  for (const reading of [speech.playing, readings()[hearing.value], readings()[other]]) {
    for (const item of upcoming(reading)) {
      const { text, room, cue } = item;
      if (!text.trim()) continue;
      const { engine, voice } = speakerOf(text);
      if (!voice) continue;
      const speed = baseSpeed(engine, item);
      const refit = cue && refits.get(renderKey(engine, voice, speed, text));
      const speeds: [number, number | null][] = refit !== undefined && refit !== null ? [[speed, room ?? null], [refit, null]] : [[speed, room ?? null]];
      for (const [s, r] of speeds) {
        const key = renderKey(engine, voice, s, text);
        if (keys.has(key)) continue;
        keys.add(key);
        wanted.push({ key, engine, voice, text, speed: s, room: r });
      }
    }
  }
  renderQueue = renderQueue.filter((job) => {
    const kept = keys.has(job.key);
    if (!kept) dropJob(job);
    return kept;
  });
  for (const { engine, voice, text, speed, room } of wanted) void whenRendered(engine, voice, speed, text, false, room);
  const rank = new Map(wanted.map(({ key }, i) => [key, i]));
  renderQueue.sort((a, b) => (rank.get(a.key) ?? 0) - (rank.get(b.key) ?? 0));
}

// ---- Playing a line ----

const sources = new Set<AudioBufferSourceNode>();   // every source scheduled and not yet ended
const readingTimers = new Set<ReturnType<typeof setTimeout>>();
function later(seconds: number, fn: () => void): void {
  const timer = setTimeout(() => { readingTimers.delete(timer); fn(); }, Math.max(0, seconds) * 1000);
  readingTimers.add(timer);
}

// Starts `buffer` so its speech begins at `start` on the audio clock — the buffer itself its quiet
// margin earlier — through the reading's level into the master stage, and shows the line from
// then until it ends. Returns its source, to cut it off.
function playBuffer(buffer: Rendered, start: number, line: SpokenLine, last: boolean): AudioBufferSourceNode {
  const ctx = audioContext();
  const source = new AudioBufferSourceNode(ctx, { buffer });
  source.connect(speech.bus || ctx.destination);
  const at = start - (buffer.lead || 0);
  if (at >= ctx.currentTime) source.start(at); else source.start(ctx.currentTime, ctx.currentTime - at);
  sources.add(source);
  source.onended = () => { sources.delete(source); source.disconnect(); };
  const end = spokenEnd(buffer, start);
  duckSpan(start, end, last ? 0 : line.pause);
  const shown: Speaking = { line, start, duration: end - start };
  later(start - ctx.currentTime, () => { setSpeaking(shown); followWords(shown, () => audioContext().currentTime); });
  later(end - ctx.currentTime, () => { if (speaking === shown) setSpeaking(null); });
  return source;
}

// Where a line whose speech begins at `start` stops sounding.
function spokenEnd(buffer: Rendered, start: number): number {
  return start - (buffer.lead || 0) + buffer.duration;
}

// The music ducked from `start` to `end`, on the audio clock, and kept down through a pause of
// `hold` seconds after it, so the lines of one stanza duck it once rather than pumping: the next
// line's own duck, starting within the hold, cancels the release.
function duckSpan(start: number, end: number, hold: number): void {
  if (!speech.duckGain) return;
  const gain = speech.duckGain.gain;
  const level = duck.value ? DUCK : 1;
  gain.cancelScheduledValues(start);
  gain.setTargetAtTime(level, start, 0.08);
  gain.setTargetAtTime(1, end + hold + 0.05, 0.15);
}

// The browser's voice, at `rate`, from now. `onEnd` is called when it has finished (or failed).
function readLine(line: SpokenLine & { stanzaLast?: boolean }, rate: number, onEnd?: () => void): void {
  if (!('speechSynthesis' in window)) { onEnd?.(); return; }
  // A line still being read when the next is due is cut off, so the reading stays on the bar.
  speechSynthesis.cancel();
  const utterance = new SpeechSynthesisUtterance(line.text);
  utterance.rate = rate;
  utterance.volume = Math.min(1, settingsNow().level);
  const voice = browserVoices.find((v) => v.name === voiceOf('browser'));
  if (voice) { utterance.voice = voice; utterance.lang = voice.lang; } else utterance.lang = language.value;
  // `cancel()` above ends the previous utterance asynchronously; its `end` must not clear the line
  // that has started since, so each handler checks it is still the one speaking.
  const shown: Speaking = { line, utterance };
  setSpeaking(shown, false);   // shown once it starts
  utterance.onstart = () => { readingDuck(true); if (speaking === shown) setSpeaking(shown); };
  utterance.onboundary = (event) => {
    if (speaking === shown && event.name !== 'sentence') showSpoken(event);
  };
  let ended = false;
  const done = () => {
    if (ended) return;
    ended = true;
    readingDuck(false, line.stanzaLast ? 0 : line.pause);
    if (speaking === shown) setSpeaking(null);
    onEnd?.();
  };
  utterance.onend = done;
  utterance.onerror = done;
  speechSynthesis.speak(utterance);
}

function readingDuck(down: boolean, hold = 0): void {
  if (!speech.duckGain) return;
  const now = audioContext().currentTime;
  if (down) duckSpan(now, now + 3600, 0);
  else { speech.duckGain.gain.cancelScheduledValues(now); speech.duckGain.gain.setTargetAtTime(1, now + hold, 0.15); }
}

/** Everything scheduled is let go: the timers, the sources, the browser's voice, the duck. */
export function quietReading(): void {
  for (const timer of readingTimers) clearTimeout(timer);
  readingTimers.clear();
  for (const source of sources) { try { source.stop(); } catch { /* already stopped */ } }
  sources.clear();
  if ('speechSynthesis' in window) speechSynthesis.cancel();
  chain = null;
  fittedLast = null;
  fittedAt = null;
  if (speech.duckGain) {
    const now = audioContext().currentTime;
    speech.duckGain.gain.cancelScheduledValues(now);
    speech.duckGain.gain.setTargetAtTime(1, now, 0.08);
  }
  setSpeaking(null);
}

// ---- The fitted pace: a line a bar ----

let fittedLast: AudioBufferSourceNode | null = null;   // the line last started, to cut off at the next cue
function fittedCue(cue: Cue, time: number, generation: number): void {
  if (generation !== speech.generation || settingsNow().pace !== 'fitted') return;
  const ctx = audioContext();
  const who = speakerOf(cue.text);
  const voice = who.voice;
  const base = baseSpeed(who.engine, { speed: cue.rate, pace: cue.pace });
  const speed = voice ? fittedSpeed(who.engine, voice, cue, base) : cue.rate;
  const buffer = voice ? rendered(who.engine, voice, speed, cue.text) : null;
  if (who.fallback) noteReading(`line ${cue.line + 1}: ${who.why}`);
  if (speech.playing) {
    fittedAt = { reading: speech.playing, index: speech.playing.cues.indexOf(cue) + 1 };
    prerender();
  }
  console.log(`[reading] fitted: bar ${cue.slot + 1} line ${cue.line + 1} at ${time.toFixed(3)} s`
    + ` (bytes ${cue.start}..${cue.end}), rate ${cue.rate}${speed !== base ? ', refitted to ' + speed.toFixed(3) : ''}, ${buffer ? who.engine + ' ' + voice : 'browser'}: ${cue.text}`);
  if (buffer) {
    // A line still sounding when the next is due is cut off at the bar, as the browser's is.
    try { fittedLast?.stop(time); } catch { /* already stopped */ }
    fittedLast = playBuffer(buffer, time, cue, true);
    const bar = speech.playing ? slotLength(speech.playing, cue.slot) : Infinity;
    const over = speechOf(buffer) - bar;
    if (over > 0.02) noteReading(`line ${cue.line + 1} runs ${over.toFixed(2)} s past its bar and may be cut off at the next cue`);
    return;
  }
  if (voice) {
    void whenRendered(who.engine, voice, base, cue.text, true, speech.playing ? slotLength(speech.playing, cue.slot) * speech.playing.fill : null);
    noteReading(`line ${cue.line + 1} not rendered in time; read by the browser voice`);
  } else if (who.why && !who.fallback) noteReading(who.why, who.bad);
  later(time - ctx.currentTime, () => { if (generation === speech.generation) readLine(cue, cue.rate); });
}

// ---- The natural pace: a line after the last ----
//
// SPEC.md §4.7. A line is read at the voice's own speed times its pace, and the next begins when
// it has finished plus its pause; a stanza begins where its cue says the music has reached it —
// and, under `breath`, not before a whole silent bar has passed since the previous stanza's last
// line ended, nor before that line's stanza-end pause: on the first bar that satisfies all three.
// All of it is timed on the audio clock from the cue's `targetTime`. A rendered line's end is known
// the moment it is scheduled, so a stanza is scheduled line after line ahead of time; the
// browser's voice reports its end only when it gets there, so its lines are chained on `onend`.
export interface Chain {
  reading: Reading;
  generation: number;
  seconds: number[];
  next: number;            // the index into reading.lines of the line to read next
  round: number | null;    // which pass of the looping piece the anchors below belong to
  wrapTo: number | null;   // the line the new round starts at, when it came round mid-line
  origin: number;          // the audio time this round's bar 1 began
  anchors: Map<number, number>;  // index into reading.lines → the audio time its cue fired for, this round
  prevEnd: number | null;
  prevPause: number;
  busy: boolean;
}
let chain: Chain | null = null;
function newChain(reading: Reading, generation: number): Chain {
  return {
    reading, generation,
    seconds: barSeconds(reading),
    next: 0, round: null, wrapTo: null, origin: 0,
    anchors: new Map(),
    prevEnd: null, prevPause: 0,
    busy: false,
  };
}
// The music's bars, in seconds from the piece's start, and its length last.
export function barSeconds(reading: Pick<Reading, 'bars' | 'cpm'>): number[] {
  return reading.bars.map((cycles) => cycles * 60 / reading.cpm / SCHEDULER_CPS);
}
// Bar `k` of the looping piece (negative or past the end: the rounds before and after) on the
// audio clock, and the bar playing at audio time `t`.
export function barTime(c: Pick<Chain, 'seconds' | 'origin'>, k: number): number {
  const bars = c.seconds, slots = bars.length - 1, length = bars[slots] ?? 0;
  const round = Math.floor(k / slots);
  return c.origin + round * length + (bars[k - round * slots] ?? 0);
}
export function barAt(c: Pick<Chain, 'seconds' | 'origin'>, t: number): number {
  const bars = c.seconds, slots = bars.length - 1, length = bars[slots] ?? 0;
  const round = Math.floor((t - c.origin) / length);
  const within = t - c.origin - round * length;
  let k = 0;
  while (k + 1 < slots && (bars[k + 1] ?? Infinity) <= within + 1e-9) k += 1;
  return k + round * slots;
}

function naturalCue(index: number, hap: Hap, time: number, cps: number, generation: number): void {
  if (generation !== speech.generation || settingsNow().pace !== 'natural') return;
  const reading = speech.playing;
  if (!reading) return;
  if (!chain || chain.generation !== generation) chain = newChain(reading, generation);
  const line = reading.lines[index];
  if (!line || !hap.whole) return;
  const length = chain.seconds[chain.seconds.length - 1] ?? 0;
  const at = hap.whole.begin.valueOf() / cps / SCHEDULER_CPS;   // seconds since the scheduler's cycle 0
  const slotAt = chain.seconds[line.slot] ?? 0;
  const round = Math.round((at - slotAt) / length);
  chain.origin = time - slotAt;
  if (round !== chain.round) {
    // The piece has come round: this round's cues begin afresh, from this one. While a line is
    // still being read (the browser's voice until its `onend`, a neural line until it is rendered)
    // the move waits for it to finish (`wrapTo`, taken in advance()), or its `after()` would step
    // past the last line and leave this round unread. The last line's end is kept, so a first
    // stanza still waits for its breath after a last one that ran over the loop's seam.
    if (chain.round !== null && !chain.busy) wrapRound(chain, index);
    else if (chain.round !== null) chain.wrapTo = index;
    else chain.next = index;
    chain.round = round;
    chain.anchors.clear();
  }
  chain.anchors.set(index, time);
  // Nothing being read, and the line waited for is not this one: either the clock was joined
  // part-way through (a live edit) or the reading had finished this round. Start here.
  if (!chain.busy && chain.next !== index && !chain.anchors.has(chain.next)) chain.next = index;
  console.log(`[reading] natural: cue for line ${line.line + 1} (${line.onset}) in bar ${line.slot + 1} at ${time.toFixed(3)} s`);
  advance();
}

// The chain moves to the new round's line `index`. A line of the old round not yet read will not
// be — its round's cues are gone — and is said, not dropped in silence (CLAUDE.md §6).
function wrapRound(c: Chain, index: number): void {
  const lines = c.reading.lines;
  if (c.next < lines.length) {
    const lost = lines.slice(c.next).map((l) => l.line + 1);
    noteLoss(`not read last time round: line ${lost.join(', ')}, the piece came round before its turn`);
  }
  c.next = index;
  c.wrapTo = null;
}

// Reads on, then puts the lines it will want next at the head of the render queue.
function advance(): void {
  advanceChain();
  if (chain && chain.generation === speech.generation) prerender();
}
function advanceChain(): void {
  const c = chain;
  if (!c || c.busy || c.generation !== speech.generation) return;
  if (c.wrapTo !== null) wrapRound(c, c.wrapTo);
  const lines = c.reading.lines;
  while (c.next < lines.length) {
    const line = lines[c.next]!;
    let earliest = -Infinity;
    if (line.onset !== 'follows') {
      const anchor = c.anchors.get(c.next);
      if (anchor === undefined) {
        // Waiting for its bar. If a later line's cue has already fired this round, this one's
        // will not (the clock was joined after it): move on to that one.
        const after = [...c.anchors.keys()].filter((k) => k > c.next).sort((a, b) => a - b)[0];
        if (after === undefined) return;
        c.next = after;
        continue;
      }
      earliest = anchor;
    } else if (c.prevEnd === null) {
      // Joined in the middle of a stanza: there is no line before to follow, so this one waits for
      // the piece to come round, as a line before the clock's bar does at the fitted pace.
      c.next += 1;
      continue;
    }
    if (c.prevEnd !== null) {
      earliest = Math.max(earliest, c.prevEnd + c.prevPause);
      if (line.onset === 'breath') {
        // On a bar, and at least two bars after the one the last line ended in, so one whole bar
        // between is silent: the first bar from there that begins no earlier than `earliest`.
        let bar = Math.max(barAt(c, c.prevEnd) + 2, barAt(c, earliest));
        if (barTime(c, bar) < earliest - 1e-6) bar += 1;
        earliest = barTime(c, bar);
      }
    }
    const pieceEnd = c.origin + (c.seconds[c.seconds.length - 1] ?? 0);
    if (earliest >= pieceEnd - 1e-6) {
      const lost = lines.slice(c.next).filter((l) => l.stanza >= line.stanza).map((l) => l.line + 1);
      noteReading(`not read this time round: line ${lost.join(', ')}, the piece ends before its turn`);
      console.log(`[reading] natural: the piece ends before line ${line.line + 1}`);
      c.next = lines.length;
      return;
    }
    speakNatural(c, line, earliest);
    if (c.busy) return;
  }
}

function speakNatural(c: Chain, line: NaturalLine, earliest: number): void {
  const ctx = audioContext();
  const who = speakerOf(line.text);
  const nextLine = c.reading.lines[c.next + 1];
  const last = !nextLine || nextLine.stanza !== line.stanza;
  const after = () => { c.prevPause = line.pause; c.next += 1; };
  if (who.fallback) noteReading(`line ${line.line + 1}: ${who.why}`);
  if (who.engine === 'browser') {
    if (who.why && !who.fallback) noteReading(who.why, who.bad);
    c.busy = true;
    later(earliest - ctx.currentTime, () => {
      if (chain !== c) return;
      console.log(`[reading] natural: line ${line.line + 1} with the browser voice at ${ctx.currentTime.toFixed(3)} s (due ${earliest.toFixed(3)}), pace ${line.pace}: ${line.text}`);
      readLine({ ...line, stanzaLast: last }, line.pace, () => {
        if (chain !== c) return;
        c.prevEnd = audioContext().currentTime;
        after();
        c.busy = false;
        advance();
      });
    });
    return;
  }
  const voice = who.voice ?? '';
  const buffer = rendered(who.engine, voice, line.pace, line.text);
  if (!buffer) {
    c.busy = true;
    noteReading(`waiting for ${ENGINE_INFO[who.engine].label} to ${who.engine === 'recorded' ? 'fetch' : 'render'} line ${line.line + 1}`);
    whenRendered(who.engine, voice, line.pace, line.text, true).then(() => {
      if (chain !== c) return;
      c.busy = false;
      advance();
    }, (error: Error & { dropped?: boolean }) => {
      if (chain !== c) return;
      // Dropped for a change of engine or voice: the line is asked for again by whoever reads now.
      if (error.dropped) { c.busy = false; advance(); return; }
      noteReading(`${ENGINE_INFO[who.engine].label} could not render line ${line.line + 1}: ${error.message}`, true);
      after();
      c.busy = false;
      advance();
    });
    return;
  }
  const start = Math.max(earliest, ctx.currentTime + LEAD);
  const late = start - earliest;
  console.log(`[reading] natural: line ${line.line + 1} with ${who.engine} ${voice} at ${start.toFixed(3)} s`
    + (late > 0.001 ? ` (${late.toFixed(3)} s late)` : '') + `, ${buffer.duration.toFixed(3)} s, pace ${line.pace}, then ${line.pause} s: ${line.text}`);
  playBuffer(buffer, start, line, last);
  c.prevEnd = spokenEnd(buffer, start);
  if (readingNote.value.startsWith('waiting for')) noteReading('');
  after();
}

/** Both cue patterns of a reading, their callbacks attached, stacked to play with the music. */
export function cuesOf(reading: Reading, generation: number): Cues {
  const byLine = new Map(reading.cues.map((cue) => [cue.line, cue]));
  const fitted = evaluate(reading.pattern).onTrigger((hap, _currentTime, _cps, targetTime) => {
    const cue = byLine.get(Number(hap.value?.n));
    if (cue) fittedCue(cue, targetTime, generation);
  }, true);
  const indexOf = new Map(reading.lines.map((line, index) => [line.line, index]));
  const natural = evaluate(reading.natural).onTrigger((hap, _currentTime, cps, targetTime) => {
    const index = indexOf.get(Number(hap.value?.n));
    if (index !== undefined) naturalCue(index, hap, targetTime, cps, generation);
  }, true);
  return { fitted, natural, both: strudel.stack(fitted, natural) };
}

// ---- What the reading says in the status bar ----

let lossNote = '';
let lossUntil = 0;
export function noteReading(text: string, bad = false, quiet = false): void {
  if (text && !quiet) console.log('[reading] ' + text);
  // A line lost at the seam is reported once, in the same pass that goes on to read the next round —
  // which may at once say why the browser voice is standing in, or that a line is still rendering.
  // So a loss stays beside whatever note follows it for a few seconds rather than being replaced
  // before anyone could read it (CLAUDE.md §6: a loss must be visible).
  const loss = performance.now() < lossUntil ? lossNote : '';
  readingNote.value = loss && text !== loss ? (text ? `${text} · ${loss}` : loss) : text;
  readingNoteBad.value = bad;
}

function noteLoss(text: string): void {
  lossNote = text;
  lossUntil = performance.now() + 8000;
  noteReading(text);
}

// ---- The reading panel's menus ----

/** The reading panel's menus, from the settings and what each engine offers. */
export function showVoices(): void {
  const settings = settingsNow();
  const known: Record<string, string> = { ...LANGUAGES };
  for (const v of engines.piper.listed || []) {
    const code = v.lang.split('-')[0] ?? v.lang;
    known[code] ??= code;
  }
  const engine = settings.engine;
  const offered = engines[engine].voices(language.value);
  readingMenus.value = {
    engines: ENGINE_NAMES.map((name) => {
      const state = engines[name].state === 'failed' ? ' — could not load' : engines[name].state === 'loading' ? ' — loading…' : '';
      return { value: name, label: ENGINE_INFO[name].label + state };
    }),
    languages: Object.entries(known).map(([value, label]) => ({ value, label })),
    voices: (engine === 'browser' ? [{ value: '', label: 'browser default' }] : [])
      .concat(offered.map((v) => ({ value: v.id, label: v.name }))),
    voice: voiceOf(engine) ?? '',
    voiceDisabled: !offered.length && engine !== 'browser',
  };
}

// A change of who reads, how fast, or in what language, while it plays: what is scheduled is
// dropped and the reading picks up at the next cue, in the new voice.
export function readingChanged(retry = false): void {
  saveReading();
  showVoices();
  ensureEngine(retry);
  prerender();
  if (playing.value) {
    quietReading();
    showPattern();
    followPace();
  }
  const who = speaker();
  noteReading(who.why, who.bad);
}

export function setPace(pace: Pace): void {
  update({ pace });
  readingChanged();
}
export function setLanguage(code: string): void {
  language.value = code;
  readingChanged();
}
export function setVoice(voice: string): void {
  const engine = settingsNow().engine;
  update({ voices: { ...settingsNow().voices, [engine]: { ...(settingsNow().voices[engine] || {}), [language.value]: voice } } });
  readingChanged();
}
export function setLevel(level: number): void {
  update({ level });
  if (speech.bus) speech.bus.gain.setTargetAtTime(level, audioContext().currentTime, 0.02);
  saveReading();
}

/** A preset in another language: the old poem's reading is let go first — stopped, and its lines
 * out of the render queue — so its lines are never rendered in the new poem's voice, nor the new
 * poem's in the old one's: until the new poem has compiled there is nothing to read. */
export function readIn(code: string): void {
  if (code === language.value) return;
  speech.generation = ++speech.generations;   // nothing the old poem cues is read any more
  quietReading();
  speech.playing = null;
  speech.cues = null;
  piece.readings = { A: null, B: null, poem: '' };
  language.value = code;
  readingChanged();
}

// ---- Choosing a voice that has to be downloaded ----
//
// A neural voice is a download of tens or hundreds of MB, so the first time one is chosen the page
// says what it costs and asks; until it is ready the browser's own voice reads, and the music never
// waits. The sizes are what each engine fetched when measured here, rounded and said as "about".
export const DOWNLOADS: Partial<Record<EngineName, () => string>> = {
  // Kokoro fetches its full model where it runs on WebGPU, its compact one on WebAssembly — which
  // `?device=wasm` forces, as the worker does (speech-worker.mjs).
  kokoro: () => ('gpu' in navigator && new URLSearchParams(location.search).get('device') !== 'wasm'
    ? 'about 330 MB (the full model, since this browser has WebGPU)'
    : 'about 90 MB (the compact model)'),
  piper: () => 'about 60 MB for each voice, fetched when the voice first reads',
  kitten: () => 'about 25 MB',
};
let previousEngine: EngineName = 'recorded';
export function chooseEngine(name: EngineName): void {
  const engine = engines[name];
  const asked = !!load('prosody.fetched.' + name);
  const size = DOWNLOADS[name];
  if (size && engine.state !== 'ready' && engine.state !== 'loading' && !asked) {
    const label = ENGINE_INFO[name].label;
    downloadQuestion.value = {
      title: `${label} needs a one-time download of ${size()}.`,
      text: 'It runs entirely in your browser and is kept after this, so it won\'t download again. '
        + 'Until it\'s ready, the poem is read by your browser\'s own voice, and the music plays on.',
      yes: `Download ${label}`,
      answer: (yes) => {
        downloadQuestion.value = null;
        if (yes) useEngine(name, true);
        else showVoices();   // the menu goes back to the engine there was
      },
    };
    return;
  }
  useEngine(name, false);
}
function useEngine(name: EngineName, show: boolean): void {
  const before = previousEngine;   // what Cancel goes back to
  previousEngine = name;
  update({ engine: name });
  readingChanged(true);   // starts the download (ensureEngine)
  const engine = engines[name];
  if (!show || !engine.render) return;
  openLoader({
    engine: name,
    back: before === name ? 'recorded' : before,
    title: `Downloading ${ENGINE_INFO[name].label}`,
    text: `${DOWNLOADS[name]?.() ?? 'A download'}, once. Your browser's voice reads meanwhile; Hide lets it carry on in the background.`,
  });
  void warmVoice(name);
}

// A voice's model may come with its first line (Piper fetches each voice then), so the loader
// stays until the voice has read something: the first line of the poem in front of the reader,
// rendered now — which also means that line is ready before Play.
async function warmVoice(name: EngineName): Promise<void> {
  const engine = engines[name];
  try {
    if (engine.loading) await engine.loading;
    if (engine.state !== 'ready') return;
    const line = (piece.readings[hearing.value]?.lines || []).find((l) => l.text.trim());
    const voice = voiceOf(name);
    if (line && engine.render && voice) {
      loaderSays(name, 'preparing the voice…');
      await whenRendered(name, voice, line.pace, line.text, true);
    }
    if (loaderEngine() === name) closeLoader();
  } catch (error) {
    const failed = error as Error & { cancelled?: boolean; dropped?: boolean };
    if (!failed.cancelled && !failed.dropped) loaderFailed(name, ENGINE_INFO[name].label, failed);
  }
}

/** The loader's Hide: it closes, and whatever was loading carries on. */
export const hideLoader = closeLoader;
/** The loader's Cancel: the voice's download stops, and the voice there was reads. */
export function cancelLoader(): void {
  const name = loaderEngine();
  const back = loaderBack() || 'recorded';
  closeLoader();
  if (!name) return;
  engines[name].cancel?.();
  useEngine(back, false);
  noteReading(`${ENGINE_INFO[name].label} download cancelled; reading with ${ENGINE_INFO[back].label}`);
}

/** At start-up: the settings saved, the browser's voices, and the menus. */
export function startReading(): void {
  loadReading();
  previousEngine = readingSettings.value.engine;
  // The recorded engine counts what each voice has of the poem heard.
  engines.recorded.heard = () => piece.readings[hearing.value];
  engines.recorded.saved = (lang) => readingSettings.value.voices.recorded?.[lang];
  listBrowserVoices();
  if ('speechSynthesis' in window) {
    speechSynthesis.addEventListener('voiceschanged', () => { listBrowserVoices(); showVoices(); });
  }
  showVoices();
  // A returning reader whose engine is the recorded readings: the manifest is fetched now — a few
  // tens of kB, no model — so the voice saved is listed and the first line of the first play is
  // read from its file, not by the browser while the manifest is still on its way. A live engine
  // still loads only when chosen or played, its model being hundreds of MB.
  if (readingSettings.value.engine === 'recorded') ensureEngine();
}
