// ---- Engines ----
//
// Who speaks is an engine, the listener's choice (a page setting, not a style key — the CLI never
// reads aloud): `browser`, the speech synthesiser, instant and the fallback for everything else;
// and three neural voices, each in a worker of its own (public/speech-worker.mjs), downloaded only
// once chosen: `kokoro`, `piper` (many languages, Slovenian among them) and `kitten`. Every engine
// answers the same three calls: `load(onProgress)`, `voices(language)` and `render(text, { voice,
// speed })`, which gives an AudioBuffer. A fifth, `recorded`, answers the same calls from files
// made ahead of time (scripts/readings/): its `render` fetches and decodes a line's recording.
// In the phone apps, `system` is the phone's own speech engine, which renders a line as the neural
// voices do with nothing to download; it stands in where the browser's voice would, since the
// Android app's WebView has no speech synthesis and iOS's cannot be placed on the beat.
import type { Reading } from '../answers';
import { inApp, invoke } from '../player/native';
import type { EngineName } from '../store';

export interface Voice {
  id: string;
  name: string;
  lang: string;
  have?: number;
  /** A phone voice good enough not to suggest a better one (Enhanced or Premium on iOS). */
  good?: boolean;
}

export interface Progress {
  text: string;
  fraction?: number | null;
}

/** A rendered line: its audio, how long it took, and the quiet margin before its speech. */
export type Rendered = AudioBuffer & { took?: number; lead?: number };

export interface Engine {
  name: EngineName;
  state: 'idle' | 'loading' | 'ready' | 'failed';
  device: string | null;
  error: string | null;
  progress: string;
  listed?: Voice[] | null;
  loading?: Promise<void> | null;
  cancelling?: boolean;
  load(onProgress?: (progress: Progress) => void): Promise<void>;
  voices(lang: string): Voice[];
  /** Null for the browser's voice, which is spoken, not made. */
  render: ((text: string, options: { voice: string; speed: number }) => Promise<Rendered>) | null;
  cancel?(): void;
  /** Where the reader can get a better voice than the ones listed, as the phone says it. */
  improve?: string | null;
}

// What each neural engine offers before it has loaded — its default voice per language — and,
// once it has, everything it lists. Kokoro and Kitten speak English only; Piper reads its
// catalogue (rhasspy/piper-voices) and offers every language there.
export const ENGINE_INFO: Record<EngineName, { label: string; defaults: Record<string, string> }> = {
  browser: { label: 'browser voice', defaults: {} },
  system: { label: 'system voice', defaults: {} },
  kokoro: { label: 'Kokoro-82M', defaults: { en: 'af_heart' } },
  piper: { label: 'Piper', defaults: { en: 'en_GB-cori-medium', sl: 'sl_SI-artur-medium' } },
  kitten: { label: 'Kitten TTS (nano)', defaults: { en: 'Bella' } },
  // The recorded readings' voices are named `engine:voice`, after the live voice that made them.
  recorded: { label: 'recorded readings', defaults: { en: 'kokoro:af_heart', sl: 'piper:sl_SI-artur-medium' } },
};
export const ENGINE_NAMES: EngineName[] = ['kokoro', 'piper', 'kitten', 'recorded', 'system', 'browser'];

// The neural engines carry eSpeak NG (GPL-3) inside them — Kokoro's phonemiser, piper-wasm, Kitten's
// — and run code fetched at run time, neither of which an App Store app may ship. A build made with
// `PROSODY_NEURAL_VOICES=off` (scripts/build-site.sh passes it to Vite; `mise run site:ios` sets it
// for the iOS app) leaves speech-worker.mjs out and offers only the recorded readings and the
// browser's voice. Every other build offers all five, as before.
export const NEURAL_ENGINES: readonly EngineName[] = ['kokoro', 'piper', 'kitten'];
export const NEURAL_VOICES = import.meta.env.VITE_PROSODY_NEURAL_VOICES !== 'off';

