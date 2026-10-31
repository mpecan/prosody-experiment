// The reading through the Prosody player, on the page's side (`docs/player.md` §5, "The reading").
//
// The player times the reading (crates/prosody-player's reader) and places each rendered line on the
// speech stem itself; this side gives it what only the page has — who reads each line (the cast) and
// the lines the engines have rendered, as PCM — and does what only the page can do with what the
// player says:
// show the line being read and the word being said at the playhead, say what the reading lost, and
// speak the lines of the browser's own voice when the playhead reaches them, off the playhead, since
// that voice cannot be placed in a stream.
import type { Reading, SpokenLine } from '../answers';
import {
  castOf, hostReading, hushBrowserVoice, noteLoss, noteReading, readAloud, readingIsAt, renderedFor, renderUrgently, speakerOf,
  speech, type Rendered,
} from '../reading';
import { ENGINE_INFO } from '../reading/engines';
import { duck, readingNote, readingSettings, type Pace } from '../store';
import { followWords, setSpeaking, speaking, type Speaking } from '../watch';

/** What the player's reader tells the page (crates/prosody-player `reader::Told`, as its JSON).
 * Frames are output frames; `serial` names the piece. */
export type Told =
  | { op: 'spoken'; serial: number; line: number; pace: Pace; start: number; end: number; over: number | null; late: number }
  | { op: 'say'; serial: number; line: number; pace: Pace; at: number; late: boolean; chain: number }
  | { op: 'cut'; at: number }
  | { op: 'wait'; serial: number; line: number; key: string }
  | { op: 'note'; text: string; loss: boolean }
  /** Where the reading is: the line it will want next, by number, or null once the round's are read. */
  | { op: 'at'; serial: number; pace: Pace; line: number | null };

/** The music's level under a line, with the duck on (`SPEC.md` §4.7's reading, as the page has always
 * ducked it). */
export const DUCK = 0.6;

/** How late, in seconds, a line of the browser's voice may still be spoken. A page being drawn
 * reaches a due line within a frame of its time — the worker tells it half a second ahead — so a
 * line this late was due while the page was hidden or asleep and its animation frames stopped (the
 * music plays on without them). Spoken on waking it would be out of its bar, and every line overdue
 * would be begun at once, each cutting off the one before; such a line is skipped and said to be. */
export const STALE_SECONDS = 0.5;

/** What the reading side needs of the player. */
export interface Link {
  /** Posts to the render worker. */
  post(message: Record<string, unknown>, transfer?: Transferable[]): void;
  /** Posts to the worklet. */
  worklet(message: Record<string, unknown>): void;
  /** Sends the stems' gains again, the reading's level among them. */
  gains(): void;
  /** The render frame at the speaker now, or null before anything has played. */
  playhead(): number | null;
  rate(): number;
  /** Which start of play is current: what the worker says of an older one is stale. */
  generation(): number;
}

let link: Link | null = null;
/** Each piece handed to the worker, by serial, with the reading compiled with it: whose lines its
 * cues name. */
const readings = new Map<number, Reading | null>();
/** A rendered line placed by the worker, to show while the playhead is in it (render frames). */
interface Shown { line: SpokenLine; start: number; end: number }
let shown: Shown[] = [];
let showing: { entry: Shown; speaking: Speaking } | null = null;
/** A line of the browser's voice the worker says is due, to speak when the playhead reaches it. */
interface Due { line: SpokenLine & { stanzaLast?: boolean }; rate: number; at: number; chain: number; generation: number }
let due: Due[] = [];
/** Where the worker cut the reading off (a hand-over, a fitted cue), in render frames: a line of the
 * browser's voice begun before one is cut off when the playhead gets there, as a placed line is. */
let cuts: number[] = [];
/** The render frame the browser's voice began its latest line at, while it may still be speaking. */
let browserFrom: number | null = null;
/** Whether the browser's voice has the worklet keep the music down: what the Mixer's duck switch
 * changes at once. */
let browserDucked = false;
/** The render keys the worker has asked for and not been sent. */
let wants = new Set<string>();
/** The render keys sent to the worker and not since let go by it: sent with a piece, before the
 * piece itself, so its first lines are there when its first block is rendered. */
const sent = new Set<string>();
/** Lines placed since the last start of play, and those placed exactly where they were due: the
 * end-to-end test's proof the reading played, and played on the music's clock. */
let placedCount = 0;
let onTimeCount = 0;
/** Starts of play from the top, the first line placed since the last, and the pace of the latest:
 * how the end-to-end test tells a restart from a hand-over, and which pace was read. */
let startsCount = 0;
let firstLine: number | null = null;
let lastPace: Pace | null = null;
/** Lines of the browser's voice skipped since the last start of play, their moment passed. */
let skippedCount = 0;

/** The line of `reading` the worker names, at `pace`, with what its cue says. */
function lineOf(reading: Reading | null | undefined, pace: Pace, line: number): (SpokenLine & { rate?: number; slot?: number }) | undefined {
  if (!reading) return undefined;
  return pace === 'fitted' ? reading.cues.find((cue) => cue.line === line) : reading.lines.find((l) => l.line === line);
}

