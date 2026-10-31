// The Prosody player on the page (`docs/player.md` §5): the piece's Performance rendered by our own
// synth in a worker, mixed by an AudioWorklet, and everything the page draws — the lit poem, the
// roll, the meters, the line being read — read from the Performance at the worklet's playhead. It is
// the page's only player: Strudel's runtime is not loaded (the pattern is still shown, copied and
// linked to strudel.cc as text).
//
//   page ──Performance, samples──► render worker ──blocks (MessageChannel)──► worklet ──► output
//     ▲                                                                          │
//     └──────────────────────────── frames played ◄──────────────────────────────┘
//
// The drums, the bass, the melody and the chords, with everything the text asks of them — gain,
// envelope, low-pass, vowel, crush, shape, each channel's compressor, echo and reverb — and the
// reading, its rendered lines placed on the fifth stem by the worker with the duck (speech.ts,
// reader.ts), mixed through the page's master.
import workletUrl from './worklet?worker&url';

import type { Mark, Performance } from '../answers';
import { ask } from '../compiler';
import { site } from '../data';
import { closeLoader, openLoader } from '../loader';
import { audible } from '../mix';
import { piece, query, summaryOf } from '../piece';
import { ensureEngine, prerender, quietReading, voiceLevel } from '../reading';
import type { RollNote } from '../roll';
import { beat, hearing, playing, poem, readingSettings, settings, status, type Channel } from '../store';
import {
  clearRoll, drawFrame, elementsBetween, fitRoll, lightMarks, litVersion, resetMeters, rollWindow, watch,
} from '../watch';
import { indexed, marksAt, notesBetween, pieceTime, settle, type Indexed, type Placement } from './clock';
import { barStarts, records, sampledOf, STEM_CHANNELS } from './notes';
import { decodeDrum, decodeFont, zoneName } from './samples';
import { dropped, handing, speechAt, speechStats, startSpeech, stopSpeech, told, wanted } from './speech';

/** Frames the worker renders at a time. */
const BLOCK = 2048;
/** How far ahead of the playhead the worker keeps rendered (`docs/player.md` §5). */
const LEAD_SECONDS = 0.5;

/** The engine, once started: the audio context, the worklet's node, the worker. */
interface Engine {
  context: AudioContext;
  node: AudioWorkletNode;
  worker: Worker;
}
let engine: Promise<Engine> | null = null;
/** The engine once it has started, for the drawing, which runs every frame. */
let ready: Engine | null = null;

/** What the worklet last said: how far it had played, by the audio clock's `time`. */
let report = { played: 0, underruns: 0, queued: 0, started: false, time: 0, generation: 0, peak: 0, speechPeak: 0 };
/** The loudest the output, and the reading's stem, have been since last asked (the end-to-end test
 * asks). */
let loudest = 0;
let loudestSpeech = 0;
/** What the worker last said it lost. */
let losses = { stolen: 0, missing: 0 };
/** A piece as the page draws it: its Performance indexed, and the text and bar times it was compiled
 * with. Each piece keeps its own, so during a live edit's hand-over the old piece's marks are still
 * found in the old text and the roll shows its bars, until the new one takes over at its bar. */
interface Heard {
  piece: Indexed;
  text: string;
  bars: number[];
}
/** Where each piece plays from, on the worklet's count; the latest that has begun is heard. */
let placements: (Placement & { heard: Heard })[] = [];
/** Pieces handed to the worker, by serial, until it says where they play from. */
const handed = new Map<number, { heard: Heard; fromTheTop: boolean }>();
let serial = 0;
let generation = 0;
/** Counts calls to play(), and Stop: a play() still loading when this has moved on was overtaken —
 * by Stop or a newer play() — and gives up rather than start sound nobody asked for, or sound the
 * Stop button no longer reaches. */
let attempt = 0;
/** The sampled sounds, by key, as the instrument numbers the worker knows them by: given once, in
 * the order first asked for. A sound that failed to load keeps its number, and the synth counts its
 * notes as missing. */
const ids = new Map<string, number>();
/** The sounds loaded into the worker. */
const loaded = new Set<string>();
/** Soundfont keys already loaded, per sound, and the zones that hold them: a later piece playing
 * new keys loads only the zones it does not have. */
const loadedKeys = new Map<string, Set<number>>();
const loadedZones = new Map<string, Set<string>>();
let said = '';
let frame = 0;

