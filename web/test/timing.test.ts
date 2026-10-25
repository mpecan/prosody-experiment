// The reading's clock: bars on the audio clock, and how a line too long for its bar is refitted.
import { describe, expect, it } from 'vitest';

import { barAt, barSeconds, barTime, fromWhere, refitSpeed, renderKey } from '../src/reading';
import { barsOf } from '../src/watch';

describe('the music\'s bars', () => {
  // Bars of one, one and two cycles at 30 cycles a minute: two seconds a cycle.
  const seconds = barSeconds({ bars: [0, 1, 2, 4], cpm: 30 });

  it('are where the reading says, in seconds', () => {
    expect(seconds).toEqual([0, 2, 4, 8]);
  });

  it('repeat round the loop on the audio clock', () => {
    const chain = { seconds, origin: 10 };
    expect(barTime(chain, 0)).toBe(10);
    expect(barTime(chain, 2)).toBe(14);
    // The next round: its first bar, then its second.
    expect(barTime(chain, 3)).toBe(18);
    expect(barTime(chain, 4)).toBe(20);
    // The round before.
    expect(barTime(chain, -1)).toBe(6);
  });

  it('are found from a time, in any round', () => {
    const chain = { seconds, origin: 10 };
    expect(barAt(chain, 10)).toBe(0);
    expect(barAt(chain, 15)).toBe(2);
    expect(barAt(chain, 19)).toBe(3);
    expect(barAt(chain, 9)).toBe(-1);
  });

  it('come from a trace the same way, for the roll', () => {
    expect(barsOf({ pattern: '', bars: [0, 1, 3], cpm: 30, marks: [] })).toEqual([0, 2, 6]);
    expect(barsOf({ pattern: '', bars: [0], cpm: 30, marks: [] })).toBeNull();
    expect(barsOf(null)).toBeNull();
  });
});

describe('a line too long for its bar', () => {
  it('is rendered again just fast enough to fit', () => {
    // Two seconds of speech for one and a half of room, at speed 1: 4/3 as fast.
    expect(refitSpeed(1, 2, 1.5)).toBeCloseTo(4 / 3, 6);
  });

  it('is left alone when it fits, or overruns by no more than 20 ms', () => {
    expect(refitSpeed(1, 1.4, 1.5)).toBeNull();
    expect(refitSpeed(1, 1.51, 1.5)).toBeNull();
  });

  it('is never made to gabble: no faster than twice its speed', () => {
    expect(refitSpeed(1, 10, 1)).toBe(2);
  });
});

describe('the order lines are wanted in', () => {
  it('runs from where the reading is round to the line before it', () => {
    expect(fromWhere(['a', 'b', 'c', 'd'], 2)).toEqual(['c', 'd', 'a', 'b']);
    expect(fromWhere(['a', 'b'], 9)).toEqual(['a', 'b']);
    expect(fromWhere(['a', 'b'], -3)).toEqual(['a', 'b']);
  });

  it('keys a render by engine, voice, speed to three places, and text', () => {
    expect(renderKey('kokoro', 'af_heart', 1.0004, 'a line')).toBe('kokoro|af_heart|1.000|a line');
  });
});
