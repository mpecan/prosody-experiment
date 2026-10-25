// The master stage's ceiling, and the sounds loaded before a pattern plays.
import { describe, expect, it } from 'vitest';

import { CLIP_CEILING, CLIP_KNEE, CLIP_RANGE, softClipCurve, soundsOf } from '../src/strudel';

describe('the soft clipper', () => {
  const points = 4097;
  const curve = softClipCurve(points);
  // The input each point of the curve stands for: −CLIP_RANGE … CLIP_RANGE.
  const input = (i: number) => (i / (points - 1) * 2 - 1) * CLIP_RANGE;

  it('never lets anything past the ceiling', () => {
    expect(Math.max(...curve.map(Math.abs))).toBeLessThanOrEqual(CLIP_CEILING);
    // And comes within a hair of it at the far ends.
    expect(curve[points - 1]).toBeGreaterThan(CLIP_CEILING - 0.001);
    expect(curve[0]).toBeLessThan(-(CLIP_CEILING - 0.001));
  });

  it('is transparent below the knee', () => {
    // 0.5 in is at (0.5 / 4 + 1) / 2 of the way along: point 2304.
    expect(input(2304)).toBe(0.5);
    expect(curve[2304]).toBeCloseTo(0.5, 6);
    expect(curve[2048]).toBe(0);
  });

  it('bends smoothly above it, rising all the way', () => {
    for (let i = 2049; i < points; i++) expect(curve[i]!).toBeGreaterThanOrEqual(curve[i - 1]!);
    const atKnee = Math.round((CLIP_KNEE / CLIP_RANGE + 1) / 2 * (points - 1));
    expect(curve[atKnee]).toBeCloseTo(CLIP_KNEE, 2);
  });
});

describe('the sounds a pattern asks for', () => {
  it('are each note of each pitched sound and each drum of each bank, rests and lengths aside', () => {
    const code = [
      'stack(',
      '  note("<36 38@2 ~ [36,43]>").sound("gm_acoustic_bass"),',
      '  s("bd ~ hh@3 bd").bank("tr909"),',
      ')',
    ].join('\n');
    expect(soundsOf(code)).toEqual([
      { s: 'gm_acoustic_bass', note: 36 },
      { s: 'gm_acoustic_bass', note: 38 },
      { s: 'gm_acoustic_bass', note: 43 },
      { s: 'bd', bank: 'tr909' },
      { s: 'hh', bank: 'tr909' },
    ]);
  });
});
