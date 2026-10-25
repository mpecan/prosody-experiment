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
    expect(html).toContain('<script src="https://unpkg.com/@strudel/web@1.3.0"></script>');
  });
});

describe('the Strudel shim', () => {
  it('exports what the soundfonts and the roll import from Strudel', () => {
    // A name missing here leaves every soundfont unregistered and the melody silent.
    const shim = read('public/strudel-globals.mjs');
    for (const name of ['registerSound', 'getAudioContext', 'Pattern', 'noteToMidi', 'startPresetNote', 'State', 'TimeSpan']) {
      expect(shim, name).toContain(name);
    }
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