/** The cast the worker reads each piece's lines with: none for a reading let go (a preset in another
 * language, until its own compile plays). */
function casts(): [number, [number, string | null][]][] {
  const pace = readingSettings.value.pace;
  return [...readings].map(([serial, reading]) => [serial, speech.playing ? [...castOf(reading, pace)] : []]);
}

/** Starts the reading side: the player calls it once its engine is up. */
export function startSpeech(player: Link): void {
  link = player;
  hostReading({
    recast: (quiet) => {
      if (quiet) forgetPlaced();
      link?.post({ op: 'cast', casts: casts(), pace: readingSettings.value.pace, quiet });
    },
    rendered: (key, buffer) => send(key, buffer),
    failed: (key) => link?.post({ op: 'failed', key }),
    duck: (down) => {
      browserDucked = down;
      link?.worklet({ duck: down && duck.value ? DUCK : 1 });
    },
    level: () => link?.gains(),
  });
}

/** What to send with a piece handed to the worker: who reads its lines, and the duck. */
export function handing(serial: number, reading: Reading | null, fromTheTop: boolean): Record<string, unknown> {
  if (fromTheTop) {
    readings.clear();
    forgetPlaced();
    placedCount = 0;
    onTimeCount = 0;
    startsCount += 1;
    firstLine = null;
    skippedCount = 0;
  }
  readings.set(serial, reading);
  // Only the pieces the worker may still be playing are kept: the one it plays and one to come.
  while (readings.size > 3) readings.delete(readings.keys().next().value!);
  speech.playing = reading;
  const pace = readingSettings.value.pace;
  const cast = castOf(reading, pace);
  // Every line already rendered goes ahead of the piece, so the worker has it before the piece's
  // first block: asked for after, the first line would wait out the worker's whole lead.
  for (const key of cast.values()) {
    const buffer = key === null || sent.has(key) ? null : renderedFor(key);
    if (key !== null && buffer) post(key, buffer);
  }
  return { cast: [...cast], pace, duck: duck.value ? DUCK : 1 };
}

/** Stop: nothing the reading placed is shown or spoken any more. */
export function stopSpeech(): void {
  readings.clear();
  forgetPlaced();
  speech.playing = null;
  readingNote.value = '';
}

/** The lines shown and due are let go, and the line showing. */
function forgetPlaced(): void {
  shown = [];
  due = [];
  cuts = [];
  browserFrom = null;
  if (showing && speaking === showing.speaking) setSpeaking(null);
  showing = null;
}

/** The duck switched on the Mixer: from what the worker renders next, and at once for a line of the
 * browser's voice that has the music down (`docs/player.md` §12). */
export function duckChanged(): void {
  link?.post({ op: 'duck', level: duck.value ? DUCK : 1 });
  if (browserDucked) link?.worklet({ duck: duck.value ? DUCK : 1 });
}

/** The worker let these recorded lines go: sent again when wanted. */
export function dropped(keys: string[]): void {
  for (const key of keys) sent.delete(key);
}

/** The worker asks for these recorded lines: each one rendered is sent now, the rest when ready. */
export function wanted(keys: string[]): void {
  wants = new Set(keys);
  for (const key of keys) {
    const buffer = renderedFor(key);
    if (buffer) send(key, buffer);
  }
}

/** A rendered line to the worker, if it wants it: its speech as mono PCM, copied (the page keeps its
 * own to send again after a stop). */
function send(key: string, buffer: Rendered): void {
  if (!wants.has(key)) return;
  wants.delete(key);
  post(key, buffer);
}
function post(key: string, buffer: Rendered): void {
  if (!link) return;
  sent.add(key);
  const pcm = buffer.getChannelData(0).slice();
  link.post({ op: 'line', key, pcm, rate: buffer.sampleRate, lead: buffer.lead ?? 0 }, [pcm.buffer]);
}

