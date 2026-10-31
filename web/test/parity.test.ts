// The parity test (docs/player.md §3, §6): the Performance plays what the Strudel text says.
//
// Each fixture in parity/ is one piece twice over, made by the compiler through the page's own
// requests (crates/prosody-web/tests/parity.rs keeps them current): the pattern text, and the
// Performance. Here the text is evaluated by Strudel itself — @strudel/core, @strudel/mini and
// @strudel/transpiler, through their public API only (evalScope, evaluate, queryArc), never their
// source — and every note Strudel would play over the whole piece is compared with the
// Performance's: the same onsets and lengths, sounds, pitches, levels and every parameter, to 1e-6.
//
// What the scheduler plays is a hap with an onset (its part begins where its whole does), at the
// hap's time plus its nudge (the page's honourNudge adds it), on a scheduler at one cycle a second
// (the page's setCps(1), reading::SCHEDULER_CPS), so the pattern's own time after `.cpm` is seconds.
//
// One measured departure is expected, and named rather than tolerated: Strudel's swingBy doubles an
// event whose delayed copy lands in a slice's second half, and drops one whose delayed copy crosses
// into the next slice (prosody_emit::perform::Swing). The Performance follows swingBy's documented
// meaning instead, so the test works out exactly which notes Strudel doubles and drops from the
// Performance's own notes, and requires those and nothing else to differ.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { beforeAll, describe, expect, test } from 'vitest';
import * as core from '@strudel/core';
import * as mini from '@strudel/mini';
import { transpiler } from '@strudel/transpiler';

type Sound =
  | { kind: 'drum'; bank: string; piece: string }
  | { kind: 'font'; name: string; variant: number; file: string }
  | { kind: 'wave'; wave: string };

interface Note {
  lane: string;
  slot: number;
  cycle: number;
  onset: number;
  length: number;
  sound: Sound;
  pitch: number | null;
  gain: number;
  attack: number | null;
  release: number | null;
  lpf: number;
  crush: number | null;
  vowel: string | null;
  shape: number | null;
  room: number;
  roomsize: number;
  delay: { level: number; time: number; feedback: number } | null;
  postgain: number;
  compressor: { threshold: number; ratio: number } | null;
}

interface Performance {
  seconds: number;
  cpm: number;
  swing: { amount: number; slices: number } | null;
  notes: Note[];
}

interface Fixture {
  name: string;
  query: string;
  pattern: string;
  performance: Performance;
}

interface Hap {
  whole: { begin: unknown; end: unknown } | undefined;
  hasOnset(): boolean;
  value: Record<string, unknown>;
}

/** What one played note is, apart from when: compared exactly (to six decimals). */
interface Heard {
  key: string;
  onset: number;
  length: number;
}

const EPSILON = 1e-6;

/** Each lane's colour, as the pattern's `.color` carries it (prosody_emit::perform::Lane). */
const LANES: Record<string, string> = {
  '#c9a6ff': 'pulse',
  '#6fd6c5': 'ornament',
  '#ffb86b': 'bass',
  '#ff7aa8': 'rhyme',
  '#8fb3ff': 'chords',
};

/** Every parameter the pattern may set on a note: one Strudel reads that the Performance does not
 * carry would be a sound the player cannot make, so a hap with any other fails. */
const KNOWN = new Set([
  's', 'n', 'bank', 'note', 'gain', 'velocity', 'nudge', 'cutoff', 'attack', 'release', 'room',
  'roomsize', 'crush', 'vowel', 'shape', 'delay', 'delaytime', 'delayfeedback', 'postgain',
  'compressor', 'compressorRatio', 'color',
]);

const round = (value: number | null | undefined): number | null =>
  value === null || value === undefined ? null : Number(value.toFixed(6));

function keyOf(fields: Record<string, unknown>): string {
  return JSON.stringify(fields, (_, value: unknown) => (typeof value === 'number' ? round(value) : value));
}

function soundName(sound: Sound): string {
  switch (sound.kind) {
    case 'drum': return `drum:${sound.bank}:${sound.piece}`;
    case 'font': return `font:${sound.name}:${sound.variant}`;
    case 'wave': return `wave:${sound.wave}`;
  }
}

function fromNote(note: Note): Heard {
  return {
    onset: note.onset,
    length: note.length,
    key: keyOf({
      lane: note.lane, sound: soundName(note.sound), pitch: note.pitch, gain: note.gain,
      attack: note.attack, release: note.release, lpf: note.lpf, crush: note.crush, vowel: note.vowel,
      shape: note.shape, room: note.room, roomsize: note.roomsize, delay: note.delay,
      postgain: note.postgain, compressor: note.compressor,
    }),
  };
}

