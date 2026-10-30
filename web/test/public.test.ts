// The files the page loads beside itself, as they are shipped.
import { readFileSync } from 'node:fs';
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
