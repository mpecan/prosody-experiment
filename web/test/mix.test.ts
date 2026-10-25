// Mute and solo, and the lane roll's geometry.
import { beforeEach, describe, expect, it } from 'vitest';

import { audible, played } from '../src/mix';
import { voiceLevel } from '../src/reading';
import { laneRects } from '../src/roll';
import { muted, readingSettings, soloed } from '../src/store';
import type { Hap } from '../src/strudel/globals';
import { barLines } from '../src/watch';
import { fixture, useFixture } from './site.fixture';

beforeEach(() => {
  useFixture();
  muted.value = {};
  soloed.value = {};
});

describe('mute and solo', () => {
  it('hear every channel until one is muted', () => {
    expect(audible('bass')).toBe(true);
    muted.value = { bass: true };
    expect(audible('bass')).toBe(false);
    expect(audible('drums')).toBe(true);
  });

  it('hear only the soloed channels once any is soloed, muted or not', () => {
    soloed.value = { bass: true, voice: true };
    muted.value = { bass: true };
    expect(audible('bass')).toBe(true);
    expect(audible('voice')).toBe(true);
    expect(audible('drums')).toBe(false);
    expect(audible('melody')).toBe(false);
  });

  it('silence an event of a channel not heard, and mark it, leaving every other as it was', () => {
    muted.value = { drums: true };
    const kick = { s: 'bd', gain: 0.28, color: '#c9a6ff' };
    const hat = { s: 'hh', gain: 0.28, color: '#6fd6c5' };
    const note = { note: 36, gain: 0.32, color: '#ffb86b' };
    const cue = { n: 3 };
    // Both drum layers play through the drums.
    expect(played(kick)).toEqual({ s: 'bd', gain: 0, color: '#c9a6ff', muted: true });
    expect(played(hat)).toEqual({ s: 'hh', gain: 0, color: '#6fd6c5', muted: true });
    expect(played(note)).toBe(note);
    // A reading cue carries no colour: the mixer never touches it.
    expect(played(cue)).toBe(cue);
  });

  it('take the voice down to nothing, and back to its fader', () => {
    readingSettings.value = { ...readingSettings.value, level: 1.4 };
    expect(voiceLevel()).toBe(1.4);
    muted.value = { voice: true };
    expect(voiceLevel()).toBe(0);
    muted.value = {};
    soloed.value = { bass: true };
    expect(voiceLevel()).toBe(0);
  });
});

const hap = (begin: number, end: number, value: Record<string, unknown>): Hap =>
  ({ whole: { begin, end }, value, hasOnset: () => true }) as unknown as Hap;

describe('the lane roll', () => {
  // Five lanes of 20 px in a 100 px canvas; a 20 px gutter, 400 px for 4 s around the playhead.
  const frame = { width: 420, height: 100, gutter: 20 };

  it('puts each event in its layer\'s lane, across by its time, lit while it sounds', () => {
    const rects = laneRects([hap(9, 10, { color: '#c9a6ff' }), hap(10.5, 11.5, { color: '#ff7aa8' })], fixture.layers, 10, [-2, 2], frame);
    // The kick: from 9 s, a second before the playhead at 10 s, so 100 px in from the gutter;
    // 100 px long less a pixel's gap; in lane 0, on its middle line (a third of 20 px tall).
    expect(rects[0]).toMatchObject({ lane: 0, x: 120, width: 99, active: false, muted: false });
    expect(rects[0]!.y).toBeCloseTo(10 - 20 / 6, 6);
    // The rhyme note: 50 px after the playhead (at 220 + 50), in lane 3, not yet sounding.
    expect(rects[1]).toMatchObject({ lane: 3, x: 270, active: false });
    const now = laneRects([hap(9.5, 10.5, { color: '#ffb86b' })], fixture.layers, 10, [-2, 2], frame);
    expect(now[0]).toMatchObject({ lane: 2, active: true });
  });

  it('sets a higher note higher in its lane, and leaves out what is not a layer or not in view', () => {
    const rects = laneRects([
      hap(10, 11, { color: '#ffb86b', note: 36 }),
      hap(11, 12, { color: '#ffb86b', note: 48 }),
      hap(10, 11, { n: 2 }),
      hap(20, 21, { color: '#ffb86b', note: 40 }),
    ], fixture.layers, 10, [-2, 2], frame);
    expect(rects).toHaveLength(2);
    expect(rects[1]!.y).toBeLessThan(rects[0]!.y);
    // Both inside the bass lane, 40–60 px.
    for (const rect of rects) {
      expect(rect.y).toBeGreaterThanOrEqual(40);
      expect(rect.y + rect.height).toBeLessThanOrEqual(60);
    }
  });

  it('draws a muted event, marked', () => {
    expect(laneRects([hap(10, 11, { color: '#8fb3ff', muted: true })], fixture.layers, 10, [-2, 2], frame)[0]!.muted).toBe(true);
  });

  it('marks the bars where they fall, round the loop', () => {
    // Bars at 0, 2 and 5 s in a 6 s piece, seen from 4 s to 13 s: 5, 6 (the loop's 0), 8, 11 and 12.
    expect(barLines([0, 2, 5, 6], 4, 13)).toEqual([1, 2, 4, 7, 8]);
  });
});
