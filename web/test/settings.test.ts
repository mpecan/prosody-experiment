import { beforeEach, describe, expect, it } from 'vitest';

import { loadSettings, restore, startingSettings } from '../src/settings';
import { fixture, useFixture } from './site.fixture';

beforeEach(() => { useFixture(); });

describe('the two settings being compared', () => {
  it('starts A at every default and B at what preceded each mapping', () => {
    const { A, B } = startingSettings(fixture.mappings);
    expect(A).toEqual({ held: 'on', bass: 'height', reading: 'breath', pulse: 'steady', 'long-stanzas': 'erode' });
    // B at each mapping's first variant — off, or for the pulse, which has none, elastic — except
    // the long stanzas, whose first variant is the quadratic arrangement erosion removes.
    expect(B).toEqual({ held: 'off', bass: 'off', reading: 'off', pulse: 'elastic', 'long-stanzas': 'erode' });
  });

  it('reads today\'s saved shape, and falls back for anything it no longer knows', () => {
    const start = startingSettings(fixture.mappings).A;
    const saved = { bass: 'feet', pulse: 'wobbly', polka: 'on' };
    expect(restore(fixture.mappings, start, saved, false)).toEqual({
      held: 'on', bass: 'feet', reading: 'breath', pulse: 'steady', 'long-stanzas': 'erode',
    });
  });

  it('reads the shape from before the selectors: the names listed off, the rest at their defaults', () => {
    const start = startingSettings(fixture.mappings).A;
    expect(restore(fixture.mappings, start, ['held', 'bass'], false)).toEqual({
      held: 'off', bass: 'off', reading: 'breath', pulse: 'steady', 'long-stanzas': 'erode',
    });
    // And the old reading box, unticked, is the reading off.
    expect(restore(fixture.mappings, start, [], true).reading).toBe('off');
  });

  it('loads what this browser saved, or starts afresh from a save it cannot read', () => {
    localStorage.setItem('prosody.settings', JSON.stringify({ A: { bass: 'syllables' }, B: { pulse: 'steady' } }));
    const loaded = loadSettings();
    expect(loaded.A.bass).toBe('syllables');
    expect(loaded.B.pulse).toBe('steady');
    expect(loaded.B.held).toBe('off');
    localStorage.setItem('prosody.settings', '{not json');
    expect(loadSettings()).toEqual(startingSettings(fixture.mappings));
    localStorage.clear();
    localStorage.setItem('prosody.read', 'off');
    expect(loadSettings().A.reading).toBe('off');
  });
});
