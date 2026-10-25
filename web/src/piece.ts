// The piece: the poem compiled under both settings — the patterns, the readings and the traces —
// kept with the text they were compiled from, and measured.
import type { Reading, Trace } from './answers';
import { ask } from './compiler';
import { CONTROLS } from './controls';
import { prerender, showVoices } from './reading';
import { play } from './strudel';
import {
  controls, explainView, hearing, patternView, playing, poem, profile, readingSettings, scores,
  settings, status, type Score, type Side,
} from './store';
import { buildLit } from './watch';

/** What the poem compiled to, per side, and the text it was compiled from. Their positions are that
 * text's, so the lit verse uses them only while the text box still says exactly that. */
export const piece = {
  traces: { A: null, B: null, poem: '' } as { A: Trace | null; B: Trace | null; poem: string },
  readings: { A: null, B: null, poem: '' } as { A: Reading | null; B: Reading | null; poem: string },
  patterns: { A: '', B: '' } as Record<Side, string>,
  /** The pattern of the side heard: what Play plays. */
  latest: '',
  // Set when the next play() must restart Strudel's clock at cycle 0 rather than hand the running
  // scheduler a new pattern: after a preset or an A/B switch. Strudel keeps its cycle count across
  // `setPattern`, so without the restart the new piece (at its own `.cpm`) starts partway through
  // and its reading begins at whatever line that bar first hosts. A live edit keeps the clock
  // running instead — the writer is listening in place — and the reading carries on from there.
  fromTheTop: false,
};

/** The query for `side`: every control, the side's variants, and the profile it is measured against. */
export function query(side: Side = hearing.value): string {
  const p = new URLSearchParams();
  for (const id of CONTROLS) p.set(id, controls.value[id] ?? '');
  p.set('set', Object.entries(settings.value[side]).map(([name, v]) => `${name}:${v}`).join(','));
  p.set('profile', profile.value);
  return p.toString();
}

/** The pattern of the side heard, in the Pattern view. */
export function showPattern(): void {
  piece.latest = piece.patterns[hearing.value];
  const reading = piece.readings[hearing.value];
  const fitted = readingSettings.value.pace === 'fitted';
  // The cue pattern is shown under the music, apart from it: it is not part of what a writer
  // pastes into strudel.cc, where nothing would speak it.
  const cues = reading && (fitted ? reading.cues : reading.lines);
  patternView.value = piece.latest + (cues && cues.length
    ? '\n// ---- spoken by this page, not part of the pattern above ----\n'
      + (fitted ? reading.pattern : reading.natural)
    : '');
}

/** The summary line of a pattern: its second line, `// 24 bars from 8 lines, …`. */
export const summaryOf = (pattern: string) => (pattern.split('\n')[1] || '').replace('// ', '');

/** Switches which side is heard and shown. */
export function hear(side: Side): void {
  hearing.value = side;
  if (readingSettings.value.engine === 'recorded') showVoices();
  showPattern();
  buildLit();
  if (playing.value && piece.latest) {
    piece.fromTheTop = true;
    void play();
  } else status(side + ' · ' + summaryOf(piece.latest));
  ask('/explain', query(), poem.value)
    .then((response) => response.text())
    .then((text) => { explainView.value = text; })
    .catch(() => {});
}

// One measurement at a time, and only of the newest poem: a request made while one is out waits
// for it and then measures whatever the poem says by then, so a writer typing into a long poem
// never queues a score per keystroke (each of "Patterns" takes seconds).
let measuring = false;
let measureNext: string | null = null;
export async function measure(text: string): Promise<void> {
  measureNext = text;
  scores.value = 'measuring';
  if (measuring) return;
  measuring = true;
  try {
    while (measureNext !== null) {
      const asked = measureNext;
      measureNext = null;
      const one = async (side: Side): Promise<Score | null> => {
        const response = await ask('/score', query(side), asked);
        return response.ok ? (await response.json()) as Score : null;
      };
      let measured: [Score | null, Score | null];
      try {
        measured = await Promise.all([one('A'), one('B')]);
      } catch {
        measured = [null, null];
      }
      if (measureNext === null) scores.value = { A: measured[0], B: measured[1] };
    }
  } finally {
    measuring = false;
  }
}

// One compilation at a time, the same way: an edit made while one is out is compiled when it
// returns, so answers never arrive out of order and a long poem's compilations never pile up.
let compiling = false;
let compileAgain = false;
export async function compile(): Promise<void> {
  if (compiling) { compileAgain = true; return; }
  compiling = true;
  try {
    do {
      compileAgain = false;
      await compileOnce();
    } while (compileAgain);
  } finally {
    compiling = false;
  }
}

/** Whether a newer edit is waiting to be compiled. */
export const newerWaiting = () => compileAgain;

async function compileOnce(): Promise<void> {
  const text = poem.value;
  if (!text.trim()) {
    piece.latest = '';
    piece.patterns = { A: '', B: '' };
    piece.readings = { A: null, B: null, poem: '' };
    patternView.value = '';
    explainView.value = '';
    scores.value = { A: null, B: null };
    status('nothing to compile');
    return;
  }
  status('compiling…');
  try {
    const [patternA, patternB, explain, readingA, readingB, traceA, traceB] = await Promise.all([
      ask('/pattern', query('A'), text),
      ask('/pattern', query('B'), text),
      ask('/explain', query(), text),
      ask('/reading', query('A'), text),
      ask('/reading', query('B'), text),
      ask('/trace', query('A'), text),
      ask('/trace', query('B'), text),
    ]);
    piece.traces = {
      A: traceA.ok ? (await traceA.json()) as Trace : null,
      B: traceB.ok ? (await traceB.json()) as Trace : null,
      poem: text,
    };
    piece.readings = {
      A: readingA.ok ? (await readingA.json()) as Reading : null,
      B: readingB.ok ? (await readingB.json()) as Reading : null,
      poem: text,
    };
    const texts = { A: await patternA.text(), B: await patternB.text() };
    explainView.value = await explain.text();
    const failed = !patternA.ok ? texts.A : !patternB.ok ? texts.B : null;
    if (failed !== null) {
      patternView.value = failed;
      piece.latest = '';
      piece.patterns = { A: '', B: '' };
      status(failed.split('\n')[0] ?? failed, true);
      return;
    }
    piece.patterns = texts;
    showPattern();
    buildLit();
    void measure(text);
    // The recorded voices are listed per poem, with how many of its lines each has.
    if (readingSettings.value.engine === 'recorded') showVoices();
    // Every changed line rendered again, ahead of its turn; the unchanged ones are kept. Not for
    // a text already replaced: the compile of the newer one asks for its own lines.
    if (!compileAgain) prerender();
    status(hearing.value + ' · ' + summaryOf(piece.latest) + (playing.value ? ' · playing' : ''));
    // Live reload: keep going with the new version — unless a newer one is already being compiled,
    // which plays instead (and keeps a preset's restart from being spent on the stale one).
    if (playing.value && !compileAgain) void play();
  } catch {
    status('server unreachable — is `prosody-web` still running?', true);
  }
}

let timer: ReturnType<typeof setTimeout> | undefined;
/** Compiles a moment after the last change, so typing compiles once a pause, not once a key. */
export function schedule(): void {
  clearTimeout(timer);
  timer = setTimeout(() => void compile(), 350);
}
