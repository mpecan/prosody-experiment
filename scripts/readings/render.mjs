// Records the example poems' readings (SPEC.md §4.7, the `recorded` engine), on this machine, for
// the page to play where it has them — on the static site above all, where a browser may have no
// WebGPU and a 92 MB model is a lot to ask for.
//
//   ./scripts/build-site.sh                  # the compiler as WebAssembly, which this asks
//   cd scripts/readings && npm ci            # playwright-core, once
//   node render.mjs                          # every missing recording, into examples/readings
//   node render.mjs --poem blake-tyger --voice kokoro:af_heart --force
//   node render.mjs --check                  # every recording there, none left over; renders nothing
//
// For every example poem it asks the compiler (the site's WebAssembly build, as the page does) for
// the reading under several settings, and so for every line any of them can speak and that line's
// natural pace. Each line is then spoken by each voice of the poem's language AT THAT PACE — the
// speed is baked in at render time, so the page plays the file as it is — by the page's own
// speech worker (crates/prosody-web/src/speech-worker.mjs) running in Chromium: the same engines,
// models and phonemisers as the live page, so a recording and a live render of a line are the
// same voice. The worker trims the model's silence to 10 ms either side; ffmpeg encodes MP3.
//
// A recording is named by its voice, a hash of its line's exact text (lib.mjs `keyOf`) and the
// pace it was spoken at (lib.mjs `fileOf`), so a line edited on the page has no recording and is
// read live, and a changed pace rule wants new files rather than relabelling the old audio (the
// old files are left over and removed, the new ones rendered). Only missing files are rendered
// unless --force; a file no example line wants any more is deleted, unless --poem or --voice
// narrowed the run. The manifest is rewritten from what is on disk, deterministically.
//
// Needs: node ≥ 20, ffmpeg with libmp3lame, network (the engines' code from jsDelivr, the models
// from Hugging Face), and Chromium for playwright-core 1.64.0 (`npx playwright-core install
// chromium` if it is not there, or --chromium PATH).

import { execFileSync, spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

import { SETTINGS, VOICES, linesOf, manifest, wanted } from './lib.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');

const options = { site: join(repo, 'target/site'), out: join(repo, 'examples/readings'), poems: [], voices: [], force: false, check: false, chromium: null };
const argv = process.argv.slice(2);
while (argv.length) {
  const arg = argv.shift();
  if (arg === '--site') options.site = resolve(argv.shift());
  else if (arg === '--out') options.out = resolve(argv.shift());
  else if (arg === '--poem') options.poems.push(argv.shift());
  else if (arg === '--voice') options.voices.push(argv.shift());
  else if (arg === '--force') options.force = true;
  else if (arg === '--check') options.check = true;
  else if (arg === '--chromium') options.chromium = resolve(argv.shift());
  else { console.error(`readings: unknown argument ${arg}`); process.exit(2); }
}

// ---- What to record ----

if (!existsSync(join(options.site, 'prosody.js'))) {
  console.error(`readings: no compiler in ${options.site} — run ./scripts/build-site.sh first`);
  process.exit(1);
}
const bindings = await import(pathToFileURL(join(options.site, 'prosody.js')).href);
await bindings.default({ module_or_path: readFileSync(join(options.site, 'prosody_bg.wasm')) });
function ask(target, body = '') {
  const answer = bindings.respond(target, body);
  const { status, body: text } = answer;
  answer.free();
  if (status !== 200) throw new Error(`${target}: ${status} ${text}`);
  return JSON.parse(text);
}

const presets = ask('/presets.json');
const poems = presets.map((preset) => ({
  name: preset.name,
  language: preset.language,
  lines: linesOf(SETTINGS.map((query) => ask('/reading' + (query ? '?' + query : ''), preset.poem))),
}));
const all = wanted(poems);
for (const id of options.voices) {
  if (!VOICES.some((v) => v.id === id)) { console.error(`readings: no voice ${id}; the voices are ${VOICES.map((v) => v.id).join(', ')}`); process.exit(2); }
}
for (const name of options.poems) {
  if (!poems.some((p) => p.name === name)) { console.error(`readings: no example ${name}`); process.exit(2); }
}
const narrowed = options.poems.length > 0 || options.voices.length > 0;
const chosen = all.filter((r) => (!options.poems.length || options.poems.includes(r.poem))
  && (!options.voices.length || options.voices.includes(r.voice.id)));

