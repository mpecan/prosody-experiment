// The Prosody player's Play and Stop on the page (src/player/index.ts), with the audio stubbed: a
// play() still loading when Stop is pressed must give up — not start sound after the Stop — and a
// piece is handed to the worker with its reading. The compiler is held at the Performance request so
// each test can act while play() waits on it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { piece } from '../src/piece';
import { engines } from '../src/reading';
import { playing, poem, readingSettings } from '../src/store';

/** The one line the Performance's reading cues: line 0, where the piece begins, at the natural pace. */
const CUE = { line: 0, stanza: 0, from: 0, to: 28, pace: 1, pause: 1.5, fitted: null, natural: { onset: 'arrives', slot: 0, at: 0 } };

/** The Performance request, held until the test lets it answer. */
let answer: (() => void) | null = null;
const asked = vi.fn();
vi.mock('../src/compiler', () => ({
  ask: vi.fn(async (path: string) => {
    asked(path);
    await new Promise<void>((resolve) => { answer = resolve; });
    return new Response(JSON.stringify({
      seconds: 2, cpm: 30, bars: [{ slot: 0, at: 0 }], notes: [], marks: [], cues: [CUE], unread: [], natural_unread: [],
    }));
  }),
}));
vi.mock('../src/player/worklet?worker&url', () => ({ default: 'worklet.js' }));

/** Every message the page sent the render worker. */
const toWorker: Record<string, unknown>[] = [];
class FakeWorker {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  postMessage(message: Record<string, unknown>): void {
    toWorker.push(message);
    if (message.op === 'init') queueMicrotask(() => this.onmessage?.({ data: { op: 'ready' } }));
  }
}
class FakeContext {
  sampleRate = 48_000;
  currentTime = 0;
  outputLatency = 0;
  baseLatency = 0;
  destination = {};
  audioWorklet = { addModule: async () => {} };
  async resume(): Promise<void> {}
}
class FakeNode {
  port = { postMessage: () => {}, onmessage: null };
  connect(): void {}
}

/** The ops the page sent the worker that start sound. */
const starts = () => toWorker.filter((m) => m.op === 'play' || m.op === 'swap').map((m) => m.op);

/** Waits until play() is held at the Performance request. */
async function heldAtThePerformance(): Promise<void> {
  await vi.waitFor(() => expect(answer).not.toBeNull());
}

beforeEach(() => {
  vi.stubGlobal('Worker', FakeWorker);
  vi.stubGlobal('AudioContext', FakeContext);
  vi.stubGlobal('AudioWorkletNode', FakeNode);
  piece.latest = '// compiled from a poem by prosody\n// 2 bars from 1 line\nstack()';
  piece.fromTheTop = false;
  poem.value = 'Tyger Tyger, burning bright,';
  playing.value = false;
  // The browser's voice loads nothing: Play fetches no engine unless a test asks for one.
  readingSettings.value = { engine: 'browser', pace: 'natural', level: 1, voices: {} };
  toWorker.length = 0;
  answer = null;
});

afterEach(async () => {
  const prosody = await import('../src/player');
  prosody.stop();
  vi.unstubAllGlobals();
});

describe('the Prosody player', () => {
  it('hands the worker the reading with the piece: its cues, who reads each line, the pace and the duck', async () => {
    const text = poem.value;
    const line = { line: 0, stanza: 0, start: 0, end: 28, from: 0, to: 28, syllables: 7, pace: 1, pause: 1.5, ends: 'stanza', words: [], text };
    piece.readings = {
      A: { pattern: '', natural: '', bars: [0, 1], cpm: 30, fill: 0.9, cues: [], lines: [{ ...line, onset: 'arrives', slot: 0 }], unread: [], natural_unread: [] },
      B: null,
      poem: text,
    };
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    answer!();
    await playingNow;
    const play = toWorker.find((m) => m.op === 'play')!;
    expect(play.cues).toEqual([CUE]);
    // No engine has loaded, so the browser's voice reads it (null): the worker tells the page when.
    expect(play.cast).toEqual([[0, null]]);
    expect(play.pace).toBe('natural');
    expect(play.duck).toBe(0.6);
    piece.readings = { A: null, B: null, poem: '' };
  });

  it('starts loading a saved live engine on Play, so it reads once it is ready', async () => {
    // A returning reader whose engine is Kokoro: nothing loads it at start-up (its model is hundreds
    // of MB), so Play must, or the browser's voice reads every line until a setting changes.
    readingSettings.value = { engine: 'kokoro', pace: 'natural', level: 1, voices: {} };
    engines.kokoro.state = 'idle';
    const load = vi.spyOn(engines.kokoro, 'load').mockImplementation(() => new Promise<void>(() => {}));
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    expect(load).toHaveBeenCalledTimes(1);
    await heldAtThePerformance();
    answer!();
    await playingNow;
    load.mockRestore();
  });

  it('hands over no cues for a reading compiled from another text', async () => {
    piece.readings = { A: null, B: null, poem: 'something else' };
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    answer!();
    await playingNow;
    expect(toWorker.find((m) => m.op === 'play')!.cues).toEqual([]);
  });

  it('plays once the Performance arrives, from the top', async () => {
    // The control: without a Stop, the same steps do start sound, so the tests below can fail.
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    answer!();
    await playingNow;
    expect(starts()).toEqual(['play']);
    expect(playing.value).toBe(true);
  });

  it('gives up when Stop comes while the Performance is on its way', async () => {
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    prosody.stop();
    answer!();
    await playingNow;
    expect(starts()).toEqual([]);
    expect(playing.value).toBe(false);
  });

  it('gives up a live edit\'s hand-over when Stop comes first', async () => {
    // Playing, then an edit (not from the top: a swap) whose Performance is still on its way when
    // Stop is pressed. Sent anyway, the worker would take it as a start of play it never pumps,
    // and the page would show Playing over silence.
    const prosody = await import('../src/player');
    const first = prosody.play();
    await heldAtThePerformance();
    answer!();
    await first;
    expect(playing.value).toBe(true);
    toWorker.length = 0;
    answer = null;
    poem.value = 'Tyger Tyger, burning brighter,';
    const edit = prosody.play();
    await heldAtThePerformance();
    prosody.stop();
    answer!();
    await edit;
    expect(starts()).toEqual([]);
    expect(playing.value).toBe(false);
  });
});
