// The reading's page side (src/player/speech.ts) with the browser's own voice: a line the worker
// says is due is spoken when the playhead reaches it, cut off where a new version takes over, and its
// duck follows the Mixer's switch at once. The voice and the player are stubbed; frames are at
// 1 000 a second, so a frame is a millisecond.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Reading } from '../src/answers';
import { duckChanged, handing, speechAt, speechStats, startSpeech, stopSpeech, told } from '../src/player/speech';
import { quietReading } from '../src/reading';
import { duck, readingNote, readingSettings } from '../src/store';
import { speaking } from '../src/watch';
import { useFixture } from './site.fixture';

const TEXT = 'Tyger Tyger, burning bright,';
const LINE = { line: 0, stanza: 0, start: 0, end: 28, from: 0, to: 28, syllables: 7, pace: 1, pause: 0.5, ends: 'stanza', words: [], text: TEXT };
const READING = {
  pattern: '', natural: '', bars: [0, 1], cpm: 30, fill: 0.9, cues: [], lines: [{ ...LINE, onset: 'arrives', slot: 0 }], unread: [], natural_unread: [],
} as unknown as Reading;

/** What the page sent the worklet, and the utterances the browser was asked to speak. */
let toWorklet: Record<string, unknown>[] = [];
let toWorker: Record<string, unknown>[] = [];
let utterances: { text: string; onstart?: () => void; onend?: () => void }[] = [];
let cancels = 0;

class FakeUtterance {
  rate = 1;
  volume = 1;
  lang = '';
  voice: unknown = null;
  onstart?: () => void;
  onend?: () => void;
  onerror?: () => void;
  onboundary?: () => void;
  constructor(readonly text: string) {}
}

beforeEach(() => {
  useFixture();
  readingSettings.value = { engine: 'browser', pace: 'natural', level: 1, voices: {} };
  duck.value = true;
  toWorklet = [];
  toWorker = [];
  utterances = [];
  cancels = 0;
  vi.stubGlobal('SpeechSynthesisUtterance', FakeUtterance);
  vi.stubGlobal('speechSynthesis', {
    speak: (utterance: FakeUtterance) => { utterances.push(utterance); },
    cancel: () => { cancels += 1; },
    getVoices: () => [],
    addEventListener: () => {},
  });
  startSpeech({
    post: (message) => { toWorker.push(message); },
    worklet: (message) => { toWorklet.push(message); },
    gains: () => {},
    playhead: () => 0,
    rate: () => 1_000,
    generation: () => 1,
  });
  handing(1, READING, true);
});

afterEach(() => {
  // As the player's Stop does.
  quietReading();
  stopSpeech();
  vi.unstubAllGlobals();
});

/** The worker says line 0 is due at frame `at` for the browser's voice; the playhead reaches it and
 * the voice starts. */
function sayAt(at: number): void {
  told(1, { op: 'say', serial: 1, line: 0, pace: 'natural', at, late: false, chain: 1 });
  speechAt(at);
  expect(utterances.map((u) => u.text)).toEqual([TEXT]);
  utterances[0]!.onstart?.();
}

describe('the browser\'s voice through the player', () => {
  it('is cut off where a new version takes over, its duck released and its line no longer shown', () => {
    sayAt(100);
    expect(toWorklet.at(-1)).toEqual({ duck: 0.6 });
    expect(speaking?.utterance).toBeTruthy();
    const before = cancels;
    // The hand-over at frame 300, told ahead (as the worker renders ahead).
    told(1, { op: 'cut', at: 300 });
    speechAt(250);
    expect(cancels).toBe(before);
    speechAt(300);
    expect(cancels).toBe(before + 1);
    expect(toWorklet.at(-1)).toEqual({ duck: 1 });
    expect(speaking).toBeNull();
  });

  it('is not cut by a cut placed before its line began', () => {
    told(1, { op: 'cut', at: 50 });
    sayAt(100);
    const before = cancels;
    speechAt(120);
    expect(cancels).toBe(before);
    expect(speaking?.utterance).toBeTruthy();
  });

  it('follows the Mixer\'s duck switch at once while it has the music down', () => {
    sayAt(100);
    expect(toWorklet.at(-1)).toEqual({ duck: 0.6 });
    duck.value = false;
    duckChanged();
    expect(toWorklet.at(-1)).toEqual({ duck: 1 });
    duck.value = true;
    duckChanged();
    expect(toWorklet.at(-1)).toEqual({ duck: 0.6 });
  });

  it('leaves the worklet alone when the switch moves with no line of its own sounding', () => {
    duck.value = false;
    duckChanged();
    expect(toWorklet).toEqual([]);
  });
});

describe('the browser\'s voice after the page was hidden or asleep', () => {
  /** Three lines, each a stanza of its own, so none waits on another. */
  const lines = [0, 1, 2].map((n) => ({ ...LINE, line: n, stanza: n, text: `line ${n + 1}`, onset: 'arrives', slot: n }));
  const THREE = { ...READING, lines } as unknown as Reading;

  beforeEach(() => {
    handing(2, THREE, true);
  });

  it('skips the lines whose moment passed while no frame was drawn, and says so', () => {
    // Due at 1, 3 and 5 s; the page draws again only at 9 s (its animation frames stopped while it
    // was hidden, and the music played on).
    for (const [line, at] of [[0, 1_000], [1, 3_000], [2, 5_000]] as const) {
      told(1, { op: 'say', serial: 2, line, pace: 'natural', at, late: false, chain: 0 });
    }
    speechAt(9_000);
    expect(utterances).toEqual([]);
    expect(speechStats().skipped).toBe(3);
    expect(readingNote.value).toMatch(/not read: lines 1, 2, 3/);
  });

  it('still speaks a line reached within a moment of its time, and only the stale ones are skipped', () => {
    told(1, { op: 'say', serial: 2, line: 0, pace: 'natural', at: 1_000, late: false, chain: 0 });
    told(1, { op: 'say', serial: 2, line: 1, pace: 'natural', at: 3_000, late: false, chain: 0 });
    speechAt(3_200);
    expect(utterances.map((u) => u.text)).toEqual(['line 2']);
    expect(speechStats().skipped).toBe(1);
    expect(readingNote.value).toMatch(/not read: line 1,/);
  });

  it('lets the natural pace\'s chain go on from now past a skipped line', () => {
    // A chain waits for the browser's voice to say it has finished; a line never spoken never would.
    told(1, { op: 'say', serial: 2, line: 0, pace: 'natural', at: 1_000, late: false, chain: 7 });
    speechAt(4_000);
    expect(utterances).toEqual([]);
    expect(toWorker).toContainEqual({ op: 'said', chain: 7, end: 4_000 });
  });
});
