// The Prosody player's Play and Stop on the page (src/player/index.ts), with the audio stubbed: a
// play() still loading when Stop is pressed must give up — not start sound after the Stop — and a
// piece is handed to the worker with its reading. The compiler is held at the Performance request so
// each test can act while play() waits on it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { piece } from '../src/piece';
import { engines } from '../src/reading';
import { hearing, playing, poem, readingSettings, statusText } from '../src/store';

/** The one line the Performance's reading cues: line 0, where the piece begins, at the natural pace. */
const CUE = { line: 0, stanza: 0, from: 0, to: 28, pace: 1, pause: 1.5, fitted: null, natural: { onset: 'arrives', slot: 0, at: 0 } };

/** The Performance request, held until the test lets it answer. */
let answer: (() => void) | null = null;
const asked = vi.fn();
vi.mock('../src/compiler', () => ({
  ask: vi.fn(async (path: string, _query: string, text: string) => {
    asked(path, text);
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
/** The page's AudioContext: made once, with the player, for the whole file. */
let context: FakeContext | null = null;
class FakeContext {
  sampleRate = 48_000;
  currentTime = 0;
  outputLatency = 0;
  baseLatency = 0;
  destination = {};
  audioWorklet = { addModule: async () => {} };
  /** Calls to suspend and resume, in order. */
  calls: string[] = [];
  constructor() {
    context = this;
  }
  async resume(): Promise<void> {
    this.calls.push('resume');
  }
  async suspend(): Promise<void> {
    this.calls.push('suspend');
  }
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
  asked.mockClear();
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
    // Its cue as the player's record (notes.ts `cueRecords`): line 0, stanza 0, its 1.5 s pause, no
    // fitted bar, its natural bar's onset (an unnamed onset waits for its bar, as `at` does, 0) at 0 s.
    expect([...(play.cues as Float64Array)]).toEqual([0, 0, 1.5, NaN, NaN, 0, 0]);
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
    expect([...(toWorker.find((m) => m.op === 'play')!.cues as Float64Array)]).toEqual([]);
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

  it('lets the audio rest a second after Stop, and not under a Play that comes sooner', async () => {
    // Left running, the AudioContext calls the worklet on silence for as long as the tab is open.
    const prosody = await import('../src/player');
    const first = prosody.play();
    await heldAtThePerformance();
    answer!();
    await first;
    const audio = context!;
    audio.calls.length = 0;
    let again: Promise<void> = Promise.resolve();
    vi.useFakeTimers();
    try {
      prosody.stop();
      vi.advanceTimersByTime(prosody.REST_AFTER - 1);
      expect(audio.calls).toEqual([]);
      vi.advanceTimersByTime(1);
      expect(audio.calls).toEqual(['suspend']);
      // Stop, then Play within the second: the rest is called off, so Play's output is not
      // suspended under it once it starts.
      audio.calls.length = 0;
      prosody.stop();
      vi.advanceTimersByTime(prosody.REST_AFTER / 2);
      answer = null;
      again = prosody.play();
      vi.advanceTimersByTime(prosody.REST_AFTER);
      expect(audio.calls).not.toContain('suspend');
    } finally {
      vi.useRealTimers();
    }
    await heldAtThePerformance();
    answer!();
    await again;
    expect(audio.calls).toEqual(['resume']);
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

  it('plays the newest text when the poem is edited while the first Play loads', async () => {
    // Nothing plays yet, so the edit's compile calls no play() of its own (it hands over only what
    // is playing): were the first play() to give up, the pressed Play would do nothing, silently.
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    const edited = 'Tyger Tyger, burning brighter,';
    poem.value = edited;
    const first = answer!;
    answer = null;
    first();
    await heldAtThePerformance();
    expect(statusText.value).toMatch(/changed while the player loaded/);
    answer!();
    await playingNow;
    expect(asked.mock.calls.filter(([path]) => path === '/performance').map(([, text]) => text))
      .toEqual(['Tyger Tyger, burning bright,', edited]);
    expect(starts()).toEqual(['play']);
    expect(playing.value).toBe(true);
  });

  it('plays the side chosen while the first Play loads', async () => {
    const prosody = await import('../src/player');
    const playingNow = prosody.play();
    await heldAtThePerformance();
    hearing.value = 'B';
    const first = answer!;
    answer = null;
    first();
    await heldAtThePerformance();
    answer!();
    await playingNow;
    expect(starts()).toEqual(['play']);
    expect(playing.value).toBe(true);
    expect(statusText.value).toMatch(/^B · /);
    hearing.value = 'A';
  });

  it('leaves a live edit\'s newer version to its own compile while playing', async () => {
    // Playing, an edit's hand-over loads, and a second edit lands before it arrives: that edit's
    // compile calls play() itself, so this one gives up rather than load the same text twice.
    const prosody = await import('../src/player');
    const first = prosody.play();
    await heldAtThePerformance();
    answer!();
    await first;
    toWorker.length = 0;
    answer = null;
    asked.mockClear();
    poem.value = 'Tyger Tyger, burning brighter,';
    const edit = prosody.play();
    await heldAtThePerformance();
    poem.value = 'Tyger Tyger, burning brightest,';
    answer!();
    await edit;
    expect(starts()).toEqual([]);
    expect(asked.mock.calls.filter(([path]) => path === '/performance')).toHaveLength(1);
    expect(playing.value).toBe(true);
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
