import { beforeEach, describe, expect, it } from 'vitest';

import { choose, loadSettings, putBack, restore, startingSettings, suggest } from '../src/settings';
import { settings, suggested } from '../src/store';
import { fixture, useFixture } from './site.fixture';

beforeEach(() => { useFixture(); });

describe('a style\'s suggestions', () => {
  beforeEach(() => {
    settings.value = startingSettings(fixture.mappings);
    suggested.value = {};
  });

  it('are set on A only, each remembered with what A had', () => {
    expect(suggest('jazz', { bass: 'syllables', pulse: 'steady', polka: 'on' })).toEqual(['bass']);
    // The pulse was steady already, and there is no polka: only the bass changed.
    expect(settings.value.A.bass).toBe('syllables');
    expect(settings.value.B.bass).toBe('off');
    expect(suggested.value).toEqual({ bass: { from: 'height', to: 'syllables', style: 'jazz' } });
  });

  it('keep what A had before any style, through a second one', () => {
    suggest('jazz', { bass: 'syllables' });
    suggest('hymn', { bass: 'feet' });
    expect(suggested.value.bass).toEqual({ from: 'height', to: 'feet', style: 'hymn' });
    putBack();
    expect(settings.value.A.bass).toBe('height');
    expect(suggested.value).toEqual({});
  });

  it('become the writer\'s own once A is set by hand', () => {
    suggest('jazz', { bass: 'syllables', pulse: 'elastic' });
    choose('A', 'bass', 'feet');
    expect(Object.keys(suggested.value)).toEqual(['pulse']);
    putBack();
    expect(settings.value.A).toMatchObject({ bass: 'feet', pulse: 'steady' });
  });
});

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
