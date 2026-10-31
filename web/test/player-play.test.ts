// The Prosody player's Play and Stop on the page (src/player/index.ts), with the audio stubbed: a
// play() still loading when Stop is pressed, or the player switched, must give up — not start sound
// after the Stop, nor sound the Stop button can no longer reach. The compiler is held at the
// Performance request so each test can act while play() waits on it.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { piece } from '../src/piece';
import { player, playing, poem } from '../src/store';

/** The Performance request, held until the test lets it answer. */
let answer: (() => void) | null = null;
const asked = vi.fn();
vi.mock('../src/compiler', () => ({
  ask: vi.fn(async (path: string) => {
    asked(path);
    await new Promise<void>((resolve) => { answer = resolve; });
    return new Response(JSON.stringify({ seconds: 2, cpm: 30, bars: [{ slot: 0, at: 0 }], notes: [], marks: [] }));
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
  toWorker.length = 0;
  answer = null;
});

afterEach(async () => {
  const prosody = await import('../src/player');
  prosody.stop();
  vi.unstubAllGlobals();
});

describe('the Prosody player', () => {
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

  it('gives up when the player is switched to Strudel during its first load', async () => {
    const transport = await import('../src/transport');
    player.value = 'prosody';
    const playingNow = transport.play();
    await heldAtThePerformance();
    transport.choosePlayer('strudel');
    // The switch reaches the player through its module's promise: let it.
    await Promise.resolve();
    await Promise.resolve();
    answer!();
    await playingNow;
    expect(starts()).toEqual([]);
    expect(playing.value).toBe(false);
    expect(player.value).toBe('strudel');
  });
});
