// ---- The reading ----
//
// Who reads the poem over the music, in which voice, and the lines rendered ahead of their turn. The
// reading is *timed* by the Prosody player (`docs/player.md` §5, "The reading"): its render worker
// knows each line's cue from the Performance and places a rendered line on the speech stem at its
// frame, with the duck, as it renders the music around it (crates/prosody-player). This module decides
// who reads each line (`castOf`) and renders the lines ahead: a neural engine renders every line
// after each compile, and keeps what it made by engine, voice, speed and text, so a recompile reuses
// every line it did not change. A line the recorded readings have no file of — an edited line, a
// poem of the writer's own — is read by a live engine instead, and said (speakerOf). The browser's
// own voice cannot be rendered ahead: the player says when its line is due and this module speaks
// it then (`readAloud`), off the playhead.
import type { Cue, Reading, SpokenLine } from '../answers';
import { closeLoader, loaderBack, loaderEngine, loaderFailed, loaderProgress, loaderSays, openLoader } from '../loader';
import { piece, showPattern } from '../piece';
import { load, loadJson, save } from '../storage';
import {
  downloadQuestion, hearing, language, playing, readingMenus, readingNote, readingNoteBad,
  readingSettings, type EngineName, type Pace, type ReadingSettings,
} from '../store';
import { audible } from '../mix';
import { setSpeaking, showSpoken, speaking, type Speaking } from '../watch';
import {
  ENGINE_INFO, NEURAL_ENGINES, NEURAL_VOICES, OFFERED_ENGINES, SYSTEM_VOICE, browserVoices, engines, failures, listBrowserVoices, type Engine, type Progress, type Rendered,
} from './engines';

export type { Rendered } from './engines';

export { ENGINE_INFO, engines } from './engines';

/** What the reading is doing now, shared with the player. */
export const speech = {
  /** The reading of the piece now playing. */
  playing: null as Reading | null,
};

/** The player, as the reading needs it. The player registers itself (`hostReading`) when it loads,
 * so this module never imports it. */
export interface ReadingHost {
  /** Who reads each line may have changed: a line was rendered, an engine loaded or failed, a
   * setting changed. With `quiet` (a change of voice, engine, pace or language), what is placed is
   * let go and the reading picks up at the next cue in the new voice. */
  recast(quiet: boolean): void;
  /** A line rendered or fetched, by its render key: the player sends it on if it wants it. */
  rendered(key: string, buffer: Rendered): void;
  /** A line the player waits for could not be rendered. */
  failed(key: string): void;
  /** The music lowered (or raised again) under the browser's voice, which the player cannot place. */
  duck(down: boolean): void;
  /** The reading's level changed (its fader, or mute and solo). */
  level(): void;
}
let host: ReadingHost | null = null;
export function hostReading(player: ReadingHost | null): void {
  host = player;
}

export const LANGUAGES: Record<string, string> = { en: 'English', sl: 'Slovenian' };

const settingsNow = () => readingSettings.value;
const readings = () => piece.readings;

// ---- The reading's own settings, remembered per browser like the voice picker ----

export const READING_DEFAULTS: ReadingSettings = { engine: 'recorded', pace: 'natural', level: 1, voices: {} };

/** The reading settings saved, read defensively: anything unknown falls back to the default, and so
 * does an engine this build does not offer (a neural voice saved before, in a build without them). */
