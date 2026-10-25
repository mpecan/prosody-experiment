// The reading's neural voices (SPEC.md §4.7), each in a module worker of its own.
//
// The page makes one worker per engine — `new Worker('./speech-worker.mjs?engine=kokoro',
// { type: 'module' })` — and asks it, one message at a time, to load and to render a line. Why a
// worker: a neural voice computes for a second or more per line, and the page's own thread is the
// one Strudel's scheduler ticks on, so rendering there would skip music. Why one each: Kokoro
// brings its own ONNX runtime inside Transformers.js and Piper uses onnxruntime-web directly, and
// two runtimes in one global scope would fight over it.
//
// Every package comes from a CDN at an exact version, checked against its published files (the
// PLAN.md log of this step lists what each one is and why it is not hand-rolled). The models come
// from Hugging Face and are kept by the browser: Kokoro's by Transformers.js in the Cache API,
// Piper's here in the Cache API, Kitten's by the HTTP cache.
//
//   in:  { id, op: 'load' }                          → { id, ok, device, voices: [{ id, name, lang }] }
//        { id, op: 'render', text, voice, speed }    → { id, ok, pcm: Float32Array, rate, lead, took }
//   out, meanwhile: { progress: { text, fraction } }   (fraction 0..1, or null if unknown) — of
//        the engine, whichever request it is working for: a voice's model downloads on its first line
//   failure:        { id, ok: false, error }
//
// A rendered line is trimmed of the silence a model puts before and after the speech, keeping a
// margin of 10 ms either side so no consonant is clipped, and says how long that margin before
// the speech is (`lead`, seconds): the page starts the buffer that much early, so the speech
// itself is heard on the bar.

const ENGINE = new URL(self.location.href).searchParams.get('engine');
// `device=wasm` (the page passes its own `?device=wasm` on) refuses WebGPU even where it is
// offered: for a GPU that misbehaves, and to hear — and test — the WebAssembly paths on a machine
// that has one.
const NO_WEBGPU = new URL(self.location.href).searchParams.get('device') === 'wasm';

// Kokoro-82M (Apache-2.0) through kokoro-js 1.2.1, whose web build bundles Transformers.js 3.5.1;
// that fetches its ONNX runtime's WebAssembly from jsDelivr at @huggingface/transformers@3.5.1.
const KOKORO = 'https://cdn.jsdelivr.net/npm/kokoro-js@1.2.1/dist/kokoro.web.js';
const KOKORO_MODEL = 'onnx-community/Kokoro-82M-v1.0-ONNX';

// Piper voices (each its own licence, in its MODEL_CARD) run on onnxruntime-web 1.18.0, with the
// espeak-ng phonemiser piper uses, built to WebAssembly: the ES module of it published in
// @mintplex-labs/piper-tts-web 1.0.5 (a maintained fork of @diffusionstudio/vits-web; this file of
// it is the emscripten loader, importing nothing) and its binary and data from
// @diffusionstudio/piper-wasm 1.0.0, which is what that package itself loads. The voices are
// rhasspy/piper-voices at a fixed revision.
const ORT = 'https://cdn.jsdelivr.net/npm/onnxruntime-web@1.18.0/dist/';
const PIPER_PHONEMIZE = 'https://cdn.jsdelivr.net/npm/@mintplex-labs/piper-tts-web@1.0.5/dist/piper-o91UDS6e.js';
const PIPER_WASM = 'https://cdn.jsdelivr.net/npm/@diffusionstudio/piper-wasm@1.0.0/build/piper_phonemize';
const PIPER_VOICES = 'https://huggingface.co/rhasspy/piper-voices/resolve/5041a7a49ae5b3a0f30e6837769eb03fdbb231bd';

// Kitten TTS nano 0.8 (Apache-2.0) through kitten-tts-webgpu 0.1.1, which runs it in WebGPU
// compute shaders and nowhere else.
const KITTEN = 'https://cdn.jsdelivr.net/npm/kitten-tts-webgpu@0.1.1/dist-lib/index.js';
const KITTEN_VOICES = ['Bella', 'Jasper', 'Luna', 'Bruno', 'Rosie', 'Hugo', 'Kiki', 'Leo'];

// Below this a sample is silence: -50 dBFS. A line keeps 10 ms either side of its speech.
const SILENCE = 10 ** (-50 / 20);
const MARGIN_SECONDS = 0.01;

function trimmed(pcm, rate) {
  let first = 0;
  while (first < pcm.length && Math.abs(pcm[first]) < SILENCE) first += 1;
  let last = pcm.length - 1;
  while (last > first && Math.abs(pcm[last]) < SILENCE) last -= 1;
  if (first >= pcm.length) return { kept: new Float32Array(0), lead: 0 };
  const margin = Math.round(MARGIN_SECONDS * rate);
  const from = Math.max(0, first - margin);
  return { kept: pcm.slice(from, Math.min(pcm.length, last + 1 + margin)), lead: (first - from) / rate };
}

