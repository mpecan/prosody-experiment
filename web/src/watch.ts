// Watch: the roll and the lit poem.
//
// Strudel's own drawing (`@strudel/draw`, the version matching the page's Strudel) is loaded only
// once Strudel has started: its imports of `@strudel/core` are answered from the playing Strudel
// by the import map, like the soundfonts'. Its `Drawer` calls back every animation frame with the
// notes near the playhead and the scheduler's time; the same time lights the poem, so the roll
// and the verse cannot disagree about what is sounding.
//
// The lit verse is drawn by hand rather than rendered by Preact: it is re-cut on every compile and
// relit every frame, and a frame's work is a few class changes on the pieces that changed.
import type { Mark, Trace } from './answers';
import { piece } from './piece';
import { folder, hearing, poem, rollBars } from './store';
import type { Hap, Pattern, Scheduler } from './strudel/globals';

const DRAW = 'https://unpkg.com/@strudel/draw@1.2.6';

interface Drawer {
  start(scheduler: Scheduler): void;
  stop(): void;
  invalidate(scheduler: Scheduler): void;
  setDrawTime(span: [number, number]): void;
}
type DrawPianoroll = (options: Record<string, unknown>) => void;

/** Lit pieces of the verse: a run of its text, and the element showing it. */
interface Piece {
  start: number;
  end: number;
  element: HTMLSpanElement;
}

export const watch = {
  drawer: null as Drawer | null,
  drawRoll: null as DrawPianoroll | null,
  drawing: false,
  /** The trace of the pattern playing, evaluated: queried for which syllable sounds. */
  tracePattern: null as Pattern | null,
  // Where the bars of the pattern now playing fall, in seconds (barsOf): taken from that pattern's
  // own trace when it starts, so the roll keeps the clock of what is sounding while the text box
  // runs ahead of it.
  heardBars: null as number[] | null,
  /** The verse under the text box, the text box, and the roll — set by the components. */
  litBox: null as HTMLElement | null,
  poemBox: null as HTMLTextAreaElement | null,
  canvas: null as HTMLCanvasElement | null,
};

/** The poem as compiled, one element per piece of it a mark can light. */
let lit: { trace: Trace | null; marks: HTMLSpanElement[][]; pieces: Piece[] } = { trace: null, marks: [], pieces: [] };

export async function startDrawing(): Promise<void> {
  try {
    const [{ Drawer }, { drawPianoroll }] = await Promise.all([
      import(/* @vite-ignore */ `${DRAW}/draw.mjs`) as Promise<{ Drawer: new (fn: (haps: Hap[], time: number) => void, span: [number, number]) => Drawer }>,
      import(/* @vite-ignore */ `${DRAW}/pianoroll.mjs`) as Promise<{ drawPianoroll: DrawPianoroll }>,
    ]);
    watch.drawRoll = drawPianoroll;
    watch.drawer = new Drawer((haps, time) => {
      paintRoll(haps, time);
      lightUp(time);
    }, rollWindow());
  } catch (error) {
    // The music does not depend on the picture of it.
    console.warn('[watch] Strudel\'s drawing could not be loaded:', error);
  }
}

// Where the bars of a trace (prosody_emit::trace) fall on the scheduler's clock, in seconds: each
// bar's start, and last the piece's length, where it loops round. The trace is built by the music's
// own walk: under the steady pulse a bar lasts its beats, so bars of three and four beats are not
// evenly spaced; under the elastic pulse every bar is one cycle. A cycle lasts `60 / cpm` on a
// scheduler at one cycle a second. Null without a trace that says so.
export function barsOf(trace: Trace | null): number[] | null {
  if (!trace || !Array.isArray(trace.bars) || trace.bars.length < 2 || !(trace.cpm > 0)) return null;
  const cycle = 60 / trace.cpm;
  return trace.bars.map((at) => at * cycle);
}

// The bars of the piece being heard: while it plays, those of the pattern that is sounding — not
// of the text box, which an edit puts ahead of the music until the next compile is playing — and
// otherwise those of the setting's last compile. The bars are times, not places in the text, so
// unlike the lit verse they need no guard on the text box. Only with no trace at all is there
// nothing to go by; a neutral two-second bar then keeps the roll drawable, claiming no meter. (The
// pattern's `.cpm` alone is no bar length: under the steady pulse a cycle is a beat.)
function barTimes(): number[] {
  return watch.heardBars || barsOf(piece.traces[hearing.value]) || [0, 2];
}

// How long a bar lasts on average: the piece's length over its bars. Equal to every bar's length
// under the elastic pulse; under the steady pulse, the mean of bars that differ.
function barLength(): number {
  const times = barTimes();
  return (times[times.length - 1] ?? 2) / (times.length - 1);
}

