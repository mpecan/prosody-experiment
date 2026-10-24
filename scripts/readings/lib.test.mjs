// `npm test`: the pure half of the recording tool, with values worked by hand.
import { test } from 'node:test';
import assert from 'node:assert/strict';

import { VOICES, fileOf, keyOf, linesOf, manifest, wanted } from './lib.mjs';

test('a line is keyed by the first 64 bits of the SHA-256 of its exact text', () => {
  // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad (FIPS 180-2, B.1).
  assert.equal(keyOf('abc'), 'ba7816bf8f01cfea');
  // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.
  assert.equal(keyOf(''), 'e3b0c44298fc1c14');
  // Exact text: a trailing space, a capital or another dash is another line.
  assert.notEqual(keyOf('Tyger Tyger, burning bright,'), keyOf('Tyger Tyger, burning bright, '));
  assert.notEqual(keyOf('a—b'), keyOf('a-b'));
});

test('a recording is filed under its voice, its text and the pace it was spoken at', () => {
  assert.equal(fileOf('kokoro:af_heart', 'ba7816bf8f01cfea', 0.95), 'kokoro-af_heart/ba7816bf8f01cfea-0.950.mp3');
  assert.equal(fileOf('piper:sl_SI-artur-medium', '0011', 1), 'piper-sl_SI-artur-medium/0011-1.000.mp3');
  // The same text at another pace is another file: a changed pace rule leaves no file standing in
  // for audio spoken at the old speed.
  assert.notEqual(fileOf('kokoro:af_heart', '0011', 1.08), fileOf('kokoro:af_heart', '0011', 1.081));
});

test('the lines of a poem are every line any setting reads, each at its one pace', () => {
  const breath = { lines: [{ text: 'one', pace: 0.94 }, { text: 'two', pace: 1.08 }], cues: [{ text: 'one', pace: 0.94 }] };
  const arrives = { lines: [{ text: 'three', pace: 1 }, { text: '  ', pace: 1 }], cues: [] };
  const lines = linesOf([breath, arrives]);
  assert.deepEqual([...lines], [['one', 0.94], ['two', 1.08], ['three', 1]]);
  assert.throws(() => linesOf([breath, { lines: [{ text: 'one', pace: 1.01 }], cues: [] }]), /read at 0.94 and at 1.01/);
});

test('each line is wanted in each voice of its poem\'s language, and no other', () => {
  const poems = [
    { name: 'tyger', language: 'en', lines: new Map([['Tyger Tyger, burning bright,', 1.02]]) },
    { name: 'zdravljica', language: 'sl', lines: new Map([['Prijatlji, obrodile', 0.97]]) },
  ];
  const all = wanted(poems);
  const english = VOICES.filter((v) => v.lang === 'en').length;
  assert.equal(english, 3, 'three English voices');
  assert.equal(all.length, english + 1);
  assert.deepEqual(all.filter((r) => r.poem === 'zdravljica').map((r) => r.voice.id), ['piper:sl_SI-artur-medium']);
  const heart = all.find((r) => r.voice.id === 'kokoro:af_heart');
  assert.equal(heart.pace, 1.02);
  assert.equal(heart.file, `kokoro-af_heart/${keyOf('Tyger Tyger, burning bright,')}-1.020.mp3`);
});

test('the manifest lists what exists, sorted, a line of the file each', () => {
  const voices = [VOICES[0], VOICES[2]];
  const poems = [{ name: 'p', language: 'en', lines: new Map([['b', 1.08], ['a', 0.92]]) }];
  const all = wanted(poems, voices);
  // Only "a" by Heart and "b" by Cori are on disk.
  const recorded = new Map([
    [fileOf('kokoro:af_heart', keyOf('a'), 0.92), { seconds: 1.23456 }],
    [fileOf('piper:en_GB-cori-medium', keyOf('b'), 1.08), { seconds: 2 }],
  ]);
  const text = manifest(poems, all, recorded, { 'kokoro:af_heart': 'webgpu' }, voices);
  const parsed = JSON.parse(text);
  assert.deepEqual(Object.keys(parsed.voices), ['kokoro:af_heart', 'piper:en_GB-cori-medium']);
  assert.equal(parsed.voices['piper:en_GB-cori-medium'].device, 'unknown');
  // keyOf('a') = ca978112ca1bbdca…, keyOf('b') = 3e23e8160039594a…: "b" sorts first.
  assert.equal(keyOf('a'), 'ca978112ca1bbdca');
  assert.equal(keyOf('b'), '3e23e8160039594a');
  assert.deepEqual(Object.keys(parsed.lines), ['3e23e8160039594a', 'ca978112ca1bbdca']);
  assert.ok(text.includes('    "ca978112ca1bbdca": {"text":"a","pace":0.920,"recordings":{"kokoro:af_heart":{"file":"kokoro-af_heart/ca978112ca1bbdca-0.920.mp3","seconds":1.235}}}\n'));
  assert.deepEqual(Object.keys(parsed.lines['3e23e8160039594a'].recordings), ['piper:en_GB-cori-medium']);
  assert.deepEqual(parsed.poems, { p: { language: 'en', lines: 2 } });
  // The same input, the same bytes.
  assert.equal(manifest(poems, all, recorded, { 'kokoro:af_heart': 'webgpu' }, voices), text);
});

test('a recording made at another pace is not listed as the line\'s', () => {
  // "a" is now read at 0.920; on disk is only its file from when it was read at 0.950. That file is
  // not the one wanted, so the manifest has no recording of "a" — the page reads it live, and
  // render.mjs renders it again — rather than claiming the old audio is at the new pace.
  const voices = [VOICES[0]];
  const poems = [{ name: 'p', language: 'en', lines: new Map([['a', 0.92]]) }];
  const all = wanted(poems, voices);
  assert.equal(all[0].file, 'kokoro-af_heart/ca978112ca1bbdca-0.920.mp3');
  const stale = new Map([['kokoro-af_heart/ca978112ca1bbdca-0.950.mp3', { seconds: 1 }]]);
  assert.deepEqual(JSON.parse(manifest(poems, all, stale, {}, voices)).lines, {});
});