async function webgpu() {
  if (NO_WEBGPU) return false;
  try {
    return !!(navigator.gpu && await navigator.gpu.requestAdapter());
  } catch (error) {
    return false;
  }
}

// A file from the Cache API, or fetched with progress and kept there.
async function cached(url, progress, label) {
  let cache = null;
  try {
    cache = await caches.open('prosody-voices');
    const hit = await cache.match(url);
    if (hit) return await hit.arrayBuffer();
  } catch (error) { cache = null; }
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${label}: ${response.status} from ${new URL(url).host}`);
  const total = Number(response.headers.get('Content-Length')) || 0;
  const reader = response.body.getReader();
  const chunks = [];
  let loaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    loaded += value.length;
    progress?.(`${label} ${Math.round(loaded / 1e6)} MB`, total ? loaded / total : null);
  }
  const bytes = new Uint8Array(loaded);
  let at = 0;
  for (const chunk of chunks) { bytes.set(chunk, at); at += chunk.length; }
  try { await cache?.put(url, new Response(bytes.slice(0))); } catch (error) { /* full: fine */ }
  return bytes.buffer;
}

const engines = {
  kokoro: () => {
    let tts = null;
    return {
      async load() {
        const { KokoroTTS } = await import(KOKORO);
        const files = new Map();
        const callback = (info) => {
          if (info.status !== 'progress' || !info.total) return;
          files.set(info.file, [info.loaded, info.total]);
          let loaded = 0, total = 0;
          for (const [have, of] of files.values()) { loaded += have; total += of; }
          progress(`Kokoro ${Math.round(loaded / 1e6)} of ${Math.round(total / 1e6)} MB`, loaded / total);
        };
        // WebGPU wants the full-precision model (kokoro-js's own advice); WebAssembly the 8-bit
        // one, 92 MB. If WebGPU is offered but fails, WebAssembly is tried before giving up.
        const attempt = async (device) => {
          const model = await KokoroTTS.from_pretrained(KOKORO_MODEL, {
            dtype: device === 'webgpu' ? 'fp32' : 'q8', device, progress_callback: callback,
          });
          progress(`Kokoro warming up (${device})`, null);
          await model.generate('Ready.', { voice: 'af_heart' });
          return model;
        };
        let device = (await webgpu()) ? 'webgpu' : 'wasm';
        try {
          tts = await attempt(device);
        } catch (error) {
          if (device !== 'webgpu') throw error;
          console.warn('[kokoro] WebGPU failed, trying WebAssembly:', error);
          device = 'wasm';
          tts = await attempt(device);
        }
        const voices = Object.entries(tts.voices).map(([id, voice]) => ({
          id, name: `${voice.name} (${voice.gender.toLowerCase()}, ${voice.language}, grade ${voice.overallGrade})`,
          lang: voice.language,
        }));
        return { device, voices };
      },
      async render(text, voice, speed) {
        const audio = await tts.generate(text, { voice, speed });
        return { pcm: audio.audio, rate: audio.sampling_rate };
      },
    };
  },

  piper: () => {
    let ort = null, createPiperPhonemize = null, phonemizeWasm = null, phonemizeData = null;
    let catalogue = null;
    const sessions = new Map();   // voice id → { session, config }

    async function voice(id) {
      if (sessions.has(id)) return sessions.get(id);
      const entry = catalogue[id];
      if (!entry) throw new Error(`no Piper voice ${id}`);
      const path = Object.keys(entry.files).find((file) => file.endsWith('.onnx'));
      const [config, model] = await Promise.all([
        cached(`${PIPER_VOICES}/${path}.json`, null, id).then((bytes) => JSON.parse(new TextDecoder().decode(bytes))),
        cached(`${PIPER_VOICES}/${path}`, progress, `Piper ${id}`),
      ]);
      const session = await ort.InferenceSession.create(new Uint8Array(model), { executionProviders: ['wasm'] });
      const loaded = { session, config };
      sessions.set(id, loaded);
      return loaded;
    }

    // espeak-ng, as piper's phonemiser: text in, the voice's phoneme ids out. A fresh module per
    // line, as piper-tts-web does — its `main` runs once — given the binary and data already
    // fetched, so nothing is fetched again.
    function phonemize(text, espeakVoice) {
      return new Promise((resolve, reject) => {
        const said = [];
        const errors = [];
        createPiperPhonemize({
          print: (line) => said.push(line),
          printErr: (line) => errors.push(line),
          locateFile: (file) => (file.endsWith('.wasm') ? `${PIPER_WASM}.wasm` : file.endsWith('.data') ? `${PIPER_WASM}.data` : file),
          wasmBinary: phonemizeWasm,
          getPreloadedPackage: () => phonemizeData.slice(0),
        }).then((module) => {
          module.callMain(['-l', espeakVoice, '--input', JSON.stringify([{ text }]), '--espeak_data', '/espeak-ng-data']);
          // One JSON object per input line, its phoneme ids those of every sentence in it.
          const ids = said.flatMap((line) => {
            try { return JSON.parse(line).phoneme_ids ?? []; } catch (error) { return []; }
          });
          if (ids.length) resolve(ids);
          else reject(new Error('piper could not phonemise the line' + (errors.length ? ': ' + errors.join(' ') : '')));
        }, reject);
      });
    }

    return {
      async load() {
        progress('Piper runtime', null);
        ort = await import(`${ORT}esm/ort.wasm.min.js`);
        ort.env.wasm.wasmPaths = ORT;
        // The page is not cross-origin isolated, so no threads: say so rather than let the
        // runtime try and warn.
        ort.env.wasm.numThreads = 1;
        ({ createPiperPhonemize } = await import(PIPER_PHONEMIZE));
        [phonemizeWasm, phonemizeData, catalogue] = await Promise.all([
          cached(`${PIPER_WASM}.wasm`, null, 'phonemiser'),
          cached(`${PIPER_WASM}.data`, progress, 'espeak-ng data'),
          cached(`${PIPER_VOICES}/voices.json`, null, 'voices').then((bytes) => JSON.parse(new TextDecoder().decode(bytes))),
        ]);
        const voices = Object.values(catalogue).map((entry) => ({
          id: entry.key,
          name: `${entry.name} (${entry.language.name_english}, ${entry.language.country_english}, ${entry.quality})`,
          lang: entry.language.code.replace('_', '-'),
        })).sort((a, b) => a.id.localeCompare(b.id));
        return { device: 'wasm', voices };
      },
      async render(text, id, speed) {
        const { session, config } = await voice(id);
        const ids = await phonemize(text, config.espeak.voice);
        const scales = config.inference;
        const feeds = {
          input: new ort.Tensor('int64', BigInt64Array.from(ids, BigInt), [1, ids.length]),
          input_lengths: new ort.Tensor('int64', BigInt64Array.from([BigInt(ids.length)]), [1]),
          // Piper's own speed control: a longer length scale is slower, at the voice's own pitch.
          scales: new ort.Tensor('float32', Float32Array.from([scales.noise_scale, scales.length_scale / speed, scales.noise_w]), [3]),
        };
        if (config.num_speakers > 1) feeds.sid = new ort.Tensor('int64', BigInt64Array.from([0n]), [1]);
        const { output } = await session.run(feeds);
        return { pcm: output.data, rate: config.audio.sample_rate };
      },
    };
  },

  kitten: () => {
    let kitten = null;
    return {
      async load() {
        if (!(await webgpu())) {
          throw new Error('Kitten runs only on WebGPU, which this browser does not offer');
        }
        kitten = await import(KITTEN);
        await kitten.textToSpeech('Ready.', { model: 'nano', onProgress: (stage) => progress(`Kitten: ${stage}`, null) });
        return { device: 'webgpu', voices: KITTEN_VOICES.map((id) => ({ id, name: id, lang: 'en-us' })) };
      },
      async render(text, voice, speed) {
        const wav = await kitten.textToSpeech(text, { voice, speed, model: 'nano' });
        return pcmOf(await wav.arrayBuffer());
      },
    };
  },
};

// A 16-bit PCM WAV file's samples, as floats.
function pcmOf(bytes) {
  const view = new DataView(bytes);
  const rate = view.getUint32(24, true);
  let at = 12;
  while (at + 8 <= view.byteLength) {
    const id = String.fromCharCode(...new Uint8Array(bytes, at, 4));
    const size = view.getUint32(at + 4, true);
    if (id === 'data') {
      const samples = new Int16Array(bytes.slice(at + 8, at + 8 + size));
      return { pcm: Float32Array.from(samples, (s) => s / 32768), rate };
    }
    at += 8 + size;
  }
  throw new Error('Kitten gave a WAV with no data');
}

function progress(text, fraction) {
  postMessage({ progress: { text, fraction } });
}

const engine = engines[ENGINE]?.();
let ready = null;
// One request at a time, in order: a line asked for while another renders waits its turn.
let queue = Promise.resolve();

onmessage = ({ data }) => {
  const { id, op } = data;
  queue = queue.then(async () => {
    try {
      if (!engine) throw new Error(`no engine named ${ENGINE}`);
      if (op === 'load') {
        ready ??= engine.load();
        const info = await ready;
        postMessage({ id, ok: true, ...info });
      } else if (op === 'render') {
        if (!ready) throw new Error('not loaded');
        await ready;
        const began = performance.now();
        const { pcm, rate } = await engine.render(data.text, data.voice, data.speed);
        const { kept, lead } = trimmed(pcm, rate);
        postMessage({ id, ok: true, pcm: kept, rate, lead, took: performance.now() - began }, [kept.buffer]);
      } else {
        throw new Error(`no such request: ${op}`);
      }
    } catch (error) {
      if (op === 'load') ready = null;   // a failed load can be tried again
      postMessage({ id, ok: false, error: String(error?.message ?? error) });
    }
  });
};