export function readSaved(saved: unknown, oldVoice: string | null, offered: readonly EngineName[] = OFFERED_ENGINES): ReadingSettings {
  const out: ReadingSettings = { ...READING_DEFAULTS, voices: {} };
  if (saved && typeof saved === 'object') {
    const s = saved as Record<string, unknown>;
    out.engine = offered.includes(s.engine as EngineName) ? s.engine as EngineName : READING_DEFAULTS.engine;
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

// Who stands in for a voice that cannot read: the browser's; in the Android app, whose WebView has
// none, the phone's own once it has started and has a voice in the poem's language — and the
// browser's (which reads nothing there) until then.
export function standIn(): EngineName {
  return SYSTEM_VOICE && engines.system.state === 'ready' && voiceOf('system') ? 'system' : 'browser';
}

// Who reads now, and why not the chosen one when it is not: the chosen engine once it has
// loaded and has a voice in the poem's language; the stand-in meanwhile, and instead.
export function speaker(): Who {
  const chosen = settingsNow().engine;
  const engine = engines[chosen];
  if (chosen === 'browser') return { engine: 'browser', why: '' };
  const back = chosen === 'system' ? 'browser' : standIn();
  const by = `reading with the ${ENGINE_INFO[back].label}`;
  const name = ENGINE_INFO[chosen].label;
  if (engine.state === 'failed') return { engine: back, why: `${name} could not load (${engine.error}); ${by}`, bad: true };
  if (engine.state !== 'ready') return { engine: back, why: `${name} ${engine.state === 'loading' ? 'loading' + (engine.progress ? ': ' + engine.progress : '') : 'not loaded'}; ${by} meanwhile` };
  const lang = language.value;
  if (!voiceOf(chosen)) return { engine: back, why: `${name} has no ${LANGUAGES[lang] || lang} voice; ${by}${lang === 'sl' && NEURAL_VOICES ? ' (Piper has one)' : ''}`, bad: true };
  return { engine: chosen, why: '' };
}

// Who reads the line `text`, in which voice, and why not the chosen one when it is not. As
// speaker(), and for the recorded readings line by line: a line recorded in the chosen voice is
// read from its file; one that is not is read by the live voice that made the recordings, if its
// engine has loaded, and otherwise by the stand-in (`fallback`, with the reason, which the
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
  const back = standIn();
  return { engine: back, voice: back === 'browser' ? null : voiceOf(back), why: `no recording of it; read by the ${ENGINE_INFO[back].label}`, fallback: true };
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
  startSystem();
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
    host?.recast(false);
    if (!chosenEngine(engine)) return;
    noteReading(`${name} ready (${engine.device === 'files' ? 'from files' : engine.device === 'webgpu' ? 'WebGPU' : 'WebAssembly'})`);
    prerender();
  }, (error: Error & { cancelled?: boolean }) => {
    if (error.cancelled) { showVoices(); return; }
    console.warn(`[reading] ${engine.name} failed to load:`, error);
    loaderFailed(engine.name, name, error);
    showVoices();
    host?.recast(false);
    if (!chosenEngine(engine)) return;
    noteReading(`${name} could not load: ${error.message} — reading with the browser voice`, true);
  });
}

// The phone's own voice, in the Android app: started with the page and on every Play (it is let go
// while the page is hidden and silent), as it costs nothing to download and stands in for any
// voice that cannot read. A failure is logged; the reading carries on without it.
function startSystem(): void {
  const engine = engines.system;
  if (!SYSTEM_VOICE || engine.state === 'ready' || engine.state === 'loading') return;
  engine.load().then(() => {
    console.log(`[reading] system voice: ${engine.device}, ${engine.listed?.length ?? 0} voices on the phone`);
    showVoices();
    host?.recast(false);
    prerender();
  }, (error: Error) => {
    console.warn('[reading] the system voice did not start:', error);
    showVoices();
  });
}

