// Sharing without storage: a creation as a link, and back.
import { beforeEach, describe, expect, it } from 'vitest';

import { setControl, startControls } from '../src/controls';
import { startingSettings } from '../src/settings';
import { current, decode, encode, MOST, settingsOf } from '../src/share';
import { hearing, language, poem, settings } from '../src/store';
import { previewOf, sharedIn } from '../../deploy/preview.js';
import { fixture, useFixture } from './site.fixture';

beforeEach(() => {
  useFixture();
  startControls();
  settings.value = startingSettings(fixture.mappings);
  poem.value = 'Tyger Tyger, burning bright,\n';
  hearing.value = 'A';
  language.value = 'en';
});

describe('a creation as a link carries', () => {
  it('a base poem name when matching a known preset, and nothing else at defaults', () => {
    expect(current()).toEqual({ v: 1, base: 'blake-tyger', style: 'default' });
  });

  it('the raw poem text when custom and not matching any known preset', () => {
    poem.value = 'A custom couplet in the morning,\nWithout a warning.\n';
    expect(current()).toEqual({ v: 1, poem: poem.value, style: 'default' });
  });

  it('a base and slice when matching a contiguous excerpt of a preset', () => {
    poem.value = 'I celebrate myself, and sing myself,\nAnd what I assume you shall assume,\nFor every atom belonging to me as good belongs to you.\n';
    expect(current()).toEqual({ v: 1, base: 'whitman-song-of-myself', slice: [0, 4], style: 'default' });
  });

  it('a base and patch when matching a small line variation of a preset', () => {
    poem.value = 'I celebrate myself, and dance myself,\nAnd what I assume you shall assume,\nFor every atom belonging to me as good belongs to you.\n\nI loafe and invite my soul,\n';
    expect(current()).toEqual({
      v: 1,
      base: 'whitman-song-of-myself',
      patch: { 0: 'I celebrate myself, and dance myself,' },
      style: 'default',
    });
  });

  it('only what differs: a control, a side\'s variant, the side heard, the language', () => {
    setControl('reverb', '0.6');
    settings.value = { ...settings.value, A: { ...settings.value.A, bass: 'feet' } };
    hearing.value = 'B';
    language.value = 'sl';
    expect(current()).toEqual({
      v: 1, base: 'blake-tyger', sound: { reverb: '0.6' }, A: { bass: 'feet' }, hear: 'B', lang: 'sl',
    });
  });

  it('as an ultra-short, URL-safe code that reads back with the resolved poem and base', async () => {
    // A clean preset link code is under 70 characters; with a control tweak it is under 100.
    const clean = await encode(current());
    expect(clean.length).toBeLessThan(70);

    setControl('scale', 'dorian');
    const shared = current();
    const code = await encode(shared);
    expect(code).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(code.length).toBeLessThan(100);
    const decoded = await decode(code);
    expect(decoded?.base).toBe('blake-tyger');
    expect(decoded?.poem).toBe('Tyger Tyger, burning bright,\n');
    expect(decoded?.sound).toEqual({ scale: 'dorian' });
  });

  it('round-trips custom poems cleanly', async () => {
    poem.value = 'Some custom text\nacross several lines\nto test inline sharing\n';
    const shared = current();
    const code = await encode(shared);
    const decoded = await decode(code);
    expect(decoded?.base).toBeUndefined();
    expect(decoded?.poem).toBe(poem.value);
  });
});

describe('a link read back', () => {
  it('sets its sound over the default palette and its sides over where each starts', () => {
    const { sound, sides } = settingsOf({ v: 1, poem: 'x', sound: { reverb: '0.6' }, A: { bass: 'feet' }, B: { pulse: 'steady' } });
    expect(sound.reverb).toBe('0.6');
    expect(sound.bass).toBe('gm_acoustic_bass');
    expect(sides.A).toMatchObject({ bass: 'feet', held: 'on' });
    expect(sides.B).toMatchObject({ pulse: 'steady', held: 'off' });
  });

  it('drops what this page does not know, as a stale save does', async () => {
    const code = await encode({ v: 1, poem: 'x', sound: { reverb: '0.6', script: 'alert(1)' }, A: { bass: 'polka', nope: 'on' }, lang: 'English!' } as never);
    const shared = await decode(code);
    expect(shared).toEqual({ v: 1, poem: 'x', sound: { reverb: '0.6' }, A: { bass: 'polka', nope: 'on' } });
    expect(settingsOf(shared!).sides.A.bass).toBe('height');
    expect(settingsOf(shared!).sides.A).not.toHaveProperty('nope');
  });

  it('is nothing for a link that is not one: junk, a later format, an unknown base, or one made to unpack to too much', async () => {
    expect(await decode('not-a-poem')).toBeNull();
    expect(await decode(await encode({ v: 2, poem: 'x' } as never))).toBeNull();
    expect(await decode(await encode({ v: 1, base: 'unknown-preset-xyz' }))).toBeNull();
    // A megabyte of one letter compresses to a few kilobytes and must not be unpacked whole.
    const bomb = await encode({ v: 1, poem: 'a'.repeat(1024 * 1024) });
    expect(bomb.length).toBeLessThan(MOST);
    expect(await decode(bomb)).toBeNull();
  });

  it('accepts legacy links carrying explicit poem without base', async () => {
    const legacy = await encode({ v: 1, poem: 'An old shared poem\n' });
    const decoded = await decode(legacy);
    expect(decoded).toEqual({ v: 1, poem: 'An old shared poem\n' });
  });
});

describe('the worker', () => {
  it('reads a link the page made, for its preview', async () => {
    poem.value = 'Tyger Tyger, burning bright,\nIn the forests of the night;\n';
    const code = await encode(current());
    const shared = await sharedIn(`https://prosody.rocks/?s=${code}`);
    expect(shared).toEqual({ poem: poem.value, style: 'default' });
    expect(previewOf(shared!).title).toBe('Tyger Tyger, burning bright, — prosody');
  });

  it('reads a base poem link for preview', async () => {
    const code = await encode({ v: 1, base: 'whitman-song-of-myself', slice: [0, 3] });
    const shared = await sharedIn(`https://prosody.rocks/?s=${code}`);
    expect(shared?.poem).toContain('I celebrate myself, and sing myself,');
    expect(previewOf(shared!).title).toBe('I celebrate myself, and sing myself, — prosody');
  });
});