function startEngine(): Promise<Engine> {
  engine ??= (async () => {
    const context = new AudioContext();
    await context.audioWorklet.addModule(workletUrl);
    const node = new AudioWorkletNode(context, 'prosody-player', { numberOfInputs: 0, outputChannelCount: [2] });
    node.connect(context.destination);
    node.port.onmessage = ({ data }: MessageEvent) => {
      if (data.generation !== generation) return;
      const ranDry = data.underruns !== report.underruns;
      report = data;
      loudest = Math.max(loudest, data.peak as number);
      loudestSpeech = Math.max(loudestSpeech, (data.speechPeak as number) ?? 0);
      if (ranDry) tell();
    };
    const worker = new Worker(new URL('./render-worker.ts', import.meta.url), { type: 'module' });
    const channel = new MessageChannel();
    node.port.postMessage({ worker: channel.port1 }, [channel.port1]);
    await new Promise<void>((resolve, reject) => {
      worker.onmessage = ({ data }: MessageEvent) => {
        if (data.op === 'ready') resolve();
        else if (data.op === 'error') reject(new Error(data.message));
      };
      worker.postMessage({
        op: 'init',
        bindings: new URL('./synth.js', document.baseURI).href,
        rate: context.sampleRate,
        block: BLOCK,
        lead: Math.round(context.sampleRate * LEAD_SECONDS),
        port: channel.port2,
      }, [channel.port2]);
    });
    worker.onmessage = ({ data }: MessageEvent) => heard(data);
    ready = { context, node, worker };
    startSpeech({
      post: (message, transfer = []) => worker.postMessage(message, transfer),
      worklet: (message) => node.port.postMessage(message),
      gains: sendGains,
      playhead: () => playhead(context),
      rate: () => context.sampleRate,
      generation: () => generation,
    });
    return ready;
  })();
  engine.catch(() => { engine = null; });
  return engine;
}

/** What the worker says. */
function heard(data: Record<string, unknown>): void {
  if (data.op === 'placement') {
    const sent = handed.get(data.serial as number);
    handed.delete(data.serial as number);
    if (!sent) return;
    const placement = {
      outStart: data.outStart as number,
      pieceStart: data.pieceStart as number,
      performance: sent.heard.piece.performance,
      heard: sent.heard,
    };
    placements = sent.fromTheTop ? [placement] : [...placements, placement];
  } else if (data.op === 'reading') {
    told(data.generation as number, data.told as Parameters<typeof told>[1]);
  } else if (data.op === 'want') {
    wanted(data.keys as string[]);
  } else if (data.op === 'dropped') {
    dropped(data.keys as string[]);
  } else if (data.op === 'losses') {
    losses = { stolen: data.stolen as number, missing: data.missing as number };
    tell();
  } else if (data.op === 'error') {
    status('the Prosody player failed: ' + String(data.message), true);
  }
}
/** The status line: what plays, and anything lost (`CLAUDE.md` §6: a fallback is visible). */
function tell(): void {
  if (!playing.value) return;
  const lost = [
    report.underruns ? `ran dry ${report.underruns} time(s), silence instead of music` : '',
    losses.stolen ? `${losses.stolen} note(s) cut short for want of voices` : '',
    losses.missing ? `${losses.missing} note(s) unheard, their sound not loaded` : '',
  ].filter(Boolean);
  status(said + (lost.length ? ' · Prosody player ' + lost.join(', ') : ''), lost.length > 0);
}

/** Loads every sampled sound `performance` needs that the worker does not have. */
async function loadSounds(worker: Worker, performance: Performance): Promise<string[]> {
  const failed: string[] = [];
  await Promise.all([...sampledOf(performance)].map(async ([key, { sound, keys }]) => {
    const have = loadedKeys.get(key);
    const wanted = [...keys].filter((k) => !have?.has(k));
    if (loaded.has(key) && (sound.kind === 'drum' || wanted.length === 0)) return;
    let id = ids.get(key);
    if (id === undefined) {
      id = Math.min(ids.size, 4094);
      ids.set(key, id);
    }
    try {
      if (sound.kind === 'drum') {
        const drum = await decodeDrum(sound.bank, sound.piece);
        worker.postMessage({ op: 'drum', id, ...drum }, [drum.left.buffer, drum.right.buffer]);
      } else {
        const zones = loadedZones.get(key) ?? new Set<string>();
        for (const zone of await decodeFont(sound.file, wanted, zones)) {
          worker.postMessage({ op: 'zone', id, ...zone }, [zone.left.buffer, zone.right.buffer]);
          zones.add(zoneName(zone));
        }
        loadedZones.set(key, zones);
        loadedKeys.set(key, new Set([...(have ?? []), ...wanted]));
      }
      loaded.add(key);
    } catch (error) {
      failed.push(key.replace(/^\w+:/, '') + ' (' + (error as Error).message + ')');
    }
  }));
  return failed;
}