// An engine that failed after it had loaded (its worker died): said if it is the one chosen, and
// the reading carries on with the browser's voice — a line waiting for it gives up (its render is
// rejected) and the next is read by the browser.
failures.onFailed = (engine, error) => {
  console.warn(`[reading] ${engine.name} failed:`, error);
  showVoices();
  host?.recast(false);
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
        arrived(key, buffer);
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
/** A line is ready: the player hears of it, and of who reads now (a refit may move a fitted line onto
 * its faster rendering). */
function arrived(key: string, buffer: Rendered): void {
  host?.rendered(key, buffer);
  host?.recast(false);
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
        arrived(job.key, buffer);
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
  line: number;
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
    ? reading.cues.map((cue) => ({ line: cue.line, text: cue.text, speed: cue.rate, pace: cue.pace, room: slotLength(reading, cue.slot) * reading.fill, cue }))
    : reading.lines.map((line) => ({ line: line.line, text: line.text, speed: line.pace, pace: line.pace }));
}
/** `lines` from `at` round to the one before it: the order the lines will be wanted in. */
export function fromWhere<T>(lines: T[], at: number): T[] {
  const from = Math.min(Math.max(0, at), lines.length);
  return lines.slice(from).concat(lines.slice(0, from));
}
// The same, from the line the reading of the playing piece is at round to the one before it: the
// order the lines will be wanted in. Any other reading from its first line.
let readingAt: { reading: Reading; pace: Pace; line: number | null } | null = null;
function upcoming(reading: Reading | null): ToRead[] {
  const lines = linesToRead(reading);
  let at = 0;
  if (readingAt && readingAt.reading === reading && readingAt.pace === settingsNow().pace) {
    const { line } = readingAt;
    at = line === null ? lines.length : Math.max(0, lines.findIndex((item) => item.line === line));
  }
  return fromWhere(lines, at);
}
/** Where the player says the reading of `reading` is: the line it will want next (null once the
 * round's are read). The render queue is put in the order it will want them from there. */
export function readingIsAt(reading: Reading, pace: Pace, line: number | null): void {
  if (readingAt?.reading === reading && readingAt.pace === pace && readingAt.line === line) return;
  readingAt = { reading, pace, line };
  prerender();
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
// the poem, or of the last poem. Called again whenever the reading moves on (readingIsAt).
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

// ---- Who reads each line, for the player ----

/** Who reads each line of `reading` at `pace`, by line number: the render key of the rendering the
 * player places (the refitted one, at the fitted pace, once it is ready), or null for the browser's
 * own voice. The player places a line from this (crates/prosody-player's reader). */
export function castOf(reading: Reading | null, pace: Pace = settingsNow().pace): Map<number, string | null> {
  const cast = new Map<number, string | null>();
  if (!reading) return cast;
  if (pace === 'fitted') {
    for (const cue of reading.cues) {
      const who = speakerOf(cue.text);
      if (who.engine === 'browser' || !who.voice) { cast.set(cue.line, null); continue; }
      const base = baseSpeed(who.engine, { speed: cue.rate, pace: cue.pace });
      cast.set(cue.line, renderKey(who.engine, who.voice, fittedSpeed(who.engine, who.voice, cue, base), cue.text));
    }
  } else {
    for (const line of reading.lines) {
      const who = speakerOf(line.text);
      cast.set(line.line, who.engine === 'browser' || !who.voice ? null : renderKey(who.engine, who.voice, line.pace, line.text));
    }
  }
  return cast;
}

/** The rendering a render key names, if it is ready. */
export function renderedFor(key: string): Rendered | null {
  return renders.get(key)?.buffer ?? null;
}

/** A line the player found unrendered when it was due: asked for first in the queue, fitted to its
 * bar at the fitted pace. */
export function renderUrgently(reading: Reading | null, line: SpokenLine & { rate?: number; slot?: number }, pace: Pace): void {
  const who = speakerOf(line.text);
  if (who.engine === 'browser' || !who.voice) return;
  const fitted = pace === 'fitted' && line.rate !== undefined;
  const speed = fitted ? baseSpeed(who.engine, { speed: line.rate!, pace: line.pace }) : line.pace;
  const room = fitted && reading ? slotLength(reading, line.slot ?? 0) * reading.fill : null;
  whenRendered(who.engine, who.voice, speed, line.text, true, room).catch((error: Error & { dropped?: boolean }) => {
    if (error.dropped) return;
    noteReading(`${ENGINE_INFO[who.engine].label} could not render line ${line.line + 1}: ${error.message}`, true);
    host?.failed(renderKey(who.engine, who.voice!, speed, line.text));
  });
}

// ---- The browser's own voice ----
//
// It cannot be rendered ahead or placed in a stream: the player's render worker says when its line
// is due, and the page speaks it when the playhead gets there — off the playhead, not in step with
// the music (docs/player.md §5).

const readingTimers = new Set<ReturnType<typeof setTimeout>>();
function later(seconds: number, fn: () => void): void {
  const timer = setTimeout(() => { readingTimers.delete(timer); fn(); }, Math.max(0, seconds) * 1000);
  readingTimers.add(timer);
}

/** Reads `line` in the browser's voice, at `rate`, from now. `onEnd` is called when it has finished
 * (or failed). */
export function readAloud(line: SpokenLine & { stanzaLast?: boolean }, rate: number, onEnd?: () => void): void {
  if (!('speechSynthesis' in window)) { onEnd?.(); return; }
  // A line still being read when the next is due is cut off, so the reading stays on the bar.
  speechSynthesis.cancel();
  const utterance = new SpeechSynthesisUtterance(line.text);
  utterance.rate = rate;
  utterance.volume = Math.min(1, voiceLevel());
  const voice = browserVoices.find((v) => v.name === voiceOf('browser'));
  if (voice) { utterance.voice = voice; utterance.lang = voice.lang; } else utterance.lang = language.value;
  // `cancel()` above ends the previous utterance asynchronously; its `end` must not clear the line
  // that has started since, so each handler checks it is still the one speaking.
  const shown: Speaking = { line, utterance };
  setSpeaking(shown, false);   // shown once it starts
  utterance.onstart = () => { host?.duck(true); if (speaking === shown) setSpeaking(shown); };
  utterance.onboundary = (event) => {
    if (speaking === shown && event.name !== 'sentence') showSpoken(event);
  };
  let ended = false;
  const done = () => {
    if (ended) return;
    ended = true;
    // The lines of a stanza duck the music once: it rises only after the pause before the next.
    later(line.stanzaLast ? 0 : line.pause, () => { if (!speaking?.utterance) host?.duck(false); });
    if (speaking === shown) setSpeaking(null);
    onEnd?.();
  };
  utterance.onend = done;
  utterance.onerror = done;
  speechSynthesis.speak(utterance);
}

/** Everything the page itself is reading is let go: the browser's voice, its timers, its duck, the
 * line shown. What the player has placed is the player's to let go (`ReadingHost.recast`). */
export function quietReading(): void {
  hushBrowserVoice();
  readingAt = null;
  setSpeaking(null);
}

/** The browser's voice cut off: its line, its timers and its duck let go, and its line no longer
 * shown. Where the reading is, and what the player has placed, are left alone — at a live edit's
 * hand-over the reading goes on in the new version (SPEC.md §4.7: a line being spoken from the
 * previous version is cut off where the new one takes over). */
export function hushBrowserVoice(): void {
  for (const timer of readingTimers) clearTimeout(timer);
  readingTimers.clear();
  if ('speechSynthesis' in window) speechSynthesis.cancel();
  host?.duck(false);
  if (speaking?.utterance) setSpeaking(null);
}

// The music's bars, in seconds from the piece's start, and its length last: for fitting a line to
// its bar's room.
export function barSeconds(reading: Pick<Reading, 'bars' | 'cpm'>): number[] {
  return reading.bars.map((cycles) => cycles * 60 / reading.cpm);
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

export function noteLoss(text: string): void {
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
    engines: OFFERED_ENGINES.map((name) => {
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
  }
  host?.recast(playing.value);
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
  applyVoiceLevel();
  saveReading();
}

/** The voice's level as it is heard: its fader's, or nothing while the mixer mutes it (or solos
 * another channel). */
export function voiceLevel(): number {
  return audible('voice') ? settingsNow().level : 0;
}
/** The voice's level, set on whatever is reading now: the player's reading stem takes it as its
 * gain (player/index.ts `sendGains`); the browser's voice, at its next line. A muted voice stops. */
export function applyVoiceLevel(): void {
  host?.level();
  if (!audible('voice') && 'speechSynthesis' in window) speechSynthesis.cancel();
}

/** A preset in another language: the old poem's reading is let go first — stopped, and its lines
 * out of the render queue — so its lines are never rendered in the new poem's voice, nor the new
 * poem's in the old one's: until the new poem has compiled there is nothing to read. */
export function readIn(code: string): void {
  if (code === language.value) return;
  quietReading();
  speech.playing = null;
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
  startSystem();
  document.addEventListener('visibilitychange', letVoicesGo);
}

// A loaded neural voice keeps its model and working memory in its worker until it is let go: about
// 800 MB for Kokoro on WebAssembly, measured on a Pixel, which a phone short of memory swaps out
// once another app is in front — and then every app stalls. So hidden and silent, the page lets
// each loaded voice go (a download under way carries on), and the phone's own engine with them;
// the next Play loads them again, as at start-up, and the lines they rendered are kept.
export function letVoicesGo(): void {
  if (document.visibilityState !== 'hidden' || playing.value) return;
  for (const name of [...NEURAL_ENGINES, 'system'] as const) {
    if (engines[name].state === 'ready') engines[name].cancel?.();
  }
}