function fromHap(hap: Hap): Heard {
  const v = hap.value;
  const unknown = Object.keys(v).filter((key) => !KNOWN.has(key));
  if (unknown.length > 0) throw new Error(`a hap sets ${unknown.join(', ')}, which the Performance does not carry`);
  const num = (key: string): number | null => (typeof v[key] === 'number' ? (v[key] as number) : null);
  const s = String(v.s);
  const sound = v.bank !== undefined ? `drum:${String(v.bank)}:${s}`
    : v.n !== undefined ? `font:${s}:${String(v.n)}`
    : `wave:${s}`;
  const begin = Number(hap.whole?.begin);
  return {
    onset: begin + (num('nudge') ?? 0),
    length: Number(hap.whole?.end) - begin,
    key: keyOf({
      lane: LANES[String(v.color)] ?? String(v.color), sound, pitch: num('note'),
      gain: (num('gain') ?? 1) * (num('velocity') ?? 1), attack: num('attack'), release: num('release'),
      lpf: num('cutoff'), crush: num('crush'), vowel: v.vowel === undefined ? null : String(v.vowel),
      shape: num('shape'), room: num('room'), roomsize: num('roomsize'),
      delay: v.delay === undefined ? null : { level: num('delay'), time: num('delaytime'), feedback: num('delayfeedback') },
      postgain: num('postgain'),
      compressor: v.compressor === undefined ? null : { threshold: num('compressor'), ratio: num('compressorRatio') },
    }),
  };
}

/** The fractional part, with a value a hair under a whole counted as that whole: the fixture's
 * cycles are printed at nine decimals, so 1/3 + 1/6 arrives as 0.4999999997. */
function fraction(value: number): number {
  return value - Math.floor(value + 1e-7);
}

/** Strudel's swingBy, as measured: from each of the Performance's notes, the notes Strudel plays
 * in its place — itself (or its delayed copy), both, or neither. */
function asStrudelPlays(performance: Performance): { heard: Heard[]; doubled: number; dropped: number } {
  const swing = performance.swing;
  if (!swing) return { heard: performance.notes.map(fromNote), doubled: 0, dropped: 0 };
  const slices = swing.slices;
  const delay = swing.amount / (2 * slices);
  const seconds = delay * 60 / performance.cpm;
  let doubled = 0, dropped = 0;
  const heard: Heard[] = [];
  for (const note of performance.notes) {
    const swung = fraction(note.cycle * slices) >= 0.5 - 1e-7;
    const copyLate = fraction((note.cycle + delay) * slices) >= 0.5 - 1e-7;
    const one = fromNote(note);
    if (swung && !copyLate) { dropped += 1; continue; }
    heard.push(one);
    if (!swung && copyLate) {
      doubled += 1;
      heard.push({ ...one, onset: one.onset + seconds });
    }
  }
  return { heard, doubled, dropped };
}

/** Pairs two lists of notes by key and onset; what is left unpaired on either side. */
function unpaired(expected: Heard[], played: Heard[]): { missing: Heard[]; extra: Heard[] } {
  const groups = new Map<string, { expected: Heard[]; played: Heard[] }>();
  const group = (key: string) => {
    let found = groups.get(key);
    if (!found) groups.set(key, (found = { expected: [], played: [] }));
    return found;
  };
  for (const note of expected) group(note.key).expected.push(note);
  for (const note of played) group(note.key).played.push(note);
  const missing: Heard[] = [], extra: Heard[] = [];
  for (const { expected: want, played: got } of groups.values()) {
    want.sort((a, b) => a.onset - b.onset);
    got.sort((a, b) => a.onset - b.onset);
    let i = 0, j = 0;
    while (i < want.length || j < got.length) {
      const a = want[i], b = got[j];
      if (a && b && Math.abs(a.onset - b.onset) < EPSILON && Math.abs(a.length - b.length) < EPSILON) {
        i += 1; j += 1;
      } else if (a && (!b || a.onset < b.onset)) {
        missing.push(a); i += 1;
      } else if (b) {
        extra.push(b); j += 1;
      }
    }
  }
  return { missing, extra };
}

const directory = join(__dirname, 'parity');
const fixtures: Fixture[] = readdirSync(directory)
  .filter((file) => file.endsWith('.json'))
  .sort()
  .map((file) => JSON.parse(readFileSync(join(directory, file), 'utf8')) as Fixture);

describe('the Performance plays what the Strudel text says', () => {
  beforeAll(async () => {
    await core.evalScope(core, mini);
  });

  test('there are fixtures, and one of them swings far enough to reach swingBy\'s departures', () => {
    expect(fixtures.length).toBeGreaterThanOrEqual(7);
    const departures = fixtures.map((fixture) => asStrudelPlays(fixture.performance))
      .reduce((sum, { doubled, dropped }) => sum + doubled + dropped, 0);
    expect(departures).toBeGreaterThan(0);
  });

  for (const fixture of fixtures) {
    test(`${fixture.name}${fixture.query ? ` (${fixture.query})` : ''}`, async () => {
      const { pattern } = await core.evaluate(fixture.pattern, transpiler) as { pattern: { queryArc(begin: number, end: number): Hap[] } };
      const performance = fixture.performance;
      // The whole piece, once: up to a hair short of where it loops round, so the next loop's first
      // notes are not asked for.
      const haps = pattern.queryArc(0, performance.seconds - 1e-7).filter((hap) => hap.hasOnset());
      const played = haps.map(fromHap);
      const { heard, doubled, dropped } = asStrudelPlays(performance);
      const { missing, extra } = unpaired(heard, played);
      expect({ missing: missing.slice(0, 5), extra: extra.slice(0, 5) }).toEqual({ missing: [], extra: [] });
      expect(played.length).toBe(performance.notes.length + doubled - dropped);
      // Without swing the two agree note for note, with no departure to name.
      if (!performance.swing) expect(doubled + dropped).toBe(0);
      expect(performance.notes.length).toBeGreaterThan(0);
    });
  }
});
