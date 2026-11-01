// The phone's own voice in the Android app: offered in place of the browser's (which the app's
// WebView has none of), standing in for a voice that cannot read, and rendering a line from the
// WAV file the phone's engine writes.
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { offeredEngines, onPhone, parseWav } from '../src/reading/engines';

/** A mono WAV of `samples`: 16-bit PCM, or 32-bit float; `declared` overrides the data chunk's size. */
function wav(samples: number[], { rate = 24000, float = false, declared, extra = false }: { rate?: number; float?: boolean; declared?: number; extra?: boolean } = {}): ArrayBuffer {
  const width = float ? 4 : 2;
  const pad = extra ? 8 + 3 + 1 : 0;   // an odd-sized chunk before the data, padded to even
  const bytes = new ArrayBuffer(12 + 24 + pad + 8 + samples.length * width);
  const view = new DataView(bytes);
  const tag = (at: number, text: string) => [...text].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
  tag(0, 'RIFF'); view.setUint32(4, bytes.byteLength - 8, true); tag(8, 'WAVE');
  tag(12, 'fmt '); view.setUint32(16, 16, true);
  view.setUint16(20, float ? 3 : 1, true); view.setUint16(22, 1, true);
  view.setUint32(24, rate, true); view.setUint32(28, rate * width, true);
  view.setUint16(32, width, true); view.setUint16(34, width * 8, true);
  let at = 36;
  if (extra) { tag(at, 'LIST'); view.setUint32(at + 4, 3, true); at += pad; }
  tag(at, 'data'); view.setUint32(at + 4, declared ?? samples.length * width, true);
  samples.forEach((s, i) => (float ? view.setFloat32(at + 8 + i * 4, s, true) : view.setInt16(at + 8 + i * 2, Math.round(s * 32767), true)));
  return bytes;
}

describe('a WAV from the phone\'s engine', () => {
  it('is read as samples at its rate, 16-bit or float', () => {
    const { pcm, rate } = parseWav(wav([0, 0.5, -0.5], { rate: 22050 }));
    expect(rate).toBe(22050);
    expect([...pcm].map((s) => Math.round(s * 100) / 100)).toEqual([0, 0.5, -0.5]);
    expect([...parseWav(wav([0.25, -1], { float: true })).pcm]).toEqual([0.25, -1]);
  });

  it('skips a chunk it does not know, padding and all', () => {
    expect(parseWav(wav([0.5], { extra: true })).pcm.length).toBe(1);
  });

  it('runs a data chunk whose size was never filled in to the end of the file', () => {
    expect(parseWav(wav([0.1, 0.2, 0.3], { declared: 0 })).pcm.length).toBe(3);
    expect(parseWav(wav([0.1, 0.2, 0.3], { declared: 0xffffffff })).pcm.length).toBe(3);
  });

  it('refuses what is not a WAV, rather than reading noise', () => {
    expect(() => parseWav(new TextEncoder().encode('not audio at all').buffer)).toThrow('not a WAV file');
  });
});

describe('the engines offered', () => {
  it('are the system voice in place of the browser\'s where the phone\'s engine is bridged', () => {
    expect(offeredEngines(true, true)).toEqual(['kokoro', 'piper', 'kitten', 'recorded', 'system']);
    expect(offeredEngines(false, true)).toEqual(['recorded', 'system']);
    expect(offeredEngines(true)).toEqual(['kokoro', 'piper', 'kitten', 'recorded', 'browser']);
  });

  it('offer the system voice on Android, an iPhone and an iPad (which says it is a Mac), not on a Mac', () => {
    expect(onPhone('Mozilla/5.0 (Linux; Android 16; Pixel 10 Pro XL; wv)', 5)).toBe(true);
    expect(onPhone('Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15', 5)).toBe(true);
    expect(onPhone('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15', 5)).toBe(true);
    expect(onPhone('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15', 0)).toBe(false);
  });
});

// The reading as the Android app loads it: Tauri's bridge stubbed with a phone that has two English
// voices (its own choice the lower-quality one) and renders every line as a short tone.
class FakeBuffer {
  data: Float32Array;
  sampleRate: number;
  constructor({ length, sampleRate }: { length: number; sampleRate: number }) {
    this.data = new Float32Array(length);
    this.sampleRate = sampleRate;
  }
  get length() { return this.data.length; }
  get duration() { return this.data.length / this.sampleRate; }
  getChannelData() { return this.data; }
  copyToChannel(source: Float32Array) { this.data.set(source); }
}