/** Plays the piece of the side heard: from the top when nothing plays or after a preset or an A/B
 * switch, otherwise handing over to the new version at the next bar (`docs/player.md` §5). */
export async function play(): Promise<void> {
  if (!piece.latest) return;
  // The reading's engine starts loading with the music, and its lines rendering: a reader whose
  // saved engine is a live one has it load on Play, not only when a setting changes (reading/index.ts
  // `startReading` loads only the recorded readings' small manifest by itself). Until it is ready
  // the browser's voice stands in, and says so.
  ensureEngine();
  prerender();
  const mine = ++attempt;
  // Overtaken by Stop or a newer play(): checked after every wait.
  const overtaken = () => mine !== attempt;
  const text = poem.value;
  const side = hearing.value;
  const fromTheTop = !playing.value || piece.fromTheTop;
  let running: Engine;
  try {
    if (!engine) {
      status('starting the Prosody player…');
      openLoader({ title: 'Getting the player ready', text: 'The synth and the samples this piece uses. Then it plays.' });
    }
    running = await startEngine();
    await running.context.resume();
  } catch (error) {
    if (!overtaken()) status('could not start the Prosody player: ' + (error as Error).message, true);
    closeLoader();
    return;
  }
  let performance: Performance;
  let failed: string[];
  try {
    if (overtaken()) return;
    const answer = await ask('/performance', query(side), text);
    if (!answer.ok) throw new Error('the Performance could not be made: ' + (await answer.text()));
    performance = (await answer.json()) as Performance;
    if (overtaken() || text !== poem.value || side !== hearing.value) return;   // a newer one plays itself
    status('loading the sounds this piece uses…' + (playing.value ? ' · playing' : ''));
    failed = await loadSounds(running.worker, performance);
  } catch (error) {
    if (!overtaken()) status('the Prosody player could not play this: ' + (error as Error).message, true);
    return;
  } finally {
    closeLoader();
  }
  if (overtaken() || text !== poem.value || side !== hearing.value) return;
  const message = {
    notes: records(performance, ids),
    bars: barStarts(performance),
    seconds: performance.seconds,
    serial: ++serial,
  };
  const heard: Heard = { piece: indexed(performance), text, bars: [...performance.bars.map((bar) => bar.at), performance.seconds] };
  handed.set(message.serial, { heard, fromTheTop });
  // The reading compiled with this Performance: from the same text, so its lines are the cues'.
  const reading = piece.readings.poem === text ? piece.readings[side] : null;
  if (fromTheTop) {
    generation += 1;
    quietReading();
  }
  const reads = { cues: reading ? performance.cues : [], ...handing(message.serial, reading, fromTheTop) };
  if (fromTheTop) {
    report = { played: 0, underruns: 0, queued: 0, started: false, time: 0, generation, peak: 0, speechPeak: 0 };
    losses = { stolen: 0, missing: 0 };
    running.worker.postMessage({ op: 'play', generation, ...message, ...reads }, [message.notes.buffer, message.bars.buffer]);
    // Nothing else is heard now, so the roll takes this piece's bars at once. After a live edit it
    // keeps the old piece's until the new one takes over (draw).
    watch.heardBars = heard.bars;
    fitRoll();
  } else {
    running.worker.postMessage({ op: 'swap', ...message, ...reads }, [message.notes.buffer, message.bars.buffer]);
  }
  piece.fromTheTop = false;
  sendGains();
  playing.value = true;
  showBeat();
  // A line the reading leaves out is said, not left for the listener to miss (SPEC.md §4.7).
  const missing = readingSettings.value.pace === 'fitted' ? reading?.unread : reading?.natural_unread;
  const lost = missing?.length ? ' · not read: ' + missing.map((line) => 'line ' + (line.line + 1) + ', ' + line.why).join('; ') : '';
  if (lost) console.log('[reading]' + lost);
  said = side + ' · ' + summaryOf(piece.latest) + ' · playing' + lost
    + (failed.length ? ' · not loaded: ' + failed.join(', ') : '');
  tell();
  if (!frame) frame = requestAnimationFrame(draw);
}

export function stop(): void {
  // Any play() still loading gives up.
  attempt += 1;
  generation += 1;
  void engine?.then(({ worker }) => worker.postMessage({ op: 'stop', generation }));
  cancelAnimationFrame(frame);
  frame = 0;
  placements = [];
  handed.clear();
  quietReading();
  stopSpeech();
  watch.heardBars = null;
  lightMarks([]);
  clearRoll();
  resetMeters();
  playing.value = false;
  showBeat();
  status('stopped');
}