/** What the player's reader says (`Told`). */
export function told(generation: number, message: Told): void {
  if (!link || generation !== link.generation()) return;
  const rate = link.rate();
  switch (message.op) {
    case 'spoken': {
      const reading = readings.get(message.serial);
      const line = lineOf(reading, message.pace, message.line);
      if (!line) return;
      placedCount += 1;
      if (message.late < 0.001) onTimeCount += 1;
      firstLine ??= line.line;
      lastPace = message.pace;
      shown.push({ line, start: message.start, end: message.end });
      const who = speakerOf(line.text);
      console.log(`[reading] ${message.pace}: line ${line.line + 1} placed at ${(message.start / rate).toFixed(3)} s`
        + (message.late > 0.001 ? ` (${message.late.toFixed(3)} s late)` : '') + ` (bytes ${line.start}..${line.end}), ${who.engine} ${who.voice ?? ''}: ${line.text}`);
      if (who.fallback) noteReading(`line ${line.line + 1}: ${who.why}`);
      else if (message.over !== null && message.over > 0.02) noteReading(`line ${line.line + 1} runs ${message.over.toFixed(2)} s past its bar and may be cut off at the next cue`);
      else if (readingNote.value.startsWith('waiting for')) noteReading('');
      return;
    }
    case 'say': {
      const reading = readings.get(message.serial);
      const line = lineOf(reading, message.pace, message.line);
      if (!line) return;
      const who = speakerOf(line.text);
      if (message.late) renderUrgently(reading ?? null, line, message.pace);
      const why = message.late ? 'not rendered in time' : who.why;
      noteReading(`line ${line.line + 1} read by the browser voice, off the playhead and not in step with the music`
        + (why ? ` (${why})` : ''), !!who.bad);
      const lines = reading?.lines ?? [];
      const next = lines[lines.findIndex((l) => l.line === line.line) + 1];
      due.push({
        line: { ...line, stanzaLast: message.pace === 'natural' && (!next || next.stanza !== line.stanza) },
        rate: message.pace === 'fitted' ? line.rate ?? 1 : line.pace,
        at: message.at,
        chain: message.chain,
        generation,
      });
      return;
    }
    case 'cut':
      shown = shown.filter((entry) => entry.start < message.at).map((entry) => ({ ...entry, end: Math.min(entry.end, message.at) }));
      due = due.filter((entry) => entry.at < message.at);
      cuts.push(message.at);
      return;
    case 'wait': {
      const reading = readings.get(message.serial);
      const line = lineOf(reading, 'natural', message.line);
      if (!line) return;
      const who = speakerOf(line.text);
      if (who.engine !== 'browser') {
        noteReading(`waiting for ${ENGINE_INFO[who.engine].label} to ${who.engine === 'recorded' ? 'fetch' : 'render'} line ${line.line + 1}`);
      }
      renderUrgently(reading ?? null, line, 'natural');
      return;
    }
    case 'note':
      // A line lost at the seam stays beside whatever note follows it for a few seconds.
      if (message.loss) noteLoss(message.text); else noteReading(message.text);
      return;
    case 'at': {
      const reading = readings.get(message.serial);
      if (reading) readingIsAt(reading, message.pace, message.line);
      return;
    }
  }
}

/** Each frame, at render frame `at`: the line placed there shown, its words followed, and any line
 * of the browser's voice now due spoken. */
export function speechAt(at: number): void {
  if (!link) return;
  const rate = link.rate();
  shown = shown.filter((entry) => entry.end > at);
  const entry = shown.find((e) => e.start <= at && at < e.end) ?? null;
  if (entry !== showing?.entry) {
    if (showing && speaking === showing.speaking) setSpeaking(null);
    showing = null;
    if (entry && !speaking?.utterance) {
      const shownLine: Speaking = { line: entry.line, start: entry.start / rate, duration: (entry.end - entry.start) / rate };
      showing = { entry, speaking: shownLine };
      setSpeaking(shownLine);
      followWords(shownLine, () => (link?.playhead() ?? 0) / rate);
    }
  }
  const reached = due.filter((d) => d.at <= at);
  due = due.filter((d) => d.at > at);
  const stale = reached.filter((d) => at - d.at > STALE_SECONDS * rate);
  const ready = reached.filter((d) => at - d.at <= STALE_SECONDS * rate);
  skip(stale, at);
  // A cut reached: a line of the browser's voice begun before it stops there. A line due now cuts
  // it anyway (readAloud), keeping the duck down between the two.
  const cut = cuts.filter((c) => c <= at);
  if (cut.length) {
    cuts = cuts.filter((c) => c > at);
    if (!ready.length && browserFrom !== null && browserFrom < Math.max(...cut)) {
      browserFrom = null;
      hushBrowserVoice();
    }
  }
  for (const line of ready) {
    console.log(`[reading] line ${line.line.line + 1} with the browser voice at ${(at / rate).toFixed(3)} s (due ${(line.at / rate).toFixed(3)} s): ${line.line.text}`);
    browserFrom = at;
    readAloud(line.line, line.rate, line.chain ? () => {
      if (!link || line.generation !== link.generation()) return;
      link.post({ op: 'said', chain: line.chain, end: Math.round(link.playhead() ?? at) });
    } : undefined);
  }
}

/** Lines of the browser's voice whose moment passed unseen (`STALE_SECONDS`): not spoken, but said
 * not to have been (`CLAUDE.md` §6), and a natural pace's chain told it may go on from now. */
function skip(stale: Due[], at: number): void {
  if (!stale.length || !link) return;
  const rate = link.rate();
  skippedCount += stale.length;
  for (const line of stale) {
    console.log(`[reading] line ${line.line.line + 1} not read by the browser voice: due ${(line.at / rate).toFixed(3)} s, reached ${(at / rate).toFixed(3)} s`);
    if (line.chain && line.generation === link.generation()) link.post({ op: 'said', chain: line.chain, end: Math.round(at) });
  }
  const lines = stale.map((line) => line.line.line + 1).join(', ');
  noteLoss(`not read: line${stale.length > 1 ? 's' : ''} ${lines}, due while the page was hidden or asleep`);
}

/** How the reading has gone, for the end-to-end test. */
export const speechStats = () => ({
  spoken: placedCount, onTime: onTimeCount, starts: startsCount, first: firstLine, pace: lastPace, skipped: skippedCount,
});
