// The app's native player as the page drives it (src/player/native.ts): the render worker's messages
// over Tauri's invoke, buffers as base64, in order, and what the player says asked for rather than
// pushed; and the page following the app's media buttons. Tauri is a fake that records each call.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { piece } from '../src/piece';
import { base64, startNative, wire } from '../src/player/native';
import { playing, poem, readingSettings } from '../src/store';

/** The Performance request, answered at once. */
vi.mock('../src/compiler', () => ({
  ask: vi.fn(async () => new Response(JSON.stringify({
    seconds: 2, cpm: 30, bars: [{ slot: 0, at: 0 }], notes: [], marks: [], cues: [], unread: [], natural_unread: [],
  }))),
}));
vi.mock('../src/player/worklet?worker&url', () => ({ default: 'worklet.js' }));

/** Every call the page made, and what the next polls answer. */
const calls: { command: string; args?: Record<string, unknown> }[] = [];
let polls: string[] = [];
/** A call the fake holds until the test lets it go, to see whether the next waits for it. */
let hold: ((value?: unknown) => void) | null = null;
let holding = false;

function fakeTauri(): void {
  vi.stubGlobal('__TAURI_INTERNALS__', {
    invoke: async (command: string, args?: Record<string, unknown>) => {
      calls.push({ command, args });
      if (command === 'player_open') return { rate: 44_100 };
      if (command === 'player_poll') return polls.shift() ?? 'null';
      if (command === 'player_post' && holding) {
        holding = false;
        await new Promise((resolve) => { hold = resolve; });
      }
      return null;
    },
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  calls.length = 0;
  polls = [];
  hold = null;
  holding = false;
  fakeTauri();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe('the bridge to the native player', () => {
  it('sends a buffer as base64 of its little-endian bytes', () => {
    // The vectors crates/prosody-app's wire tests read back: 1.0f32 is 00 00 80 3f, 0.5f64 is
    // 00 00 00 00 00 00 e0 3f.
    expect(base64(new Float32Array([1]))).toBe('AACAPw==');
    expect(base64(new Float64Array([0.5]))).toBe('AAAAAAAA4D8=');
    expect(base64(new Float64Array(0))).toBe('');
    // A view into a larger buffer sends its own bytes only.
    expect(base64(new Float32Array([2, 1, 2]).subarray(1, 2))).toBe('AACAPw==');
    const message = wire({ op: 'zone', left: new Float32Array([1]), loopStart: NaN, cast: [[0, null]] });
    expect(JSON.parse(JSON.stringify(message))).toEqual({ op: 'zone', left: 'AACAPw==', loopStart: null, cast: [[0, null]] });
  });

  it('opens at the output\'s rate and posts in order, each after the last has been taken', async () => {
    const heard = vi.fn();
    const engine = await startNative(2048, 0.5, { report: vi.fn(), heard, playing: () => true });
    expect(engine.rate).toBe(44_100);
    expect(calls[0]).toEqual({ command: 'player_open', args: { block: 2048, lead: 0.5 } });
    holding = true;
    engine.post({ op: 'drum', id: 0, left: new Float32Array([1]), right: new Float32Array(0), rate: 44_100 });
    engine.post({ op: 'stop', generation: 1 });
    await vi.advanceTimersByTimeAsync(0);
    // The stop waits for the drum.
    expect(calls.filter((call) => call.command === 'player_post').length).toBe(1);
    hold!();
    await vi.advanceTimersByTimeAsync(0);
    const posts = calls.filter((call) => call.command === 'player_post').map((call) => call.args!.message);
    expect(posts).toEqual([
      { op: 'drum', id: 0, left: 'AACAPw==', right: '', rate: 44_100 },
      { op: 'stop', generation: 1 },
    ]);
    engine.mix({ gains: [1, 0, 1, 1, 1] });
    await vi.advanceTimersByTimeAsync(0);
    expect(calls.at(-1)).toEqual({ command: 'player_mix', args: { gains: [1, 0, 1, 1, 1], duck: null } });
    expect(heard).not.toHaveBeenCalled();
  });

  it('asks how the player is doing, and hands on its report and what it said, in order', async () => {
    const report = vi.fn();
    const heard = vi.fn();
    const engine = await startNative(2048, 0.5, { report, heard, playing: () => true });
    polls.push(JSON.stringify({
      report: { played: 4096, underruns: 0, queued: 2048, started: true, generation: 1, peak: 0.5, speechPeak: 0, latency: 0.02 },
      events: [{ op: 'placement', outStart: 0, pieceStart: 0, serial: 1 }, { op: 'want', keys: [] }],
      lost: 0,
    }));
    await vi.advanceTimersByTimeAsync(40);
    expect(report).toHaveBeenCalledTimes(1);
    expect(report.mock.calls[0]![0]).toMatchObject({ played: 4096, queued: 2048, generation: 1 });
    expect(typeof report.mock.calls[0]![0].time).toBe('number');
    expect(heard.mock.calls.map(([message]) => message.op)).toEqual(['placement', 'want']);
    expect(engine.latency()).toBe(0.02);
    // A player not open answers null, and nothing is handed on.
    await vi.advanceTimersByTimeAsync(40);
    expect(report).toHaveBeenCalledTimes(1);
  });

  it('asks every half second while nothing plays, and sooner once something is posted', async () => {
    let playingNow = false;
    const engine = await startNative(2048, 0.5, { report: vi.fn(), heard: vi.fn(), playing: () => playingNow });
    const asked = () => calls.filter((call) => call.command === 'player_poll').length;
    await vi.advanceTimersByTimeAsync(499);
    expect(asked()).toBe(0);
    await vi.advanceTimersByTimeAsync(1);
    expect(asked()).toBe(1);
    // 100 ms into the next half second, a Play is posted: asked 40 ms after it, not 400.
    await vi.advanceTimersByTimeAsync(100);
    playingNow = true;
    engine.post({ op: 'stop', generation: 1 });
    await vi.advanceTimersByTimeAsync(40);
    expect(asked()).toBe(2);
    // Playing: every 40 ms.
    await vi.advanceTimersByTimeAsync(120);
    expect(asked()).toBe(5);
  });

  it('tells the app when the page is hidden and shown again', async () => {
    await startNative(2048, 0.5, { report: vi.fn(), heard: vi.fn(), playing: () => true });
    let state = 'hidden';
    Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => state });
    document.dispatchEvent(new Event('visibilitychange'));
    state = 'visible';
    document.dispatchEvent(new Event('visibilitychange'));
    await vi.advanceTimersByTimeAsync(0);
    const views = calls.filter((call) => call.command === 'player_view').map((call) => call.args);
    // Earlier players in this file listen too, each telling both: hidden first, shown last.
    expect(views.length).toBeGreaterThanOrEqual(2);
    const half = views.length / 2;
    expect(views.slice(0, half).every((view) => view!.visible === false)).toBe(true);
    expect(views.slice(half).every((view) => view!.visible === true)).toBe(true);
  });
});

describe('the page in the app', () => {
  beforeEach(() => {
    piece.latest = '// compiled from a poem by prosody\n// 2 bars from 1 line\nstack()';
    piece.fromTheTop = false;
    poem.value = '+++\n[style]\nroot = "C"\n+++\n# a note\n\nTyger Tyger, burning bright,\nIn the forests of the night;';
    playing.value = false;
    readingSettings.value = { engine: 'browser', pace: 'natural', level: 1, voices: {} };
  });

  it('plays through the native player, naming the piece for the notification, and stops on its pause button', async () => {
    const prosody = await import('../src/player');
    await prosody.play();
    await vi.advanceTimersByTimeAsync(0);
    const play = calls.find((call) => call.command === 'player_post' && (call.args!.message as { op: string }).op === 'play');
    expect(play).toBeDefined();
    const message = play!.args!.message as Record<string, unknown>;
    expect(message.title).toBe('Tyger Tyger, burning bright,');
    expect(typeof message.notes).toBe('string');
    expect(playing.value).toBe(true);
    // The lock screen's pause: the native player has stopped the music; the page follows.
    polls.push(JSON.stringify({ report: { played: 0, underruns: 0, queued: 0, started: false, generation: 1, peak: 0, speechPeak: 0, latency: 0 }, events: [{ op: 'media', action: 'stop' }], lost: 0 }));
    await vi.advanceTimersByTimeAsync(40);
    expect(playing.value).toBe(false);
    await vi.advanceTimersByTimeAsync(0);
    const last = calls.filter((call) => call.command === 'player_post').at(-1)!.args!.message as Record<string, unknown>;
    expect(last.op).toBe('stop');
  });

  it('takes the title from the poem\'s first line after its header and its notes', async () => {
    const { titleOf } = await import('../src/player');
    expect(titleOf('+++\n[style]\n+++\n\n# about\nThe first line\nThe second')).toBe('The first line');
    expect(titleOf('Just a line')).toBe('Just a line');
    expect(titleOf('')).toBe('Prosody');
  });
});
