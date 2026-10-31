// The Prosody player's page side (src/player): the note records the synth reads, the clock the page
// draws by, the worklet's mixing, and reading a soundfont file. Expected values are worked out by
// hand from the inputs; the record layout is the one crates/prosody-synth-wasm pins on its side.
import { describe, expect, it } from 'vitest';

import type { Performance, PerformedNote } from '../src/answers';
import { indexed, marksAt, notesBetween, pieceTime, settle, within } from '../src/player/clock';
import { FADE, GLIDE, softClip, StemQueue } from '../src/player/mixer';
import { barStarts, records, sampledOf, STRIDE } from '../src/player/notes';
import { parseZones, zonesFor } from '../src/player/samples';

function note(overrides: Partial<PerformedNote>): PerformedNote {
  return {
    from: 0, to: 3, channel: 'melody', lane: 'rhyme', slot: 0, onset: 0.25, length: 0.5,
    sound: { kind: 'wave', wave: 'sawtooth' }, pitch: 64, gain: 0.8, attack: null, release: 0.1, lpf: 3000, postgain: 1,
    ...overrides,
  };
}

function performance(notes: PerformedNote[], seconds = 2): Performance {
  return {
    seconds, cpm: 30, bars: [{ slot: 0, at: 0 }, { slot: 1, at: 1 }], notes,
    marks: [
      { from: 0, to: 3, lane: 'pulse', slot: 0, onset: 0, length: 0.5, fresh: true, melody: false, bass: false },
      { from: 4, to: 7, lane: 'pulse', slot: 0, onset: 0.4, length: 0.2, fresh: true, melody: true, bass: false },
      { from: 8, to: 9, lane: 'pulse', slot: 1, onset: 1.5, length: 0.25, fresh: false, melody: false, bass: false },
    ],
  };
}

describe('note records', () => {
  it('lays a note out as the synth reads it', () => {
    // The same record as crates/prosody-synth-wasm's `record(3, 2, 1, 64)`: tag 3, melody (2), an
    // oscillator (2), sawtooth (1), at 0.25 s for 0.5 s, MIDI 64, gain 0.8, postgain 1, no attack,
    // release 0.1, cutoff 3 kHz.
    const piece = performance([note({}), note({}), note({}), note({})]);
    const out = records(piece, new Map());
    expect(out.length).toBe(4 * STRIDE);
    const fourth = [...out.slice(3 * STRIDE, 4 * STRIDE)];
    expect(fourth.slice(0, 9)).toEqual([3, 2, 2, 1, 0.25, 0.5, 64, 0.8, 1]);
    expect(Number.isNaN(fourth[9])).toBe(true);
    expect(fourth.slice(10)).toEqual([0.1, 3000]);
  });

  it('numbers sampled sounds by the ids given, and a drum has no pitch', () => {
    const drum = note({ channel: 'drums', sound: { kind: 'drum', bank: 'uzu', piece: 'bd' }, pitch: null });
    const font = note({ channel: 'bass', sound: { kind: 'font', name: 'gm_acoustic_bass', variant: 1, file: '0320_x' }, pitch: 36 });
    const out = records(performance([drum, font]), new Map([['drum:uzu/bd', 5], ['font:0320_x', 2]]));
    expect([out[1], out[2], out[3]]).toEqual([0, 0, 5]);
    expect(Number.isNaN(out[6])).toBe(true);
    expect([out[STRIDE + 1], out[STRIDE + 2], out[STRIDE + 3]]).toEqual([1, 1, 2]);
  });

  it('gives a sound with no number one nothing is loaded as, so the synth counts it missing', () => {
    const drum = note({ sound: { kind: 'drum', bank: 'uzu', piece: 'sd' }, pitch: null });
    expect(records(performance([drum]), new Map())[3]).toBe(4095);
  });

  it('lists each sampled sound once, with the keys its notes play', () => {
    const font = (pitch: number) => note({ sound: { kind: 'font', name: 'gm_piano', variant: 1, file: 'piano' }, pitch });
    const sounds = sampledOf(performance([
      font(60), font(60.4), font(72), note({}),
      note({ sound: { kind: 'drum', bank: 'uzu', piece: 'hh' }, pitch: null }),
    ]));
    expect([...sounds.keys()]).toEqual(['font:piano', 'drum:uzu/hh']);
    expect([...sounds.get('font:piano')!.keys]).toEqual([60, 72]);
    expect([...barStarts(performance([]))]).toEqual([0, 1]);
  });
});