// One recording per voice and text, whichever poems share the line.
const unique = (list) => [...new Map(list.map((r) => [r.file, r])).values()];
const toRender = unique(chosen).filter((r) => options.force || !existsSync(join(options.out, r.file)));

// What is on disk, and what is there that nothing wants.
function onDisk() {
  const files = [];
  if (!existsSync(options.out)) return files;
  for (const dir of readdirSync(options.out)) {
    const path = join(options.out, dir);
    if (!statSync(path).isDirectory()) continue;
    for (const name of readdirSync(path)) files.push(`${dir}/${name}`);
  }
  return files;
}
const wantedFiles = new Set(all.map((r) => r.file));

if (options.check) {
  const missing = unique(all).filter((r) => !existsSync(join(options.out, r.file)));
  const extra = onDisk().filter((file) => !wantedFiles.has(file));
  const previous = existsSync(join(options.out, 'manifest.json')) ? readFileSync(join(options.out, 'manifest.json'), 'utf8') : '';
  const fresh = writeManifest(false);
  for (const r of missing) console.error(`readings: missing ${r.file} (${r.voice.id}, ${r.poem}): ${r.text}`);
  for (const file of extra) console.error(`readings: no line wants ${file}`);
  if (previous !== fresh) console.error('readings: manifest.json is not what the files on disk make — run node render.mjs');
  console.log(`readings: ${unique(all).length - missing.length} of ${unique(all).length} recordings there, ${extra.length} left over, ${sizeOf()} bytes`);
  process.exit(missing.length || extra.length || previous !== fresh ? 1 : 0);
}

// ---- Speaking ----

console.log(`readings: ${unique(all).length} recordings wanted (${poems.length} poems, ${VOICES.length} voices), ${toRender.length} to render`);
const devices = readDevices();
if (toRender.length) await render(toRender, devices);

if (!narrowed) {
  for (const file of onDisk().filter((f) => !wantedFiles.has(f))) {
    rmSync(join(options.out, file));
    console.log(`readings: removed ${file}, which no line wants any more`);
  }
}
writeManifest(true, devices);
console.log(`readings: ${sizeOf()} bytes in ${relative(process.cwd(), options.out) || '.'}`);

