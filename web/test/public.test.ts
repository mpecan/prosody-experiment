// The files the page loads beside itself, as they are shipped.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const web = join(__dirname, '..');
const read = (file: string) => readFileSync(join(web, file), 'utf8');

describe('the page\'s HTML', () => {
  const html = read('index.html');

  it('answers Strudel\'s own imports from the Strudel already on the page, by relative paths', () => {
    // Relative, so the static page works from a subdirectory of a host as well as from `/`.
    expect(html).toContain('"@strudel/core": "./strudel-globals.mjs"');
    expect(html).toContain('"@strudel/webaudio": "./strudel-globals.mjs"');
    // From beside the page, not a CDN: the phone app plays with no network.
    expect(html).toContain('<script vite-ignore src="./vendor/strudel-web.js"></script>');
    expect(html).not.toMatch(/<script[^>]* src="https?:/);
    expect(html).not.toMatch(/<link[^>]* href="https?:/);
  });
});

describe('the Strudel shim', () => {
  // The names a vendored module imports from the packages the import map sends to the shim.
  const importedFromShim = (file: string) =>
    [...read(file).matchAll(/import\s*\{([^}]*)\}\s*from\s*["'](@strudel\/core|@strudel\/webaudio|sfumato)["']/g)]
      .flatMap((match) => match[1].split(','))
      .map((name) => name.trim().split(/\s+as\s+/)[0])
      .filter(Boolean);

  it('exports every name the soundfonts and the roll import from Strudel', () => {
    // A name missing here leaves every soundfont unregistered and the melody silent.
    const shim = read('public/strudel-globals.mjs');
    const names = [
      ...importedFromShim('node_modules/@strudel/soundfonts/dist/index.mjs'),
      ...importedFromShim('node_modules/@strudel/draw/draw.mjs'),
    ];
    expect(names).toContain('registerSound');
    expect(names).toContain('TimeSpan');
    for (const name of names) expect(shim, name).toMatch(new RegExp(`\\b${name}\\b`));
  });
});

describe('the speech worker', () => {
  it('loads every package it fetches at an exact version', () => {
    const worker = read('public/speech-worker.mjs');
    for (const pinned of [
      'kokoro-js@1.2.1/',
      'onnxruntime-web@1.18.0/',
      '@mintplex-labs/piper-tts-web@1.0.5/',
      '@diffusionstudio/piper-wasm@1.0.0/',
      'kitten-tts-webgpu@0.1.1/',
      'piper-voices/resolve/5041a7a49ae5b3a0f30e6837769eb03fdbb231bd',
    ]) expect(worker, pinned).toContain(pinned);
  });
});

describe('the soundfonts', () => {
  // scripts/render-soundfonts.mjs writes them; the Prosody player reads them (src/player/samples.ts).
  const dir = join(web, 'public/soundfonts');
  const manifests = readdirSync(dir).filter((file) => file.endsWith('.json'));

  it('has one for every soundfont the compiler plays, each zone\'s recording beside it', () => {
    // style::SOUNDFONT_VARIANTS has 28; site.json names each by its file (a test there pins that).
    expect(manifests.length).toBe(28);
    const recordings = new Set(readdirSync(join(dir, 'zones')));
    const used = new Set<string>();
    for (const file of manifests) {
      const manifest = JSON.parse(read(`public/soundfonts/${file}`));
      expect(manifest.licence, file).toMatch(/FluidR3 GM|GeneralUser GS/);
      expect(manifest.zones.length, file).toBeGreaterThan(0);
      manifest.zones.forEach((zone: { position: number; low: number; high: number; rate: number; file: string;
        loopStart: number | null; loopEnd: number | null }, index: number) => {
        expect(zone.position, file).toBe(index);
        expect(zone.low <= zone.high && zone.high <= 127, file).toBe(true);
        expect(zone.rate, file).toBeGreaterThanOrEqual(8_000);
        expect(zone.loopStart === null || (zone.loopEnd !== null && zone.loopEnd > zone.loopStart), file).toBe(true);
        expect(existsSync(join(dir, zone.file)), zone.file).toBe(true);
        used.add(zone.file.replace(/^zones\//, ''));
      });
    }
    // No recording is shipped that no soundfont plays.
    expect([...recordings].filter((name) => !used.has(name))).toEqual([]);
  });
});
