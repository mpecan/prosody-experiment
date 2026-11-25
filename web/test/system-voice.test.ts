// The phone's own voice in the Android app: offered in place of the browser's (which the app's
// WebView has none of), standing in for a voice that cannot read, and rendering a line from the
// WAV file the phone's engine writes.
import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';

import { offeredEngines, onPhone, parseWav, systemVoiceLabel } from '../src/reading/engines';

/** A mono WAV of `samples`: 16-bit PCM, or 32-bit float; `declared` overrides the data chunk's size,
 * `format` the format its header says (1 PCM, 3 float). */
function wav(samples: number[], { rate = 24000, float = false, declared, extra = false, format }: { rate?: number; float?: boolean; declared?: number; extra?: boolean; format?: number } = {}): ArrayBuffer {
  const width = float ? 4 : 2;
  const pad = extra ? 8 + 3 + 1 : 0;   // an odd-sized chunk before the data, padded to even
  const bytes = new ArrayBuffer(12 + 24 + pad + 8 + samples.length * width);
  const view = new DataView(bytes);
  const tag = (at: number, text: string) => [...text].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
  tag(0, 'RIFF'); view.setUint32(4, bytes.byteLength - 8, true); tag(8, 'WAVE');
  tag(12, 'fmt '); view.setUint32(16, 16, true);
  view.setUint16(20, format ?? (float ? 3 : 1), true); view.setUint16(22, 1, true);
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

  it('reads 32-bit audio as float though the header says PCM, as Android\'s writes it', () => {
    expect([...parseWav(wav([0, 0.5, -0.5], { float: true, format: 1 })).pcm]).toEqual([0, 0.5, -0.5]);
  });

  it('refuses a format it does not read, rather than reading silence', () => {
    expect(() => parseWav(wav([0.5], { format: 3 }))).toThrow('a WAV the reading cannot read (format 3, 16 bits)');
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

describe('a phone voice in the menu', () => {
  it('is named, with its language and how good it is, never by its raw id', () => {
    expect(systemVoiceLabel({ id: 'com.apple.voice.premium.en-US.Ava', lang: 'en-US', name: 'Ava', tier: 'Premium', rank: 40 })).toBe('Ava (American English, Premium)');
    expect(systemVoiceLabel({ id: 'com.apple.voice.compact.en-GB.Daniel', lang: 'en-GB', name: 'Daniel', tier: null, rank: 20 })).toBe('Daniel (British English)');
    expect(systemVoiceLabel({ id: 'en-us-x-iob-local', lang: 'en-US', rank: 400 })).toBe('American English, iob');
    expect(systemVoiceLabel({ id: 'en-US-language', lang: 'en-US', rank: 400 })).toBe('American English, default');
  });

  it('tells apart Android voices whose ids differ only past the variant', () => {
    expect(systemVoiceLabel({ id: 'en-us-x-sfg#female_1-local', lang: 'en-US', rank: 400 })).toBe('American English, sfg female 1');
    expect(systemVoiceLabel({ id: 'en-us-x-sfg#male_1-local', lang: 'en-US', rank: 400 })).toBe('American English, sfg male 1');
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
  // A test's own answer to a command, in place of the phone's below (undefined: the phone answers).
  let answering: ((command: string) => unknown) | null = null;
  let reading: typeof import('../src/reading');
  let store: typeof import('../src/store');
  let piece: typeof import('../src/piece')['piece'];
  const agent = navigator.userAgent;
  // When the phone answers a release: at once, unless a test holds it back.
  let release: Promise<void> = Promise.resolve();
  // What the phone answers speech_voices with: Android's shape, until a test hands it an iPhone's.
  let phone: Record<string, unknown> = {
    voices: [
      { id: 'en-us-x-iob-local', lang: 'en-US', rank: 400, good: true },
      { id: 'en-gb-x-gba-local', lang: 'en-GB', rank: 300, good: false },
      { id: 'de-de-x-deb-local', lang: 'de-DE', rank: 400, good: true },
    ],
    default: 'en-gb-x-gba-local',
    engine: 'com.google.android.tts',
  };

  beforeAll(async () => {
    vi.stubGlobal('AudioBuffer', FakeBuffer);
    vi.stubGlobal('__TAURI_INTERNALS__', {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        calls.push({ command, args });
        const own = answering?.(command);
        if (own !== undefined) return own;
        if (command === 'speech_voices') return phone;
        if (command === 'speech_release') await release;
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

  it('is let go while the page is hidden and silent, and started again by the next Play — once the release has landed', async () => {
    let answer!: () => void;
    release = new Promise((resolve) => { answer = resolve; });
    const hidden = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    reading.letVoicesGo();
    expect(reading.engines.system.state).toBe('idle');
    expect(calls.at(-1)).toEqual({ command: 'speech_release', args: undefined });
    hidden.mockReturnValue('visible');   // shown again, and Play pressed
    reading.ensureEngine();
    // The phone has not answered the release: asking for the voices now could land before it.
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(calls.at(-1)?.command).toBe('speech_release');
    answer();
    await vi.waitFor(() => expect(calls.at(-1)?.command).toBe('speech_voices'));
    await vi.waitFor(() => expect(reading.engines.system.state).toBe('ready'));
    release = Promise.resolve();
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

  it('says where to get a better voice while the phone has only basic ones, and lists one got meanwhile', async () => {
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'system' };
    // Android says nothing of where to get one, but names the better voice it has.
    expect(reading.voiceHint()).toBe('American English, iob is on this phone: choose it under Voice for a better reading.');
    const IMPROVE = 'For a better voice, download an Enhanced or Premium one in Settings.';
    phone = {
      voices: [
        { id: 'com.apple.voice.super-compact.en-US.Samantha', lang: 'en-US', name: 'Samantha', tier: null, rank: 10, good: false },
        { id: 'com.apple.eloquence.en-US.Reed', lang: 'en-US', name: 'Reed', tier: 'Eloquence', rank: 0, good: false },
      ],
      default: 'com.apple.voice.super-compact.en-US.Samantha',
      engine: 'AVSpeechSynthesizer',
      improve: IMPROVE,
    };
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    reading.relistSystem();
    await vi.waitFor(() => expect(reading.engines.system.state).toBe('ready'));
    expect(reading.engines.system.voices('en').map((v) => v.name)).toEqual(['Samantha (American English)', 'Reed (American English, Eloquence)']);
    expect(reading.voiceHint()).toBe(IMPROVE);
    // Back from Settings with a Premium voice: it is listed, first, and the hint is gone.
    phone = {
      ...phone,
      voices: [...(phone.voices as object[]), { id: 'com.apple.voice.premium.en-US.Ava', lang: 'en-US', name: 'Ava', tier: 'Premium', rank: 40, good: true }],
      default: 'com.apple.voice.premium.en-US.Ava',
    };
    reading.relistSystem();
    await vi.waitFor(() => expect(reading.engines.system.voices('en')).toHaveLength(3));
    expect(reading.voiceOf('system')).toBe('com.apple.voice.premium.en-US.Ava');
    expect(reading.voiceHint()).toBeNull();
    // A reader who chose Samantha keeps her, and is told of Ava.
    store.readingSettings.value = { ...store.readingSettings.value, voices: { system: { en: 'com.apple.voice.super-compact.en-US.Samantha' } } };
    expect(reading.voiceOf('system')).toBe('com.apple.voice.super-compact.en-US.Samantha');
    expect(reading.voiceHint()).toBe('Ava (American English, Premium) is on this phone: choose it under Voice for a better reading.');
    store.readingSettings.value = { ...store.readingSettings.value, voices: {} };
    vi.restoreAllMocks();
  });

  it('gives no hint while the phone\'s voice only stands in: the Voice menu lists another engine\'s', () => {
    // The recordings chosen, the phone's voice standing in in a basic voice.
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'recorded', voices: { system: { en: 'com.apple.voice.super-compact.en-US.Samantha' } } };
    expect(reading.standIn()).toBe('system');
    expect(reading.voiceHint()).toBeNull();
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'system', voices: {} };
  });

  it('keeps a released neural voice\'s list, so the menu shows the voice chosen', () => {
    const kokoro = reading.engines.kokoro;
    kokoro.state = 'ready';
    kokoro.listed = [{ id: 'af_heart', name: 'Heart', lang: 'en-us' }, { id: 'am_adam', name: 'Adam', lang: 'en-us' }];
    store.readingSettings.value = { ...store.readingSettings.value, voices: { kokoro: { en: 'am_adam' } } };
    kokoro.cancel!();
    expect(kokoro.state).toBe('idle');
    expect(reading.voiceOf('kokoro')).toBe('am_adam');
    kokoro.listed = null;
    store.readingSettings.value = { ...store.readingSettings.value, voices: {} };
  });

  it('says what came of starting the phone\'s voice, when it is the voice chosen', async () => {
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'system' };
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    answering = (command) => (command === 'speech_voices' ? Promise.reject(new Error('no engine')) : undefined);
    reading.relistSystem();
    await vi.waitFor(() => expect(reading.engines.system.state).toBe('failed'));
    // The WebView has no voice of its own: nothing reads, and that is said.
    await vi.waitFor(() => expect(store.readingNote.value).toBe('system voice could not load (no engine); nothing reads it (this app has no browser voice)'));
    expect(store.readingNoteBad.value).toBe(true);
    answering = null;
    reading.relistSystem();
    await vi.waitFor(() => expect(reading.engines.system.state).toBe('ready'));
    await vi.waitFor(() => expect(store.readingNote.value).toBe(''));
    vi.restoreAllMocks();
  });

  it('names the phone\'s voice where it stands in for a voice that failed', async () => {
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'kokoro' };
    // The reading's own copy of the module (vi.resetModules above), whose handler it set.
    const { failures } = await import('../src/reading/engines');
    failures.onFailed(reading.engines.kokoro, new Error('the worker failed'));
    expect(store.readingNote.value).toBe('Kokoro-82M failed: the worker failed — reading with the system voice; choose it again to retry');
    store.readingSettings.value = { ...store.readingSettings.value, engine: 'system' };
  });

  it('keeps the voices for a Play still loading while the page is hidden', () => {
    expect(reading.engines.system.state).toBe('ready');
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    reading.speech.starting = true;
    reading.letVoicesGo();
    expect(reading.engines.system.state).toBe('ready');
    reading.speech.starting = false;
    reading.letVoicesGo();
    expect(reading.engines.system.state).toBe('idle');
    vi.restoreAllMocks();
  });

  it('lets go a phone voice that finished starting while the page was hidden and silent', async () => {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    calls.length = 0;
    reading.ensureEngine();
    await vi.waitFor(() => expect(calls.map((c) => c.command)).toEqual(['speech_voices', 'speech_release']));
    expect(reading.engines.system.state).toBe('idle');
    vi.restoreAllMocks();
  });

  it('lets go a voice rendering a line only once the line is done, and drops what was queued quietly', async () => {
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
    await reading.engines.system.load();
    let finish!: () => void;
    const slow = new Promise<void>((resolve) => { finish = resolve; });
    answering = (command) => (command === 'speech_render'
      ? slow.then(() => wav([...new Array(2400).fill(0), ...new Array(2400).fill(0.5)]))
      : undefined);
    const lines = [{ line: 0, text: 'Never seen before', pace: 1 }, { line: 1, text: 'Nor this one either', pace: 1 }];
    const poem = { lines, cues: [] } as unknown as import('../src/answers').Reading;
    piece.readings = { A: poem, B: null, poem: '' };
    store.hearing.value = 'A';
    reading.prerender();
    // The second line is due now: a failure to render it would be said, in red.
    reading.renderUrgently(poem, { ...lines[1], stanza: 0 } as unknown as import('../src/answers').SpokenLine, 'natural');
    reading.noteReading('');
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden');
    reading.letVoicesGo();
    expect(reading.engines.system.state).toBe('ready');
    finish();
    await vi.waitFor(() => expect(reading.engines.system.state).toBe('idle'));
    expect(calls.at(-1)?.command).toBe('speech_release');
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(store.readingNoteBad.value).toBe(false);
    expect(store.readingNote.value).toBe('');
    answering = null;
    piece.readings = { A: null, B: null, poem: '' };
    vi.restoreAllMocks();
  });
});