// The phone's own speech engine is bridged on Android and iOS (src-tauri/src/speech.rs), not on a
// desktop; where it is, it replaces the browser's voice. An iPad's WebView says it is a Mac, and
// is told apart by its touch screen.
export const SYSTEM_VOICE = inApp() && onPhone(navigator.userAgent, navigator.maxTouchPoints);
export function onPhone(agent: string, touches: number): boolean {
  return /Android|iPhone|iPad|iPod/.test(agent) || (/Macintosh/.test(agent) && touches > 1);
}

/** The engines a build offers: all of them, or, without the neural voices, the recordings and the
 * stand-in voice — the browser's, or in the phone apps the system's. */
export function offeredEngines(neural: boolean, system = false): EngineName[] {
  return ENGINE_NAMES.filter((name) => (neural || !NEURAL_ENGINES.includes(name))
    && name !== (system ? 'browser' : 'system'));
}
export const OFFERED_ENGINES = offeredEngines(NEURAL_VOICES, SYSTEM_VOICE);

export const sameLanguage = (lang: string | undefined, wanted: string) =>
  (lang || '').toLowerCase().replace('_', '-').split('-')[0] === wanted;

/** The browser's speech voices, as it lists them. */
export let browserVoices: SpeechSynthesisVoice[] = [];
export function listBrowserVoices(): void {
  if ('speechSynthesis' in window) browserVoices = speechSynthesis.getVoices();
}

const browser: Engine = {
  name: 'browser',
  state: 'ready',
  device: null,
  error: null,
  progress: '',
  load: async () => {},
  voices: (lang) => browserVoices.filter((v) => sameLanguage(v.lang, lang)).map((v) => ({ id: v.name, name: `${v.name} (${v.lang})`, lang: v.lang })),
  // The browser's voice cannot be rendered ahead: it is spoken, not made.
  render: null,
};

// ---- The phone's own voice ----

/** A WAV file's first channel as samples, and its rate: 16-bit, 8-bit or 32-bit float PCM, as an
 * Android speech engine writes it, and as SpeechPlugin.swift does (float). */
export function parseWav(bytes: ArrayBuffer): { pcm: Float32Array; rate: number } {
  const view = new DataView(bytes);
  const tag = (at: number) => String.fromCharCode(view.getUint8(at), view.getUint8(at + 1), view.getUint8(at + 2), view.getUint8(at + 3));
  if (bytes.byteLength < 12 || tag(0) !== 'RIFF' || tag(8) !== 'WAVE') throw new Error('not a WAV file');
  let format = 0, channels = 1, rate = 0, bits = 0;
  for (let at = 12; at + 8 <= bytes.byteLength;) {
    const id = tag(at);
    // An engine still writing gives the data chunk's size as 0 or past the end: it runs to the end.
    const declared = view.getUint32(at + 4, true);
    const size = id === 'data' && (declared === 0 || at + 8 + declared > bytes.byteLength) ? bytes.byteLength - at - 8 : declared;
    if (id === 'fmt ') {
      format = view.getUint16(at + 8, true);
      channels = view.getUint16(at + 10, true);
      rate = view.getUint32(at + 12, true);
      bits = view.getUint16(at + 22, true);
    } else if (id === 'data') {
      // Android's FileSynthesisCallback labels float audio as PCM (format 1, 32 bits): any 32-bit
      // file is read as float. A pair not read here is refused, never decoded as silence.
      const float = bits === 32 && (format === 3 || format === 1);
      if (!rate || !(float || (format === 1 && (bits === 16 || bits === 8)))) throw new Error(`a WAV the reading cannot read (format ${format}, ${bits} bits)`);
      const width = bits / 8;
      const frames = width && channels ? Math.floor(size / (width * channels)) : 0;
      const pcm = new Float32Array(frames);
      for (let i = 0; i < frames; i += 1) {
        const p = at + 8 + i * width * channels;
        pcm[i] = float ? view.getFloat32(p, true)
          : bits === 16 ? view.getInt16(p, true) / 32768
            : (view.getUint8(p) - 128) / 128;
      }
      return { pcm, rate };
    }
    at += 8 + size + (size % 2);   // chunks are padded to an even length
  }
  throw new Error('a WAV file with no audio');
}