describe('in the Android app', () => {
  const calls: { command: string; args?: Record<string, unknown> }[] = [];
  let reading: typeof import('../src/reading');
  let store: typeof import('../src/store');
  let piece: typeof import('../src/piece')['piece'];
  const agent = navigator.userAgent;

  beforeAll(async () => {
    vi.stubGlobal('AudioBuffer', FakeBuffer);
    vi.stubGlobal('__TAURI_INTERNALS__', {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        calls.push({ command, args });
        if (command === 'speech_voices') {
          return {
            voices: [
              { id: 'en-us-x-iob-local', lang: 'en-US', quality: 400 },
              { id: 'en-gb-x-gba-local', lang: 'en-GB', quality: 300 },
              { id: 'de-de-x-deb-local', lang: 'de-DE', quality: 400 },
            ],
            default: 'en-gb-x-gba-local',
            engine: 'com.google.android.tts',
          };
        }
        if (command === 'speech_render') return wav([...new Array(2400).fill(0), ...new Array(2400).fill(0.5)]);
        return null;
      },
    });
    Object.defineProperty(navigator, 'userAgent', { value: 'Mozilla/5.0 (Linux; Android 16; Pixel) wv', configurable: true });
    vi.resetModules();
    reading = await import('../src/reading');
    store = await import('../src/store');
    piece = (await import('../src/piece')).piece;
    const { useFixture } = await import('./site.fixture');
    useFixture();
    store.language.value = 'en';
    store.readingSettings.value = { engine: 'kokoro', pace: 'natural', level: 1, voices: {} };
  });
  afterAll(() => {
    vi.unstubAllGlobals();
    Object.defineProperty(navigator, 'userAgent', { value: agent, configurable: true });
  });

  it('stands in with the browser\'s voice only until the phone\'s engine has started', async () => {
    expect(reading.speaker()).toMatchObject({ engine: 'browser' });
    await reading.engines.system.load();
    expect(reading.speaker()).toEqual({ engine: 'system', why: 'Kokoro-82M not loaded; reading with the system voice meanwhile' });
  });

  it('reads in the phone\'s own choice of voice for the language, the better ones after it', () => {
    expect(reading.voiceOf('system')).toBe('en-gb-x-gba-local');
    expect(reading.engines.system.voices('en').map((v) => v.id)).toEqual(['en-gb-x-gba-local', 'en-us-x-iob-local']);
    expect(reading.engines.system.voices('de').map((v) => v.id)).toEqual(['de-de-x-deb-local']);
  });

  it('reads a line with no recording, when the voice that made the recordings has not loaded', () => {
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'recorded' };
    const recorded = reading.engines.recorded;
    recorded.state = 'ready';
    recorded.manifest = { voices: { 'kokoro:af_heart': { name: 'Heart', lang: 'en-US' } }, lines: {} };
    expect(reading.speakerOf('A line nobody recorded')).toEqual({
      engine: 'system', voice: 'en-gb-x-gba-local', why: 'no recording of it; read by the system voice', fallback: true,
    });
  });

  it('renders a line from the phone\'s WAV, its leading silence trimmed to a lead', async () => {
    const line = await reading.engines.system.render!('Tyger Tyger', { voice: 'en-gb-x-gba-local', speed: 1.1 });
    expect(calls.at(-1)).toEqual({ command: 'speech_render', args: { text: 'Tyger Tyger', voice: 'en-gb-x-gba-local', speed: 1.1 } });
    expect(line.sampleRate).toBe(24000);
    expect(line.lead).toBeCloseTo(0.01, 5);   // the 10 ms margin kept before the speech
    expect(line.length).toBe(240 + 2400);
  });

  it('is let go while the page is hidden and silent, and started again by the next Play', () => {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    reading.letVoicesGo();
    expect(reading.engines.system.state).toBe('idle');
    expect(calls.at(-1)).toEqual({ command: 'speech_release', args: undefined });
    reading.ensureEngine();
    expect(calls.at(-1)?.command).toBe('speech_voices');
    vi.restoreAllMocks();
  });

  it('reads with the phone\'s own voice by default', () => {
    expect(reading.READING_DEFAULTS.engine).toBe('system');
    expect(reading.readSaved(null, null).engine).toBe('system');
    expect(reading.readSaved({ engine: 'browser' }, null).engine).toBe('system');
  });

  it('hands a line to the phone\'s voice until the chosen voice has rendered it, then takes it back', async () => {
    await reading.engines.system.load();
    const kokoro = reading.engines.kokoro;
    kokoro.state = 'ready';
    kokoro.listed = [{ id: 'af_heart', name: 'Heart', lang: 'en-us' }];
    // Kokoro's line takes as long as the test says: the phone's must not wait behind it.
    let finish!: () => void;
    const slow = new Promise<void>((resolve) => { finish = resolve; });
    const real = kokoro.render;
    kokoro.render = async () => {
      await slow;
      return new FakeBuffer({ length: 24000, sampleRate: 24000 }) as unknown as import('../src/reading/engines').Rendered;
    };
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'kokoro', pace: 'natural' };
    store.hearing.value = 'A';
    const poem = { lines: [{ line: 0, text: 'Tyger Tyger', pace: 1 }], cues: [] } as unknown as import('../src/answers').Reading;
    piece.readings = { A: poem, B: null, poem: '' };
    reading.prerender();
    await vi.waitFor(() => expect(reading.castOf(poem, 'natural').get(0)).toBe('system|en-gb-x-gba-local|1.000|Tyger Tyger'));
    finish();
    await vi.waitFor(() => expect(reading.castOf(poem, 'natural').get(0)).toBe('kokoro|af_heart|1.000|Tyger Tyger'));
    kokoro.render = real;
    kokoro.state = 'idle';
  });
});