// The roll shows about a whole number of bars — the setting, at the mean bar length — half behind
// the playhead and half ahead.
function rollWindow(): [number, number] {
  const half = (Number(rollBars.value) || 4) * barLength() / 2;
  return [-half, half];
}

export function fitRoll(): void {
  watch.drawer?.setDrawTime(rollWindow());
}

function rollContext(): CanvasRenderingContext2D | null {
  const canvas = watch.canvas;
  if (!canvas) return null;
  const ratio = window.devicePixelRatio || 1;
  const [w, h] = [Math.round(canvas.clientWidth * ratio), Math.round(canvas.clientHeight * ratio)];
  if (canvas.width !== w || canvas.height !== h) [canvas.width, canvas.height] = [w, h];
  return canvas.getContext('2d');
}

function paintRoll(haps: Hap[], time: number): void {
  if (!watch.drawRoll || folder.value !== 'roll') return;
  const ctx = rollContext();
  if (!ctx) return;
  const span = rollWindow();
  // Only the music: every layer of it carries a colour, and the reading's cues carry none.
  watch.drawRoll({
    ctx, time, haps: haps.filter((hap) => hap.value?.color),
    drawTime: span, fold: 1,
    background: '#18171f', playheadColor: '#e6e3ef', inactive: '#4a4760', active: '#ffffff',
    colorizeInactive: 1, fillActive: 1,
  });
  // Bar lines, where one bar of the arrangement hands over to the next — at the trace's bar starts,
  // repeated each time the piece loops round, so they fall where the music's bars change length.
  const [from, to] = [time + span[0], time + span[1]];
  const times = barTimes();
  const length = times[times.length - 1] ?? 2;
  ctx.save();
  ctx.strokeStyle = 'rgba(230, 227, 239, 0.16)';
  ctx.lineWidth = Math.max(1, (window.devicePixelRatio || 1));
  for (let loop = Math.floor(from / length); loop * length <= to; loop++) {
    for (const start of times.slice(0, -1)) {
      const at = loop * length + start;
      if (at < from || at > to) continue;
      const x = Math.round(((at - from) / (to - from)) * ctx.canvas.width) + 0.5;
      ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, ctx.canvas.height); ctx.stroke();
    }
  }
  ctx.restore();
}

export function clearRoll(): void {
  const ctx = rollContext();
  ctx?.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
}

/** Where the verse is cut so that each mark, line and word lights a run of whole pieces. */
export function cutsOf(text: string, marks: { start: number; end: number }[], spans: { from: number; to: number }[]): number[] {
  const cuts = new Set([0, text.length]);
  for (const mark of marks) {
    if (mark.end > mark.start) { cuts.add(mark.start); cuts.add(mark.end); }
  }
  for (const at of spans) {
    if (at.to > at.from) { cuts.add(at.from); cuts.add(at.to); }
  }
  return [...cuts].filter((at) => at <= text.length).sort((a, b) => a - b);
}

// The verse under the text box, cut at every mark's edges so each mark lights a run of whole
// pieces. Marks are used only while the text box says exactly what they were compiled from —
// mid-keystroke the verse is plain text until the next compile catches up, rather than lighting
// letters a few places off. Positions are UTF-16 units, as the server counts them.
export function buildLit(): void {
  const box = watch.litBox;
  if (!box) return;
  const text = poem.value;
  const trace = piece.traces.poem === text ? piece.traces[hearing.value] : null;
  lightUp(null);
  // The reading's lines too, so a line being read is a run of whole pieces, and its words, so the
  // word being said is a run of whole pieces too.
  const reading = piece.readings.poem === text ? piece.readings[hearing.value] : null;
  const spans = [...(reading?.cues || []), ...(reading?.lines || [])].flatMap((line) => [line, ...(line.words || [])]);
  const edges = cutsOf(text, trace?.marks || [], spans);
  const pieces: Piece[] = [];
  const fragment = document.createDocumentFragment();
  for (let i = 0; i + 1 < edges.length; i++) {
    const [start = 0, end = 0] = [edges[i], edges[i + 1]];
    const span = document.createElement('span');
    span.textContent = text.slice(start, end);
    pieces.push({ start, end, element: span });
    fragment.append(span);
  }
  // A text box keeps the line after a final newline; a block of text does not. One space keeps
  // the two the same height, so their scrolling stays in step.
  fragment.append(' ');
  box.replaceChildren(fragment);
  box.scrollTop = watch.poemBox?.scrollTop ?? 0;
  lit = {
    trace,
    pieces: piece.readings.poem === text ? pieces : [],
    marks: trace ? trace.marks.map((mark) =>
      pieces.filter((p) => p.start >= mark.start && p.start < mark.end).map((p) => p.element)) : [],
  };
  showReading();
}