interface SystemVoice {
  id: string;
  lang: string;
  /** Its own name, where the phone gives one (iOS: Ava, Daniel); Android gives only the id. */
  name?: string | null;
  /** What the menu says of its quality (Premium, Enhanced, Eloquence), if anything. */
  tier?: string | null;
  rank: number;
  good?: boolean;
}
interface SystemVoices {
  voices: SystemVoice[];
  default: string | null;
  engine: string | null;
  improve?: string | null;
}

// A language tag as a reader says it (en-GB: British English), falling back to the tag itself.
let languageNames: Intl.DisplayNames | null | undefined;
function languageName(tag: string): string {
  if (languageNames === undefined) {
    try { languageNames = new Intl.DisplayNames(['en'], { type: 'language' }); } catch { languageNames = null; }
  }
  try { return languageNames?.of(tag) ?? tag; } catch { return tag; }
}
/** What the voice menu calls a phone voice: its name, its language and how good it is (iOS:
 * "Ava (American English, Premium)"); Android's, which have only ids, by language and the id's
 * whole variant ("American English, iob"; "American English, sfg female 1" for
 * en-us-x-sfg#female_1-local, so its male twin is told apart). */
export function systemVoiceLabel(voice: SystemVoice): string {
  const language = languageName(voice.lang);
  if (voice.name) return `${voice.name} (${language}${voice.tier ? ', ' + voice.tier : ''})`;
  const variant = /-x-(.+?)(?:-local|-network)?$/i.exec(voice.id)?.[1]?.replace(/[#_]/g, ' ')
    ?? (voice.id.endsWith('-language') ? 'default' : voice.id);
  return `${language}, ${variant}`;
}

const system: Engine = {
  name: 'system',
  state: 'idle',
  device: null,
  error: null,
  progress: '',
  listed: null,
  // Starting the phone's engine and asking for its voices: a fraction of a second, nothing fetched.
  load() {
    if (system.state === 'ready') return Promise.resolve();
    if (system.loading) return system.loading;
    system.state = 'loading';
    system.error = null;
    system.loading = (async () => {
      try {
        const answer = await invoke('speech_voices') as SystemVoices;
        // The phone's own choice first, then the better voices: voiceOf takes the first offered.
        const rank = (v: SystemVoice) => (v.id === answer.default ? 1000 : 0) + v.rank;
        system.listed = [...answer.voices].sort((a, b) => rank(b) - rank(a) || systemVoiceLabel(a).localeCompare(systemVoiceLabel(b)))
          .map((v) => ({ id: v.id, name: systemVoiceLabel(v), lang: v.lang, good: v.good ?? true }));
        system.improve = answer.improve ?? null;
        system.device = answer.engine;
        system.state = 'ready';
      } catch (error) {
        system.state = 'failed';
        system.error = String((error as Error).message ?? error);
        throw error;
      } finally {
        system.loading = null;
      }
    })();
    return system.loading;
  },
  // Let go while the page is hidden and silent; the next Play starts it again (ensureEngine).
  cancel() {
    if (system.state !== 'ready') return;
    system.state = 'idle';
    invoke('speech_release').catch(() => {});
  },
  voices: (lang) => (system.listed ?? []).filter((v) => sameLanguage(v.lang, lang)),
  async render(text, { voice, speed }) {
    const began = performance.now();
    const bytes = await invoke('speech_render', { text, voice, speed }) as ArrayBuffer;
    const { pcm, rate } = parseWav(bytes);
    const buffer = new AudioBuffer({ numberOfChannels: 1, length: Math.max(1, pcm.length), sampleRate: rate });
    buffer.copyToChannel(pcm as Float32Array<ArrayBuffer>, 0);
    const kept = trimmedBuffer(buffer);
    kept.took = performance.now() - began;
    return kept;
  },
};

/** Called when an engine that had loaded fails (its worker died). Set by the reading. */
export const failures = { onFailed: (_engine: Engine, _error: Error) => {} };

interface WorkerAnswer {
  id: number;
  ok?: boolean;
  error?: string;
  progress?: Progress;
  device?: string;
  voices?: Voice[];
  pcm?: Float32Array;
  rate?: number;
  took?: number;
  lead?: number;
}

function workerEngine(name: EngineName): Engine {
  let worker: Worker | null = null;
  let asked = 0;
  const waiting = new Map<number, { resolve: (answer: WorkerAnswer) => void; reject: (error: Error) => void }>();
  const ask = (message: Record<string, unknown>) => new Promise<WorkerAnswer>((resolve, reject) => {
    if (!worker) { reject(new Error('the engine is not loaded')); return; }
    asked += 1;
    waiting.set(asked, { resolve, reject });
    worker.postMessage({ id: asked, ...message });
  });
  const crashed = (error: Error) => {
    engine.state = 'failed';
    engine.error = error.message;
    for (const pending of waiting.values()) pending.reject(error);
    waiting.clear();
    worker?.terminate();
    worker = null;
    failures.onFailed(engine, error);
  };
  const engine: Engine = {
    name,
    state: 'idle',     // idle → loading → ready, or failed (and then idle again on a retry)
    device: null,
    listed: null,      // the voices it lists once loaded
    error: null,
    progress: '',
    load(onProgress) {
      if (engine.state === 'ready') return Promise.resolve();
      if (engine.loading) return engine.loading;
      engine.state = 'loading';
      engine.error = null;
      engine.loading = (async () => {
        try {
          // `?device=wasm` on the page keeps every engine off WebGPU (see speech-worker.mjs).
          const device = new URLSearchParams(location.search).get('device') === 'wasm' ? '&device=wasm' : '';
          worker ??= new Worker(new URL(`./speech-worker.mjs?engine=${name}${device}`, document.baseURI), { type: 'module' });
          worker.onmessage = ({ data }: MessageEvent<WorkerAnswer>) => {
            if (data.progress) { engine.progress = data.progress.text; onProgress?.(data.progress); return; }
            const pending = waiting.get(data.id);
            waiting.delete(data.id);
            if (!pending) return;
            if (data.ok) pending.resolve(data); else pending.reject(new Error(data.error));
          };
          worker.onerror = (event) => {
            const error = new Error(event.message || 'the worker failed');
            for (const pending of waiting.values()) pending.reject(error);
            waiting.clear();
            // A worker that dies after it has loaded would leave every later request posted to
            // nothing: the engine is failed instead, so the browser voice reads and it is said.
            if (engine.state === 'ready') crashed(error);
          };
          const info = await ask({ op: 'load' });
          engine.device = info.device ?? null;
          engine.listed = info.voices ?? [];
          engine.state = 'ready';
        } catch (error) {
          // Cancelled by the listener (cancel(), below): not a failure, just not loaded.
          engine.state = engine.cancelling ? 'idle' : 'failed';
          engine.error = engine.cancelling ? null : (error as Error).message;
          engine.cancelling = false;
          // A worker that failed to load is let go, so choosing the engine again starts afresh.
          worker?.terminate();
          worker = null;
          throw error;
        } finally {
          engine.loading = null;
        }
      })();
      return engine.loading;
    },
    // Stops a download the listener no longer wants: the worker is let go at once, so the network
    // and the CPU are freed, and every request waiting on it is told so. The engine is idle again,
    // to be chosen afresh.
    cancel() {
      engine.cancelling = engine.state === 'loading';
      const error = Object.assign(new Error('cancelled'), { cancelled: true });
      for (const pending of waiting.values()) pending.reject(error);
      waiting.clear();
      worker?.terminate();
      worker = null;
      // A loaded engine let go keeps its list of voices, which no reload changes: the menu goes on
      // showing the voice chosen, not the engine's default (voiceOf finds only a voice listed).
      if (!engine.cancelling) engine.state = 'idle';
    },
    voices(lang) {
      const listed = engine.listed || Object.entries(ENGINE_INFO[name].defaults)
        .map(([code, id]) => ({ id, name: id, lang: code }));
      return listed.filter((v) => sameLanguage(v.lang, lang));
    },
    async render(text, { voice, speed }) {
      const answer = await ask({ op: 'render', text, voice, speed });
      const pcm = answer.pcm ?? new Float32Array(0);
      const buffer: Rendered = new AudioBuffer({ numberOfChannels: 1, length: Math.max(1, pcm.length), sampleRate: answer.rate ?? 24000 });
      buffer.copyToChannel(pcm as Float32Array<ArrayBuffer>, 0);
      buffer.took = answer.took;
      buffer.lead = answer.lead || 0;   // the quiet margin before the speech: started that much early
      return buffer;
    },
  };
  return engine;
}

// ---- The recorded readings ----
//
// SPEC.md §4.7, `recorded`. Every line of every example poem, spoken ahead of time on the
// maintainer's machine by the page's own engines (scripts/readings/), at the line's natural pace
// — baked in, so the file plays as it is — one MP3 a line and voice, beside the page in
// `readings/`, with a manifest. The manifest is keyed by voice and by a hash of the line's exact
// text, and carries the text: a line is looked up by its text, so an edited line simply has none.
// A voice is named `engine:voice` after the live voice that made it; where a line has no
// recording, that live voice reads it if its engine is already loaded, and otherwise the browser's
// voice does — never a model downloaded unasked — and the status bar says which (speakerOf).
// Loading is fetching the manifest (a few tens of kB); a line's file is fetched when it is
// rendered ahead, as any engine's line is.
export const READINGS = './readings/';

interface Manifest {
  voices: Record<string, { name: string; lang: string }>;
  lines: Record<string, { text: string; pace: number; recordings: Record<string, { file: string; seconds: number }> }>;
}
interface RecordedLine {
  key: string;
  text: string;
  pace: number;
  recordings: Record<string, { file: string; seconds: number }>;
}

export interface RecordedEngine extends Engine {
  manifest: Manifest | null;
  byText: Map<string, RecordedLine>;
  has(voice: string, text: string): boolean;
  /** The lines of the reading heard, for counting what each voice has recorded. */
  heard: () => Reading | null;
  /** The voice saved for a language, for listing before the manifest is here. */
  saved: (lang: string) => string | undefined;
}

const recorded: RecordedEngine = {
  name: 'recorded',
  state: 'idle',
  device: 'files',
  error: null,
  progress: '',
  manifest: null,
  byText: new Map(),
  heard: () => null,
  saved: () => undefined,
  load() {
    const engine = recorded;
    if (engine.state === 'ready') return Promise.resolve();
    if (engine.loading) return engine.loading;
    engine.state = 'loading';
    engine.error = null;
    engine.loading = (async () => {
      try {
        const response = await fetch(new URL(READINGS + 'manifest.json', document.baseURI));
        if (!response.ok) throw new Error(`no recordings here (${response.status} for readings/manifest.json)`);
        const manifest = (await response.json()) as Manifest;
        engine.manifest = manifest;
        engine.byText = new Map(Object.entries(manifest.lines).map(([key, line]) => [line.text, { key, ...line }]));
        engine.state = 'ready';
        console.log(`[reading] recorded: ${engine.byText.size} lines in ${Object.keys(manifest.voices).length} voices`);
      } catch (error) {
        engine.state = 'failed';
        engine.error = (error as Error).message;
        throw error;
      } finally {
        engine.loading = null;
      }
    })();
    return engine.loading;
  },
  // The recorded voices of `lang` that have recordings of the poem heard, most lines first, each
  // saying how many of its lines it has; every voice of the language if none has any (a poem of
  // the writer's own), so the menu is never empty and every line is read live, and said.
  voices(lang) {
    const engine = recorded;
    // Before the manifest is here: the voice saved for the language, else the default, by id.
    if (!engine.manifest) {
      const id = engine.saved(lang) ?? ENGINE_INFO.recorded.defaults[lang];
      return id ? [{ id, name: id, lang }] : [];
    }
    const texts = recordedTexts(engine.heard());
    const listed = Object.entries(engine.manifest.voices)
      .filter(([, v]) => sameLanguage(v.lang, lang))
      .map(([id, v]) => {
        const have = texts.filter((text) => engine.byText.get(text)?.recordings[id]).length;
        // A side whose reading is off reads nothing: said so, not as 0 of 0 recorded.
        const count = texts.length ? `${have} of ${texts.length} lines recorded` : 'this side reads nothing';
        return { id, name: `${v.name} (${count})`, lang: v.lang, have };
      });
    const some = listed.filter((v) => v.have > 0).sort((a, b) => b.have - a.have);
    return some.length ? some : listed;
  },
  has(voice, text) {
    return !!recorded.byText.get(text)?.recordings[voice];
  },
  // The recording of `text` in `voice`, decoded and trimmed of its silence (an MP3 adds an
  // encoder's delay before it), as an AudioBuffer. At the speed it was recorded at — the line's
  // pace — it is the file as it is; asked for another (the fitted pace's refit, or a pace rule
  // changed since the recording), it is resampled to it, which shifts its pitch as well.
  async render(text, { voice, speed }) {
    const began = performance.now();
    const line = recorded.byText.get(text);
    const recording = line?.recordings[voice];
    if (!line || !recording) throw new Error(`no recording of this line in ${voice}`);
    const response = await fetch(new URL(READINGS + recording.file, document.baseURI));
    if (!response.ok) throw new Error(`${response.status} for readings/${recording.file}`);
    let buffer: AudioBuffer = await decoder().decodeAudioData(await response.arrayBuffer());
    const rate = speed / line.pace;
    if (Math.abs(rate - 1) > 0.0005) buffer = await resampled(buffer, rate);
    const kept = trimmedBuffer(buffer);
    kept.took = performance.now() - began;
    return kept;
  },
};

export const engines: Record<EngineName, Engine> & { recorded: RecordedEngine } = {
  browser,
  system,
  kokoro: workerEngine('kokoro'),
  piper: workerEngine('piper'),
  kitten: workerEngine('kitten'),
  recorded,
};

// What decodes a recording: a context of the page's own, so a line can be fetched ahead before the
// player has made its audio context. The player resamples a line to the output's rate itself.
let decoding: OfflineAudioContext | null = null;
function decoder(): OfflineAudioContext {
  decoding ??= new OfflineAudioContext(1, 1, 48000);
  return decoding;
}

/** The lines a reading may speak, by text. */
export function recordedTexts(reading: Reading | null): string[] {
  if (!reading) return [];
  return [...new Set([...reading.lines, ...reading.cues].map((line) => line.text).filter((text) => text.trim()))];
}

/** `buffer` played `rate` times as fast, as a new buffer. */
async function resampled(buffer: AudioBuffer, rate: number): Promise<AudioBuffer> {
  const offline = new OfflineAudioContext(1, Math.max(1, Math.ceil(buffer.length / rate)), buffer.sampleRate);
  const source = new AudioBufferSourceNode(offline, { buffer, playbackRate: rate });
  source.connect(offline.destination);
  source.start();
  return offline.startRendering();
}

// A decoded recording without the silence before and after its speech, keeping 10 ms either side
// and saying how long the margin before is (`lead`), as speech-worker.mjs does for a rendered line:
// the page starts it that much early, so the speech itself begins on the time.
const SILENCE = 10 ** (-50 / 20);
/** Where the speech in `pcm` begins and ends, with a margin either side, in samples. */
export function speechSpan(pcm: Float32Array, sampleRate: number): { from: number; to: number; lead: number } {
  let first = 0;
  while (first < pcm.length && Math.abs(pcm[first] ?? 0) < SILENCE) first += 1;
  let last = pcm.length - 1;
  while (last > first && Math.abs(pcm[last] ?? 0) < SILENCE) last -= 1;
  const margin = Math.round(0.01 * sampleRate);
  const silent = first >= pcm.length;
  const from = silent ? 0 : Math.max(0, first - margin);
  const to = silent ? 1 : Math.min(pcm.length, last + 1 + margin);
  return { from, to, lead: silent ? 0 : (first - from) / sampleRate };
}
function trimmedBuffer(buffer: AudioBuffer): Rendered {
  const pcm = buffer.getChannelData(0);
  const { from, to, lead } = speechSpan(pcm, buffer.sampleRate);
  const kept: Rendered = new AudioBuffer({ numberOfChannels: 1, length: Math.max(1, to - from), sampleRate: buffer.sampleRate });
  kept.copyToChannel(pcm.subarray(from, to), 0);
  kept.lead = lead;
  return kept;
}