describe('the clock', () => {
  it('maps a played frame to its piece, the latest placement that has begun', () => {
    const a = performance([]), b = performance([]);
    const placements = [
      { outStart: 0, pieceStart: 0, performance: a },
      { outStart: 96_000, pieceStart: 48_000, performance: b },
    ];
    // At 48 kHz: frame 24 000 is half a second into A; frame 120 000 is B's frame 48 000 + 24 000,
    // 1.5 s into B. Before the first placement, nothing.
    expect(pieceTime(24_000, placements, 48_000)).toEqual({ placement: placements[0], time: 0.5 });
    expect(pieceTime(120_000, placements, 48_000)).toEqual({ placement: placements[1], time: 1.5 });
    expect(pieceTime(5, [{ outStart: 10, pieceStart: 0, performance: a }], 48_000)).toBeNull();
    expect(within(4.5, 2)).toBe(0.5);
  });

  it('keeps the placement playing and those still to come, and lets the rest go', () => {
    const a = performance([]), b = performance([]), c = performance([]), d = performance([]);
    // A from the top; edits B and C both placed at the bar at 96 000 (C, sent later, is the one
    // heard from there); D at the bar after, 192 000.
    const placements = [
      { outStart: 0, pieceStart: 0, performance: a },
      { outStart: 96_000, pieceStart: 0, performance: b },
      { outStart: 96_000, pieceStart: 0, performance: c },
      { outStart: 192_000, pieceStart: 0, performance: d },
    ];
    // Before anything plays, and while A plays, all are kept.
    expect(settle(placements, -10)).toEqual(placements);
    expect(settle(placements, 50_000)).toEqual(placements);
    // From 96 000, C plays: A is done with, and B never will be heard.
    const later = settle(placements, 100_000);
    expect(later.map((p) => p.performance)).toEqual([c, d]);
    expect(pieceTime(100_000, later, 48_000)?.placement.performance).toBe(c);
    // From 192 000 only D is left; and settling again changes nothing.
    expect(settle(later, 200_000).map((p) => p.performance)).toEqual([d]);
    expect(settle(settle(later, 200_000), 200_000).map((p) => p.performance)).toEqual([d]);
  });

  it('lights the marks sounding at a time, any time round', () => {
    const piece = indexed(performance([]));
    // At 0.45 s the first mark (0–0.5) and the second (0.4–0.6) are lit; at 3.6 s, which is 1.6 s
    // into the second time round, the third (1.5–1.75).
    expect(marksAt(piece, 0.45).map((m) => m.index)).toEqual([0, 1]);
    expect(marksAt(piece, 0.55).map((m) => m.index)).toEqual([1]);
    expect(marksAt(piece, 3.6).map((m) => m.index)).toEqual([2]);
    expect(marksAt(piece, 1.0)).toEqual([]);
  });

  it('finds the notes in a window, from each time round it spans', () => {
    // A 2 s piece with notes at 0.25 (0.5 long) and 1.8 (0.1 long). The window 1.7–2.5 holds the
    // second note and the first note of the next time round, at 2.25.
    const piece = indexed(performance([note({ onset: 0.25 }), note({ onset: 1.8, length: 0.1 })]));
    const found = notesBetween(piece, 1.7, 2.5).map(({ begin, end }) => [begin, end]);
    expect(found).toEqual([[1.8, 1.9000000000000001], [2.25, 2.75]]);
    // A note still sounding when the window opens is in it.
    expect(notesBetween(piece, 0.7, 0.8).map(({ begin }) => begin)).toEqual([0.25]);
  });
});

