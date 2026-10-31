// The Prosody player's page side (src/player): the note records the synth reads, the clock the page
// draws by, the worklet's mixing, and reading a soundfont file. Expected values are worked out by
// hand from the inputs; the record layouts are the ones crates/prosody-player pins on its side.
import { describe, expect, it } from 'vitest';

import type { Performance, PerformedNote } from '../src/answers';
import { indexed, marksAt, notesBetween, pieceTime, settle, within } from '../src/player/clock';
import { FADE, GLIDE, Glue, glueCurve, MAKEUP, PER_FRAME, softClip, StemQueue } from '../src/player/mixer';
import { barStarts, CUE_STRIDE, cueRecords, records, sampledOf, STRIDE } from '../src/player/notes';
import { zonesFor } from '../src/player/samples';

function note(overrides: Partial<PerformedNote>): PerformedNote {
  return {
    from: 0, to: 3, channel: 'melody', lane: 'rhyme', slot: 0, onset: 0.25, length: 0.5,
    sound: { kind: 'wave', wave: 'sawtooth' }, pitch: 64, gain: 0.8, attack: null, release: 0.1, lpf: 3000, postgain: 1,
    crush: null, vowel: null, shape: null, room: 0, roomsize: 2.5, delay: null, compressor: null,
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
    cues: [], unread: [], natural_unread: [],
  };
}

