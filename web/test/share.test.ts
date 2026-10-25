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
  it('the poem, and nothing else while everything is where a visit starts', () => {
    expect(current()).toEqual({ v: 1, poem: 'Tyger Tyger, burning bright,\n', style: 'default' });
  });

  it('only what differs: a control, a side\'s variant, the side heard, the language', () => {
    setControl('reverb', '0.6');
    settings.value = { ...settings.value, A: { ...settings.value.A, bass: 'feet' } };
    hearing.value = 'B';
    language.value = 'sl';
    expect(current()).toEqual({
      v: 1, poem: 'Tyger Tyger, burning bright,\n', sound: { reverb: '0.6' }, A: { bass: 'feet' }, hear: 'B', lang: 'sl',
    });
  });

  it('as a short, URL-safe code that reads back as itself', async () => {
    setControl('scale', 'dorian');
    const shared = current();
    const code = await encode(shared);
    expect(code).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(code.length).toBeLessThan(120);
    expect(await decode(code)).toEqual(shared);
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

  it('is nothing for a link that is not one: junk, a later format, or one made to unpack to too much', async () => {
    expect(await decode('not-a-poem')).toBeNull();
    expect(await decode(await encode({ v: 2, poem: 'x' } as never))).toBeNull();
    // A megabyte of one letter compresses to a few kilobytes and must not be unpacked whole.
    const bomb = await encode({ v: 1, poem: 'a'.repeat(1024 * 1024) });
    expect(bomb.length).toBeLessThan(MOST);
    expect(await decode(bomb)).toBeNull();
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
});