describe('the worklet mixer', () => {
  /** A block of `frames` frames whose stem `stem` holds `left` and `right` throughout. */
  function block(frame: number, frames: number, stem: number, left: number, right: number) {
    const data = new Float32Array(8 * frames);
    data.fill(left, stem * 2 * frames, stem * 2 * frames + frames);
    data.fill(right, stem * 2 * frames + frames, (stem + 1) * 2 * frames);
    return { frame, frames, data };
  }

  it('mixes the stems at their gains and counts from the first block', () => {
    const queue = new StemQueue();
    queue.push(block(1000, 4, 0, 0.25, 0.5));
    const left = new Float32Array(2), right = new Float32Array(2);
    queue.mix(left, right, [1, 1, 1, 1]);
    expect([...left]).toEqual([0.25, 0.25]);
    expect([...right]).toEqual([0.5, 0.5]);
    expect(queue.played).toBe(1002);
    expect(queue.underruns).toBe(0);
  });

  it('glides a stem to a new gain rather than stepping, and starts at the gains asked for', () => {
    const queue = new StemQueue();
    // A stem muted before play is silent from its first frame: nothing sounded to glide from.
    queue.push(block(0, 1000, 0, 0.5, 0.5));
    const two = new Float32Array(2), other = new Float32Array(2);
    queue.mix(two, other, [0, 1, 1, 1]);
    expect([...two]).toEqual([0, 0]);
    // Unmuted, it rises by 1/GLIDE a frame: 0.5 × 1/256, then × 2/256.
    queue.mix(two, other, [1, 1, 1, 1]);
    expect(two[0]).toBeCloseTo(0.5 / GLIDE, 9);
    expect(two[1]).toBeCloseTo(0.5 * 2 / GLIDE, 9);
    // After GLIDE frames it is all the way up, and stays there.
    const long = new Float32Array(GLIDE), side = new Float32Array(GLIDE);
    queue.mix(long, side, [1, 1, 1, 1]);
    expect(long[GLIDE - 3]).toBeCloseTo(0.5, 9);
    expect(long[GLIDE - 1]).toBeCloseTo(0.5, 9);
    // Muted again: down by 1/GLIDE a frame, no step to 0.
    queue.mix(two, other, [0, 1, 1, 1]);
    expect(two[0]).toBeCloseTo(0.5 * (GLIDE - 1) / GLIDE, 9);
    expect(two[1]).toBeCloseTo(0.5 * (GLIDE - 2) / GLIDE, 9);
  });

  it('fades what was queued out over FADE frames when it is flushed', () => {
    const queue = new StemQueue();
    queue.push(block(0, 1000, 1, 0.4, 0.2));
    const two = new Float32Array(2), other = new Float32Array(2);
    queue.mix(two, other, [1, 1, 1, 1]);
    queue.flush();
    // The next FADE frames of the dropped music, falling linearly: frame k at (FADE − k)/FADE.
    const out = new Float32Array(FADE + 4), outRight = new Float32Array(FADE + 4);
    queue.mix(out, outRight, [1, 1, 1, 1]);
    expect(out[0]).toBeCloseTo(0.4, 6);
    expect(outRight[0]).toBeCloseTo(0.2, 6);
    expect(out[FADE / 2]).toBeCloseTo(0.4 * 0.5, 6);
    expect(out[FADE - 1]).toBeCloseTo(0.4 / FADE, 6);
    expect([...out.slice(FADE)]).toEqual([0, 0, 0, 0]);
    // Silence after a flush is no shortfall, and the count is not moved by the tail.
    expect(queue.underruns).toBe(0);
    queue.push(block(5000, 4, 0, 0.1, 0.1));
    expect(queue.played).toBe(5000);
  });

  it('carries across blocks and counts a quantum it could not fill', () => {
    const queue = new StemQueue();
    queue.push(block(0, 3, 1, 0.1, 0.1));
    queue.push(block(3, 3, 1, 0.2, 0.2));
    const left = new Float32Array(4), right = new Float32Array(4);
    queue.mix(left, right, [1, 1, 1, 1]);
    expect([...left].map((x) => Math.round(x * 10) / 10)).toEqual([0.1, 0.1, 0.1, 0.2]);
    expect(queue.queued).toBe(2);
    queue.mix(left, right, [1, 1, 1, 1]);
    expect([...left].map((x) => Math.round(x * 10) / 10)).toEqual([0.2, 0.2, 0, 0]);
    expect(queue.underruns).toBe(1);
    expect(queue.played).toBe(6);
  });

  it('is not short of anything before the first block, nor after a flush', () => {
    const queue = new StemQueue();
    const left = new Float32Array(4), right = new Float32Array(4);
    queue.mix(left, right, [1, 1, 1, 1]);
    queue.push(block(50, 4, 0, 0.1, 0.1));
    queue.flush();
    queue.mix(left, right, [1, 1, 1, 1]);
    expect(queue.underruns).toBe(0);
    // After a flush the next block says where the count starts again.
    queue.push(block(9000, 4, 0, 0.1, 0.1));
    expect(queue.played).toBe(9000);
  });

  it('clips as the page master does', () => {
    // Below the knee untouched; one bend above it, knee + bend·tanh(1) = 0.6 + 0.35 × 0.761594.
    expect(softClip(0.5)).toBe(0.5);
    expect(softClip(0.95)).toBeCloseTo(0.866558, 6);
    expect(softClip(-0.95)).toBeCloseTo(-0.866558, 6);
    expect(softClip(100)).toBe(softClip(4));
    expect(softClip(100)).toBeLessThan(0.95);
  });
});