// ---- The reading, shown behind the words ----
//
// The line being read, and the word being said. The browser's voice reports its words as it says
// them (`boundary` events; some voices send none, and then only the line shows). A rendered line
// reports nothing, so its words are spread over its length by how many syllables each sounds — the
// cue carries them — which is where a reader's time goes.

/** The line being read: by the browser's voice, or rendered and started at `start` for `duration`. */
export interface Speaking {
  line: { from: number; to: number; text: string; words?: { from: number; to: number; syllables: number }[] };
  utterance?: SpeechSynthesisUtterance;
  start?: number;
  duration?: number;
}
export let speaking: Speaking | null = null;
export function setSpeaking(shown: Speaking | null, show = true): void {
  speaking = shown;
  if (show) showReading();
}
let wordFrame = 0;
function showReading(): void {
  for (const p of lit.pieces) {
    const inLine = !!speaking && p.start < speaking.line.to && p.end > speaking.line.from;
    p.element.classList.toggle('reading', inLine);
    if (!inLine) p.element.classList.remove('spoken');
  }
}
function markSpoken(from: number, to: number): void {
  for (const p of lit.pieces) p.element.classList.toggle('spoken', p.start < to && p.end > from);
}
/** The word the browser's voice says it is at. */
export function showSpoken(event: SpeechSynthesisEvent): void {
  if (!speaking) return;
  const { line } = speaking;
  const at = event.charIndex;
  const length = event.charLength || (line.text.slice(at).match(/^\S+/) || [''])[0].length;
  markSpoken(line.from + at, line.from + at + Math.max(1, length));
}
/** The words of a rendered line, followed by how far into it the audio clock is. */
export function followWords(shown: Speaking, now: () => number): void {
  cancelAnimationFrame(wordFrame);
  const words = shown.line.words || [];
  const total = words.reduce((sum, word) => sum + word.syllables, 0);
  if (!total) return;
  const step = () => {
    if (speaking !== shown) return;
    const into = (now() - (shown.start ?? 0)) / (shown.duration || 1);
    let reached = 0;
    const word = words.find((w) => (reached += w.syllables) / total > into) || words[words.length - 1];
    if (word) markSpoken(word.from, word.to);
    wordFrame = requestAnimationFrame(step);
  };
  step();
}

// ---- Lighting the verse ----

let shining = new Map<HTMLElement, Mark>();   // element → the mark lighting it
export function lightUp(time: number | null): void {
  const next = new Map<HTMLElement, Mark>();
  if (time !== null && watch.tracePattern && lit.trace) {
    for (const hap of watch.tracePattern.queryArc(time, time + 1e-4)) {
      if (!hap.whole) continue;
      const [begin, end] = [hap.whole.begin.valueOf(), hap.whole.end.valueOf()];
      if (time < begin || time >= end) continue;
      const id = Number(hap.value?.n);
      const mark = lit.trace.marks[id];
      if (!mark) continue;
      for (const element of lit.marks[id] || []) {
        const held = next.get(element);
        if (!held || (mark.fresh && !held.fresh)) next.set(element, mark);
      }
    }
  }
  let follow: HTMLElement | null = null;
  for (const [element] of shining) if (!next.has(element)) element.classList.remove('on', 'echo');
  for (const [element, mark] of next) {
    if (shining.get(element) === mark) continue;
    if (mark.fresh && !follow) follow = element;
    element.classList.toggle('on', mark.fresh);
    element.classList.toggle('echo', !mark.fresh);
    element.style.setProperty('--c', mark.colour);
    element.style.setProperty('--u', mark.melody ? '#ff7aa8' : mark.bass ? '#ffb86b' : 'transparent');
  }
  shining = next;
  // A long poem scrolls to keep the line being heard anew in view; a loop never pulls it away.
  if (follow) followInView(follow);
}

// Scrolls the text box — the verse follows it — so `element` sits in view, unless the writer has
// the caret in the poem, when the page leaves their place alone.
function followInView(element: HTMLElement): void {
  const box = watch.poemBox, verse = watch.litBox;
  if (!box || !verse || document.activeElement === box) return;
  const frame = verse.getBoundingClientRect(), at = element.getBoundingClientRect();
  if (at.top >= frame.top + 32 && at.bottom <= frame.bottom - 32) return;
  const target = box.scrollTop + (at.top - frame.top) - frame.height / 2;
  const still = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  box.scrollTo({ top: Math.max(0, target), behavior: still ? 'auto' : 'smooth' });
}