describe('note records', () => {
  it('lays a note out as the synth reads it', () => {
    // The same record as crates/prosody-player's `records` test `record(3, 2, 1, 64)`: tag 3, melody (2), an
    // oscillator (2), sawtooth (1), at 0.25 s for 0.5 s, MIDI 64, gain 0.8, postgain 1, no attack,
    // release 0.1, cutoff 3 kHz; no crush, vowel or shape (NaN), no room (0, size 2.5), no delay
    // (NaN, 0, 0), no compressor (NaN, 1).
    const piece = performance([note({}), note({}), note({}), note({})]);
    const out = records(piece, new Map());
    expect(out.length).toBe(4 * STRIDE);
    const fourth = [...out.slice(3 * STRIDE, 4 * STRIDE)];
    expect(fourth.slice(0, 9)).toEqual([3, 2, 2, 1, 0.25, 0.5, 64, 0.8, 1]);
    expect(Number.isNaN(fourth[9])).toBe(true);
    expect(fourth.slice(10, 12)).toEqual([0.1, 3000]);
    expect(fourth.slice(12, 15).every(Number.isNaN)).toBe(true);
    expect(fourth.slice(15, 17)).toEqual([0, 2.5]);
    expect(Number.isNaN(fourth[17])).toBe(true);
    expect(fourth.slice(18, 20)).toEqual([0, 0]);
    expect(Number.isNaN(fourth[20])).toBe(true);
    expect(fourth[21]).toBe(1);
  });

  it('writes the effects as the synth reads them', () => {
    // The same effects as crates/prosody-player's `a_records_effects_are_read_field_by_field`:
    // 8 bits, /o/ (3), shape 0.1, room 0.35 at 2.5 s, an echo of 0.2 at 0.3 s and 0.35, −24 dB at 7:1.
    const out = records(performance([note({
      crush: 8, vowel: 'o', shape: 0.1, room: 0.35, roomsize: 2.5,
      delay: { level: 0.2, time: 0.3, feedback: 0.35 }, compressor: { threshold: -24, ratio: 7 },
    })]), new Map());
    expect([...out.slice(12, 22)]).toEqual([8, 3, 0.1, 0.35, 2.5, 0.2, 0.3, 0.35, -24, 7]);
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

describe('cue records', () => {
  it('lays a cue out as the player reads it', () => {
    // The same records as crates/prosody-player's `a_cue_is_read_field_by_field`: line 3 of stanza 1,
    // pausing 0.5 s, fitted in bar 2 at 4 s and under `breath` from bar 3 at 6 s; then line 4, read
    // only at the natural pace, following it (no fitted bar, no natural bar: NaN).
    const out = cueRecords([
      { line: 3, stanza: 1, from: 0, to: 9, pace: 1, pause: 0.5, fitted: { slot: 2, at: 4, rate: 1 }, natural: { onset: 'breath', slot: 3, at: 6 } },
      { line: 4, stanza: 1, from: 10, to: 19, pace: 1, pause: 0.25, fitted: null, natural: { onset: 'follows', slot: null, at: null } },
    ]);
    expect(out.length).toBe(2 * CUE_STRIDE);
    expect([...out]).toEqual([3, 1, 0.5, 2, 4, 1, 6, 4, 1, 0.25, NaN, NaN, 2, NaN]);
  });

  it('writes no natural onset for a line the natural pace does not read, and `at` as 0', () => {
    const out = cueRecords([
      { line: 0, stanza: 0, from: 0, to: 1, pace: 1, pause: 0, fitted: { slot: 0, at: 0, rate: 1 }, natural: null },
      { line: 1, stanza: 0, from: 2, to: 3, pace: 1, pause: 0, fitted: null, natural: { onset: 'at', slot: 1, at: 2 } },
    ]);
    expect(Number.isNaN(out[5])).toBe(true);
    expect([out[CUE_STRIDE + 5], out[CUE_STRIDE + 6]]).toEqual([0, 2]);
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
  /** A block of `frames` frames whose stem `stem` holds `left` and `right` throughout, at `duck`. */
  function block(frame: number, frames: number, stem: number, left: number, right: number, duck = 1) {
    const data = new Float32Array(PER_FRAME * frames);
    data.fill(left, stem * 2 * frames, stem * 2 * frames + frames);
    data.fill(right, stem * 2 * frames + frames, (stem + 1) * 2 * frames);
    data.fill(duck, 10 * frames, 11 * frames);
    return { frame, frames, data };
  }
  // Every level below is far under the glue's −6 dB threshold, so the glue is its makeup alone:
  // ×MAKEUP, 10^(1.575/20) = 1.19881, from its first frame.
  const ALL = [1, 1, 1, 1, 1];

  it('mixes the stems at their gains and counts from the first block', () => {
    const queue = new StemQueue();
    queue.push(block(1000, 4, 0, 0.1, 0.2));
    const left = new Float32Array(2), right = new Float32Array(2);
    queue.mix(left, right, ALL);
    expect(left[0]).toBeCloseTo(0.1 * MAKEUP, 6);
    expect(left[1]).toBeCloseTo(0.1 * MAKEUP, 6);
    expect(right[0]).toBeCloseTo(0.2 * MAKEUP, 6);
    expect(queue.played).toBe(1002);
    expect(queue.underruns).toBe(0);
  });

  it('glides a stem to a new gain rather than stepping, and starts at the gains asked for', () => {
    const queue = new StemQueue();
    // A stem muted before play is silent from its first frame: nothing sounded to glide from.
    queue.push(block(0, 1000, 0, 0.2, 0.2));
    const two = new Float32Array(2), other = new Float32Array(2);
    queue.mix(two, other, [0, 1, 1, 1, 1]);
    expect([...two]).toEqual([0, 0]);
    // Unmuted, it rises by 1/GLIDE a frame: 0.2 × 1/256, then × 2/256 (each through the makeup).
    queue.mix(two, other, ALL);
    expect(two[0]).toBeCloseTo(0.2 / GLIDE * MAKEUP, 7);
    expect(two[1]).toBeCloseTo(0.2 * 2 / GLIDE * MAKEUP, 7);
    // After GLIDE frames it is all the way up, and stays there.
    const long = new Float32Array(GLIDE), side = new Float32Array(GLIDE);
    queue.mix(long, side, ALL);
    expect(long[GLIDE - 3]).toBeCloseTo(0.2 * MAKEUP, 7);
    expect(long[GLIDE - 1]).toBeCloseTo(0.2 * MAKEUP, 7);
    // Muted again: down by 1/GLIDE a frame, no step to 0.
    queue.mix(two, other, [0, 1, 1, 1, 1]);
    expect(two[0]).toBeCloseTo(0.2 * (GLIDE - 1) / GLIDE * MAKEUP, 7);
    expect(two[1]).toBeCloseTo(0.2 * (GLIDE - 2) / GLIDE * MAKEUP, 7);
  });

  it('fades what was queued out over FADE frames when it is flushed', () => {
    const queue = new StemQueue();
    queue.push(block(0, 1000, 1, 0.4, 0.2));
    const two = new Float32Array(2), other = new Float32Array(2);
    queue.mix(two, other, ALL);
    queue.flush();
    // The next FADE frames of the dropped music, falling linearly: frame k at (FADE − k)/FADE.
    const out = new Float32Array(FADE + 4), outRight = new Float32Array(FADE + 4);
    queue.mix(out, outRight, ALL);
    expect(out[0]).toBeCloseTo(0.4 * MAKEUP, 6);
    expect(outRight[0]).toBeCloseTo(0.2 * MAKEUP, 6);
    expect(out[FADE / 2]).toBeCloseTo(0.4 * 0.5 * MAKEUP, 6);
    expect(out[FADE - 1]).toBeCloseTo(0.4 / FADE * MAKEUP, 6);
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
    queue.mix(left, right, ALL);
    expect([...left].map((x) => Math.round(x / MAKEUP * 10) / 10)).toEqual([0.1, 0.1, 0.1, 0.2]);
    expect(queue.queued).toBe(2);
    queue.mix(left, right, ALL);
    expect([...left].map((x) => Math.round(x / MAKEUP * 10) / 10)).toEqual([0.2, 0.2, 0, 0]);
    expect(queue.underruns).toBe(1);
    expect(queue.played).toBe(6);
  });

  it('is not short of anything before the first block, nor after a flush', () => {
    const queue = new StemQueue();
    const left = new Float32Array(4), right = new Float32Array(4);
    queue.mix(left, right, ALL);
    queue.push(block(50, 4, 0, 0.1, 0.1));
    queue.flush();
    queue.mix(left, right, ALL);
    expect(queue.underruns).toBe(0);
    // After a flush the next block says where the count starts again.
    queue.push(block(9000, 4, 0, 0.1, 0.1));
    expect(queue.played).toBe(9000);
  });

  it('ducks the music under the reading and not the reading', () => {
    // Melody 0.05 at a duck of 0.6 is 0.03; the reading's 0.05 is not ducked; together 0.08, and
    // through the makeup 0.08 × 1.19881 = 0.095905: the frame prosody_synth's master test works.
    const queue = new StemQueue();
    const b = block(0, 2, 2, 0.05, 0.05, 0.6);
    b.data.fill(0.05, 4 * 2 * 2, 4 * 2 * 2 + 2);
    queue.push(b);
    const left = new Float32Array(1), right = new Float32Array(1);
    queue.mix(left, right, ALL);
    expect(left[0]).toBeCloseTo(0.095905, 5);
  });

  it('ducks the music, not the reading, under the browser\'s voice, falling as the renderer\'s duck falls', () => {
    // The page asks for 0.6 while the browser's own voice reads. At 48 kHz the fall's time constant
    // (0.08 s) is 3 840 frames: one constant in, the music has gone 1 − 1/e of the way, to
    // 0.6 + 0.4/e = 0.747152 of itself. The reading's stem is untouched, and is what the peak sees.
    const queue = new StemQueue(48_000);
    const b = block(0, 4_000, 0, 0.1, 0.1);
    b.data.fill(0.02, 4 * 2 * 4_000, 4 * 2 * 4_000 + 4_000);
    queue.push(b);
    const left = new Float32Array(4_000), right = new Float32Array(4_000);
    queue.mix(left, right, ALL, 0.6);
    expect(left[3_839]).toBeCloseTo((0.1 * (0.6 + 0.4 / Math.E) + 0.02) * MAKEUP, 5);
    expect(right[0]).toBeLessThan(0.12 * MAKEUP);
    expect(queue.speechPeak).toBeCloseTo(0.02, 6);
  });

  it('glues as prosody_synth::dynamics does', () => {
    // The curve: −6 dB, 8:1, a 6 dB knee from the threshold. 3 dB in: −3 + (1/8 − 1)·9/12 =
    // −3.65625; at 0 dBFS, the knee's end: −2.625, so the makeup is 0.6 × 2.625 = 1.575 dB.
    expect(glueCurve(-20)).toBe(-20);
    expect(glueCurve(-3)).toBeCloseTo(-3.65625, 9);
    expect(glueCurve(0)).toBeCloseTo(-2.625, 9);
    expect(20 * Math.log10(MAKEUP)).toBeCloseTo(1.575, 9);
    // Held at 0 dBFS it settles at the curve's 2.625 dB of reduction and the makeup: −1.05 dB,
    // ×0.8861; the attack's time constant is 0.6 ms, so 5 ms is long enough.
    const glue = new Glue(48_000);
    let g = 0;
    for (let i = 0; i < 240; i++) g = glue.gain(1);
    expect(g).toBeCloseTo(10 ** (-1.05 / 20), 3);
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

describe('a soundfont\'s zones', () => {
  // Three zones as scripts/render-soundfonts.mjs writes them, with made-up values.
  const zone = (position: number, low: number, high: number) => ({
    position, low, high, root: 60, loopStart: null, loopEnd: null, rate: 32_000, file: `zones/${position}.flac`,
  });
  const zones = [zone(0, 0, 40), zone(1, 41, 70), zone(2, 60, 80)];

  it('loads only the zones the keys need, by the synth\'s rule', () => {
    // Each key is looked up one below, as Strudel plays it: 30 as 29, 50 as 49.
    expect(zonesFor(zones, [30])).toEqual([zones[0]]);
    expect(zonesFor(zones, [30, 50])).toEqual([zones[0], zones[1]]);
    // Key 41, the second zone's lowest, plays the zone below it.
    expect(zonesFor(zones, [41])).toEqual([zones[0]]);
    // Key 65 looks up 64, in two zones: the first in the file plays it, as prosody_synth
    // `Instrument::zone` chooses.
    expect(zonesFor(zones, [65])).toEqual([zones[1]]);
    // Key 100 looks up 99, in none: the nearest (60–80) plays it.
    expect(zonesFor(zones, [100])).toEqual([zones[2]]);
  });
});