describe('a soundfont file', () => {
  // The shape of a webaudiofont file, with made-up values: two zones with samples, one without.
  const file = `console.log('load _tone_x');
var _tone_x={
	zones:[
		{
			midi:32
			,originalPitch:2800
			,keyRangeLow:0
			,keyRangeHigh:40
			,loopStart:100
			,loopEnd:900
			,coarseTune:0
			,fineTune:-50
			,sampleRate:44100
			,ahdsr:true
			,file:'AAAA'
		}
		,{
			midi:32
			,originalPitch:6000
			,keyRangeLow:41
			,keyRangeHigh:70
			,loopStart:0
			,loopEnd:0
			,coarseTune:2
			,fineTune:0
			,sampleRate:22050
			,file:'QUJD+/8='
		}
		,{
			midi:32
			,originalPitch:7200
			,keyRangeLow:71
			,keyRangeHigh:127
		}
	]
};`;

  it('reads each zone with a sample, its root less its tuning', () => {
    const zones = parseZones(file);
    expect(zones.length).toBe(2);
    // 2 800 cents is MIDI 28; fine-tuned −50 cents it sounds as 28.5 at its own rate.
    expect(zones[0]).toEqual({ low: 0, high: 40, root: 28.5, loopStart: 100, loopEnd: 900, sampleRate: 44100, file: 'AAAA' });
    // 6 000 cents, coarse-tuned up 2 semitones: it sounds as 58.
    expect(zones[1]).toMatchObject({ low: 41, high: 70, root: 58, sampleRate: 22050, file: 'QUJD+/8=' });
  });

  it('loads only the zones the keys need, by the synth\'s rule', () => {
    const zones = parseZones(file);
    expect(zonesFor(zones, [30])).toEqual([zones[0]]);
    expect(zonesFor(zones, [30, 50, 64])).toEqual(zones);
    // Key 100 is in no zone with a sample: the nearest (41–70) plays it.
    expect(zonesFor(zones, [100])).toEqual([zones[1]]);
  });
});