/** Mute and solo, as stem gains in the worklet: heard within about 5 ms (mixer.ts `GLIDE`). The
 * reading's stem is at the reading's level (0–2, its fader in the Mixer), or nothing while muted. */
export function sendGains(): void {
  ready?.node.port.postMessage({
    gains: STEM_CHANNELS.map((channel) => (channel === 'voice' ? voiceLevel() : audible(channel as Channel) ? 1 : 0)),
  });
}

// ---- The beat on the Play button ----
// Under the steady pulse a cycle is a beat, so the trace's cycles a minute are the tempo; under the
// elastic one there is no single beat to show, and the button stays still.
export function showBeat(): void {
  const trace = piece.traces[hearing.value];
  const steady = (settings.value[hearing.value]?.pulse ?? 'steady') !== 'elastic';
  beat.value = playing.value && steady && trace && trace.cpm > 0 ? 60 / trace.cpm : null;
}

/** The render frame at the speaker now, or null before anything has played: the worklet's count
 * moved on by the audio clock since it said it, never further than it had queued, less the output's
 * latency (`docs/player.md` §5). */
function playhead(context: AudioContext): number | null {
  if (!report.started) return null;
  const rate = context.sampleRate;
  const since = Math.max(0, context.currentTime - report.time) * rate;
  const latency = (context.outputLatency || context.baseLatency || 0) * rate;
  return report.played + Math.min(since, report.queued) - latency;
}

/** Each frame: the playhead's piece and time, then the roll and meters, then the lit poem. */
let marksCache: { performance: Performance | null; version: number; marks: Map<number, { mark: Mark; elements: HTMLElement[] }> } = {
  performance: null, version: -1, marks: new Map(),
};
function draw(): void {
  frame = requestAnimationFrame(draw);
  if (!ready) return;
  const at = playhead(ready.context);
  if (at === null) return;
  placements = settle(placements, at);
  const now = pieceTime(at, placements, ready.context.sampleRate);
  if (!now) return;
  const { heard } = now.placement;
  // The roll's bars switch with the piece heard: at a live edit's hand-over, not when it was sent.
  if (watch.heardBars !== heard.bars) {
    watch.heardBars = heard.bars;
    fitRoll();
  }
  const [from, to] = rollWindow();
  const notes = notesBetween(heard.piece, now.time + from, now.time + to).map(({ note, begin, end }) => rollNoteOf(note, begin, end));
  drawFrame(notes, now.time);
  lightMarks(lit(heard, now.time));
  speechAt(at);
}

/** The colour each lane is drawn in. */
const colourOf = (lane: string) => site.layers.find((layer) => layer.name === lane)?.colour ?? '#888';

/** A note as the roll and the meters draw it. */
function rollNoteOf(note: Indexed['notes'][number], begin: number, end: number): RollNote {
  return { begin, end, colour: colourOf(note.lane), note: note.pitch ?? undefined, muted: !audible(note.channel) };
}

/** The marks lit at `time`, each over its words in the verse — only while the verse shows the text
 * the piece heard was compiled from, whose positions its marks' are. */
function lit(heard: Heard, time: number): { mark: Mark; elements: HTMLElement[] }[] {
  if (heard.text !== poem.value) return [];
  if (marksCache.performance !== heard.piece.performance || marksCache.version !== litVersion()) {
    marksCache = { performance: heard.piece.performance, version: litVersion(), marks: new Map() };
  }
  return marksAt(heard.piece, time).map(({ mark, index }) => {
    let entry = marksCache.marks.get(index);
    if (!entry) {
      entry = {
        mark: { start: mark.from, end: mark.to, colour: colourOf(mark.lane), fresh: mark.fresh, melody: mark.melody, bass: mark.bass },
        elements: elementsBetween(mark.from, mark.to, heard.text),
      };
      marksCache.marks.set(index, entry);
    }
    return entry;
  });
}

// For the end-to-end test (web/e2e): how the player is doing, read from outside.
(globalThis as Record<string, unknown>).prosodyPlayer = {
  stats: () => {
    const at = ready ? playhead(ready.context) : null;
    const peak = loudest;
    const speechPeak = loudestSpeech;
    loudest = 0;
    loudestSpeech = 0;
    return {
      peak,
      speechPeak,
      ...speechStats(),
      underruns: report.underruns,
      started: report.started,
      queued: report.queued,
      playhead: at === null || !ready ? null : at / ready.context.sampleRate,
      stolen: losses.stolen,
      missing: losses.missing,
      /** Pieces handed to the worker, from the top or as a live edit's hand-over. */
      handed: serial,
    };
  },
};