async function render(jobs, devices) {
  ffmpegOrExit();
  const { chromium } = await import('playwright-core');
  const worker = readFileSync(join(repo, 'crates/prosody-web/src/speech-worker.mjs'));
  // The worker as the page has it, at the same relative URL, from a page of our own on localhost.
  const server = createServer((request, response) => {
    if (request.url.startsWith('/speech-worker.mjs')) {
      response.writeHead(200, { 'Content-Type': 'text/javascript; charset=utf-8' });
      response.end(worker);
    } else if (request.url === '/') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
      response.end(host());
    } else { response.writeHead(404); response.end(); }
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  // A profile of its own, kept beside the tool (and ignored by git), so the models it downloads —
  // up to 330 MB for Kokoro on WebGPU — are downloaded once, not on every run.
  async function launch() {
    const context = await chromium.launchPersistentContext(join(here, '.profile'), {
      channel: options.chromium ? undefined : 'chromium',
      executablePath: options.chromium ?? undefined,
      headless: true,
      args: ['--enable-unsafe-webgpu'],
    });
    const page = context.pages()[0] ?? await context.newPage();
    // The engines' download progress, a line at most every two seconds; anything else as it comes.
    let said = 0;
    page.on('console', (message) => {
      const text = message.text();
      const progress = / MB$/.test(text);
      if (progress && Date.now() - said < 2000) return;
      said = Date.now();
      console.error('  [browser]', text);
    });
    page.on('pageerror', (error) => console.error('  [browser error]', error.message));
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    return { context, page };
  }
  // A line that takes this long has hung the engine — Kokoro on WebGPU in headless Chromium was
  // seen once to stop answering after 385 lines, and the same line rendered at once in a fresh
  // browser — so the browser is started afresh and the line tried again, up to TRIES times. Loading a model counts against the first
  // line, so the first allows for a download.
  const STUCK_MS = 60_000, FIRST_MS = 900_000, TRIES = 3;
  let session = await launch();
  let fresh = new Set();   // engines loaded in this session
  try {
    let done = 0;
    for (const job of jobs) {
      const began = Date.now();
      let answer = null;
      for (let attempt = 1; !answer; attempt += 1) {
        const limit = fresh.has(job.voice.engine) ? STUCK_MS : FIRST_MS;
        try {
          answer = await Promise.race([
            session.page.evaluate(([engine, voice, text, speed]) => window.speak(engine, voice, text, speed),
              [job.voice.engine, job.voice.voice, job.text, job.pace]),
            new Promise((_, reject) => setTimeout(() => reject(new Error(`no answer in ${limit / 1000} s`)), limit)),
          ]);
        } catch (error) {
          if (attempt >= TRIES) throw new Error(`${job.voice.id} could not speak "${job.text}": ${error.message}`);
          console.error(`readings: ${job.voice.id} ${error.message} for "${job.text}"; starting the browser again`);
          await session.context.close().catch(() => {});
          session = await launch();
          fresh = new Set();
        }
      }
      if (!answer.ok) throw new Error(`${job.voice.id} could not speak "${job.text}": ${answer.error}`);
      fresh.add(job.voice.engine);
      devices[job.voice.id] = answer.device;
      const path = join(options.out, job.file);
      mkdirSync(dirname(path), { recursive: true });
      encode(Buffer.from(answer.wav, 'base64'), path);
      done += 1;
      console.log(`readings: ${done}/${jobs.length} ${job.voice.id} on ${answer.device}, pace ${job.pace.toFixed(2)}, ${answer.seconds.toFixed(2)} s in ${((Date.now() - began) / 1000).toFixed(1)} s: ${job.text}`);
    }
  } finally {
    await session.context.close().catch(() => {});
    server.close();
  }
}

function ffmpegOrExit() {
  const probe = spawnSync('ffmpeg', ['-hide_banner', '-encoders'], { encoding: 'utf8' });
  if (probe.status !== 0 || !probe.stdout.includes('libmp3lame')) {
    console.error('readings: needs ffmpeg with libmp3lame on PATH (brew install ffmpeg)');
    process.exit(1);
  }
}

// WAV in, MP3 out: mono, 32 kbit/s, at the voice's own rate, with no tags and no encoder version,
// so the same samples make the same bytes.
function encode(wav, path) {
  execFileSync('ffmpeg', ['-hide_banner', '-loglevel', 'error', '-y', '-f', 'wav', '-i', 'pipe:0',
    '-ac', '1', '-c:a', 'libmp3lame', '-b:a', '32k', '-map_metadata', '-1',
    '-fflags', '+bitexact', '-flags:a', '+bitexact', path], { input: wav });
}

// How long a recording is, in seconds, by ffprobe.
function secondsOf(path) {
  const out = execFileSync('ffprobe', ['-v', 'error', '-show_entries', 'format=duration', '-of', 'default=nw=1:nk=1', path], { encoding: 'utf8' });
  return Number(out.trim());
}

function readDevices() {
  try {
    const old = JSON.parse(readFileSync(join(options.out, 'manifest.json'), 'utf8'));
    return Object.fromEntries(Object.entries(old.voices).map(([id, v]) => [id, v.device]));
  } catch (error) { return {}; }
}

function writeManifest(write, devices = readDevices()) {
  let old = new Map();
  try {
    const previous = JSON.parse(readFileSync(join(options.out, 'manifest.json'), 'utf8'));
    for (const entry of Object.values(previous.lines)) {
      for (const f of Object.values(entry.recordings)) old.set(f.file, f.seconds);
    }
  } catch (error) { old = new Map(); }
  const recorded = new Map();
  for (const r of unique(all)) {
    const path = join(options.out, r.file);
    if (!existsSync(path)) continue;
    // A file rendered in this run is measured again; one kept keeps its measure.
    const fresh = toRender.some((j) => j.file === r.file) || !old.has(r.file);
    recorded.set(r.file, { seconds: fresh ? secondsOf(path) : old.get(r.file) });
  }
  const text = manifest(poems, all, recorded, devices);
  if (write) { mkdirSync(options.out, { recursive: true }); writeFileSync(join(options.out, 'manifest.json'), text); }
  return text;
}

function sizeOf() {
  let bytes = 0;
  for (const file of [...onDisk(), 'manifest.json']) {
    const path = join(options.out, file);
    if (existsSync(path)) bytes += statSync(path).size;
  }
  return bytes;
}

// The page Chromium runs the worker in: one worker per engine, loaded once, asked one line at a
// time; the trimmed samples come back as a 16-bit WAV.
function host() {
  return `<!doctype html><meta charset="utf-8"><title>readings</title><script type="module">
const workers = {};
let asked = 0;
function worker(engine) {
  if (workers[engine]) return workers[engine];
  const w = new Worker('./speech-worker.mjs?engine=' + engine, { type: 'module' });
  const waiting = new Map();
  w.onmessage = ({ data }) => {
    if (data.progress) { console.log(engine + ': ' + data.progress.text); return; }
    const pending = waiting.get(data.id); waiting.delete(data.id);
    if (pending) pending(data);
  };
  const ask = (message) => new Promise((resolve) => { asked += 1; waiting.set(asked, resolve); w.postMessage({ id: asked, ...message }); });
  workers[engine] = { ask, loaded: ask({ op: 'load' }) };
  return workers[engine];
}
function wav(pcm, rate) {
  const bytes = new DataView(new ArrayBuffer(44 + pcm.length * 2));
  const text = (at, s) => { for (let i = 0; i < s.length; i++) bytes.setUint8(at + i, s.charCodeAt(i)); };
  text(0, 'RIFF'); bytes.setUint32(4, 36 + pcm.length * 2, true); text(8, 'WAVE');
  text(12, 'fmt '); bytes.setUint32(16, 16, true); bytes.setUint16(20, 1, true); bytes.setUint16(22, 1, true);
  bytes.setUint32(24, rate, true); bytes.setUint32(28, rate * 2, true); bytes.setUint16(32, 2, true); bytes.setUint16(34, 16, true);
  text(36, 'data'); bytes.setUint32(40, pcm.length * 2, true);
  for (let i = 0; i < pcm.length; i++) bytes.setInt16(44 + i * 2, Math.max(-1, Math.min(1, pcm[i])) * 32767, true);
  let binary = '';
  const all = new Uint8Array(bytes.buffer);
  for (let i = 0; i < all.length; i += 0x8000) binary += String.fromCharCode(...all.subarray(i, i + 0x8000));
  return btoa(binary);
}
window.speak = async (engine, voice, text, speed) => {
  const w = worker(engine);
  const loaded = await w.loaded;
  if (!loaded.ok) return { ok: false, error: loaded.error };
  const answer = await w.ask({ op: 'render', text, voice, speed });
  if (!answer.ok) return { ok: false, error: answer.error };
  return { ok: true, device: engine === 'kokoro' ? 'kokoro-js 1.2.1, ' + (loaded.device === 'webgpu' ? 'fp32 on WebGPU' : 'q8 on WebAssembly') : 'onnxruntime-web 1.18.0 on WebAssembly',
    wav: wav(answer.pcm, answer.rate), seconds: answer.pcm.length / answer.rate };
};
</script>`;
}
