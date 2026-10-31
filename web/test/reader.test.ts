// The reading as the render worker times it (src/player/reader.ts): which line is placed where on the
// output's clock, at both paces, worked out by hand. The clock runs at 1 000 frames a second so a
// frame is a millisecond; every piece below has bars a second long.
import { describe, expect, it } from 'vitest';

import type { PerformedCue } from '../src/answers';
import { Reader, type Cast, type Segment, type Told, type Voice } from '../src/player/reader';

const RATE = 1_000;

/** A voice that records what it is asked, every recording 600 frames long. */
function voice() {
  const spoken: [number, number, number, number][] = [];
  const cuts: number[] = [];
  const told: Told[] = [];
  const v: Voice = {
    speak: (id, start, speech, hold) => { spoken.push([id, start, speech, hold]); return start + 600; },
    cut: (at) => { cuts.push(at); },
    tell: (message) => { told.push(message); },
  };
  return { v, spoken, cuts, told };
}

function cue(line: number, stanza: number, extra: Partial<PerformedCue>): PerformedCue {
  return { line, stanza, from: 0, to: 1, pace: 1, pause: 0.25, fitted: null, natural: null, ...extra };
}

function segment(cues: PerformedCue[], cast: Cast, extra: Partial<Segment> = {}): Segment {
  return { serial: 1, outStart: 0, pieceStart: 0, length: 4_000, bars: [0, 1, 2, 3, 4], cues, cast, ...extra };
}

/** Renders `from..to` in blocks of 500 frames, as the worker does. */
function run(reader: Reader, from: number, to: number): void {
  for (let at = from; at < to; at += 500) {
    reader.before(at, at + 500);
    reader.at(at + 500);
  }
}

describe('the fitted pace', () => {
  const cues = [
    cue(0, 0, { fitted: { slot: 0, at: 0, rate: 1 } }),
    cue(1, 0, { fitted: { slot: 2, at: 2, rate: 1.2 } }),
  ];

  it('places a rendered line on its bar, its quiet margin before it, and cuts the line before', () => {
    const { v, spoken, cuts, told } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'fitted';
    reader.held.set('k0', { id: 7, lead: 0, seconds: 0.6 });
    reader.held.set('k1', { id: 8, lead: 0.01, seconds: 1.5 });
    reader.play(segment(cues, new Map([[0, 'k0'], [1, 'k1']])));
    run(reader, 0, 3_000);
    // Line 0 on bar 1 (frame 0); line 1 on bar 3 (frame 2 000), its recording 10 ms early.
    expect(spoken).toEqual([[7, 0, 0, 0], [8, 1_990, 2_000, 0]]);
    expect(cuts).toEqual([0, 2_000]);
    // Line 1's 1.48 s of speech runs 0.48 s past its one-second bar, and says so.
    const placed = told.filter((t) => t.op === 'spoken');
    expect(placed.map((t) => t.op === 'spoken' && t.over)).toEqual([expect.closeTo(-0.4, 9), expect.closeTo(0.48, 9)]);
  });

  it('comes round with the piece', () => {
    const { v, spoken } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'fitted';
    reader.held.set('k0', { id: 7, lead: 0, seconds: 0.6 });
    reader.play(segment(cues.slice(0, 1), new Map([[0, 'k0']])));
    run(reader, 0, 9_000);
    // Every 4 s.
    expect(spoken.map(([, start]) => start)).toEqual([0, 4_000, 8_000]);
  });

  it('hands the browser\'s voice its line when it is due, and a line not yet rendered too, as late', () => {
    const { v, spoken, told } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'fitted';
    reader.play(segment(cues, new Map([[0, null], [1, 'k1']])));
    run(reader, 0, 3_000);
    expect(spoken).toEqual([]);
    expect(told.filter((t) => t.op === 'say')).toEqual([
      { op: 'say', serial: 1, line: 0, pace: 'fitted', at: 0, late: false, chain: 0 },
      { op: 'say', serial: 1, line: 1, pace: 'fitted', at: 2_000, late: true, chain: 0 },
    ]);
  });

  it('reads nothing the cast leaves out', () => {
    const { v, spoken, told } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'fitted';
    reader.held.set('k0', { id: 7, lead: 0, seconds: 0.6 });
    reader.play(segment(cues, new Map()));
    run(reader, 0, 3_000);
    expect(spoken).toEqual([]);
    expect(told.some((t) => t.op === 'say')).toBe(false);
  });
});

describe('the natural pace', () => {
  // Stanza 1: line 0 where its bar (1) begins, then line 1 after line 0's pause of 0.25 s. Stanza 2:
  // line 2 under `breath`, its cue on bar 3 (2 s).
  const cues = [
    cue(0, 0, { pause: 0.25, natural: { onset: 'arrives', slot: 0, at: 0 } }),
    cue(1, 0, { pause: 0.5, natural: { onset: 'follows', slot: null, at: null } }),
    cue(2, 1, { pause: 1.5, natural: { onset: 'breath', slot: 2, at: 2 } }),
  ];
  const cast: Cast = new Map([[0, 'a'], [1, 'b'], [2, 'c']]);

  function reader() {
    const recorded = voice();
    const r = new Reader(RATE, recorded.v);
    r.pace = 'natural';
    r.held.set('a', { id: 1, lead: 0, seconds: 0.6 });
    r.held.set('b', { id: 2, lead: 0, seconds: 0.6 });
    r.held.set('c', { id: 3, lead: 0, seconds: 0.6 });
    return { r, ...recorded };
  }

  it('reads a stanza line after line, and the next a silent bar after it', () => {
    const { r, spoken } = reader();
    r.play(segment(cues, cast));
    run(r, 0, 3_500);
    // Line 0 at 0 s, ending at 0.6 s, held its pause (250 frames) since line 1 is in its stanza.
    // Line 1 after that pause, at 0.85 s, ending at 1.45 s; the last of its stanza, so no hold.
    // Line 2's cue is at 2 s, but under `breath` a whole bar must be silent after bar 2 (where line
    // 1 ended): bar 4, at 3 s.
    expect(spoken).toEqual([[1, 0, 0, 250], [2, 850, 850, 0], [3, 3_000, 3_000, 0]]);
  });

  it('waits for a line still being rendered, and reads it from where the render has got to', () => {
    const { r, spoken, told } = reader();
    r.held.delete('b');
    r.play(segment(cues, cast));
    run(r, 0, 1_500);
    expect(spoken).toEqual([[1, 0, 0, 250]]);
    expect(told.filter((t) => t.op === 'wait')).toEqual([{ op: 'wait', serial: 1, line: 1, key: 'b' }]);
    // It arrives with the frontier at 1.5 s, past the 0.85 s it was due: it starts there, placed
    // as the block from 1.5 s is rendered.
    r.held.set('b', { id: 2, lead: 0, seconds: 0.6 });
    r.arrived('b');
    run(r, 1_500, 2_000);
    expect(spoken[1]).toEqual([2, 1_500, 1_500, 0]);
    const placed = told.filter((t) => t.op === 'spoken');
    expect(placed[1]).toMatchObject({ line: 1, start: 1_500, late: expect.closeTo(0.65, 9) });
  });

  it('chains a line of the browser\'s voice on when the page says it has finished', () => {
    const { r, spoken, told } = reader();
    r.play(segment(cues, new Map([[0, null], [1, 'b'], [2, 'c']])));
    run(r, 0, 500);
    const say = told.find((t) => t.op === 'say');
    expect(say).toMatchObject({ line: 0, pace: 'natural', at: 0 });
    expect(spoken).toEqual([]);
    // The page's playhead stood at 0.9 s when the voice finished: line 1 follows 0.25 s later,
    // placed with the block it begins in.
    r.said(say!.op === 'say' ? say!.chain : -1, 900);
    expect(spoken).toEqual([]);
    run(r, 500, 1_500);
    expect(spoken).toEqual([[2, 1_150, 1_150, 0]]);
    // A word from an older chain changes nothing.
    r.said(-1, 2_000);
    expect(spoken.length).toBe(1);
  });

  it('places a long stanza a line at a time, as the block each begins in comes, not all at once', () => {
    // Line 0 where bar 1 begins, then 99 lines each following the last by 0.25 s: line n begins at
    // n × 0.85 s. Placed all at once they would overflow the synth (MAX_PLACED, 64).
    const long = [cue(0, 0, { natural: { onset: 'arrives', slot: 0, at: 0 } }),
      ...Array.from({ length: 99 }, (_, n) => cue(n + 1, 0, { natural: { onset: 'follows', slot: null, at: null } }))];
    const bars = Array.from({ length: 101 }, (_, n) => n);
    const { v, spoken } = voice();
    const r = new Reader(RATE, v);
    r.pace = 'natural';
    for (let n = 0; n < 100; n++) r.held.set(`k${n}`, { id: n, lead: 0, seconds: 0.6 });
    r.play(segment(long, new Map(long.map((c) => [c.line, `k${c.line}`])), { length: 100_000, bars }));
    run(r, 0, 500);
    expect(spoken).toEqual([[0, 0, 0, 250]]);
    run(r, 500, 3_000);
    expect(spoken.map(([, start]) => start)).toEqual([0, 850, 1_700, 2_550]);
  });

  it('tells no line as spoken that the synth refused, and goes on from the last heard', () => {
    // Lines 0, 1, 2 of one stanza, each following the last; the synth refuses line 1 (held as 2).
    const stanza = [cue(0, 0, { pause: 0.25, natural: { onset: 'arrives', slot: 0, at: 0 } }),
      cue(1, 0, { pause: 0.5, natural: { onset: 'follows', slot: null, at: null } }),
      cue(2, 0, { natural: { onset: 'follows', slot: null, at: null } })];
    const { r, spoken, told } = reader();
    const speak = r['voice'].speak;
    r['voice'].speak = (id, start, speech, hold) => (id === 2 ? null : speak(id, start, speech, hold));
    r.play(segment(stanza, cast));
    run(r, 0, 2_000);
    // Line 1 (due at 0.85 s) is not heard; line 2 follows line 0's end (0.6 s) by line 1's pause
    // (0.5 s), at 1.1 s.
    expect(spoken).toEqual([[1, 0, 0, 250], [3, 1_100, 1_100, 0]]);
    expect(told.filter((t) => t.op === 'spoken').map((t) => t.op === 'spoken' && t.line)).toEqual([0, 2]);
  });

  it('goes on without a line the page could not render, by that line\'s pause', () => {
    const stanza = [cue(0, 0, { pause: 0.25, natural: { onset: 'arrives', slot: 0, at: 0 } }),
      cue(1, 0, { pause: 0.5, natural: { onset: 'follows', slot: null, at: null } }),
      cue(2, 0, { natural: { onset: 'follows', slot: null, at: null } })];
    const { r, spoken } = reader();
    r.held.delete('b');
    r.play(segment(stanza, cast));
    run(r, 0, 1_000);
    expect(spoken).toEqual([[1, 0, 0, 250]]);
    // Line 1 never comes; line 2 follows line 0's end (0.6 s) by line 1's pause (0.5 s): 1.1 s.
    r.failed('b');
    run(r, 1_000, 1_500);
    expect(spoken).toEqual([[1, 0, 0, 250], [3, 1_100, 1_100, 0]]);
    // A failure of a key nobody waits for changes nothing.
    r.failed('c');
    run(r, 1_500, 2_000);
    expect(spoken.length).toBe(2);
  });

  it('asks again for a line waited on whose recording the cast no longer names', () => {
    // Waiting on 'b' for line 1; the voice changes and line 1 is now cast as 'b2', which is held.
    const { r, spoken } = reader();
    r.held.delete('b');
    r.play(segment(cues, cast));
    run(r, 0, 1_500);
    expect(spoken).toEqual([[1, 0, 0, 250]]);
    r.held.set('b2', { id: 5, lead: 0, seconds: 0.6 });
    r.recast(new Map([[1, new Map([[0, 'a'], [1, 'b2'], [2, 'c']])]]), 'natural', false);
    run(r, 1_500, 2_000);
    // Due at 0.85 s, the frontier at 1.5 s: it starts there.
    expect(spoken[1]).toEqual([5, 1_500, 1_500, 0]);
  });

  it('comes round while a line is being read: the old round\'s unread lines are said, the new round read', () => {
    // A 4 s piece: line 0 on bar 1, read by the browser's voice; line 1 follows it. The voice is
    // still reading line 0 when the piece comes round at 4 s.
    const { r, told } = reader();
    r.play(segment(cues.slice(0, 2), new Map([[0, null], [1, 'b']])));
    run(r, 0, 4_500);
    const says = () => told.filter((t) => t.op === 'say');
    expect(says()).toHaveLength(1);
    expect(says()[0]).toMatchObject({ line: 0, at: 0 });
    // It finishes at 4.1 s: line 1 of the first round was never read, and is said; the new round's
    // line 0 (its bar at 4 s) follows the voice's end by its pause, at 4.35 s.
    r.said(says()[0]!.op === 'say' ? (says()[0] as { chain: number }).chain : -1, 4_100);
    expect(told.filter((t) => t.op === 'note')).toEqual([
      { op: 'note', text: 'not read last time round: line 2, the piece came round before its turn', loss: true },
    ]);
    expect(says()).toHaveLength(2);
    expect(says()[1]).toMatchObject({ line: 0, at: 4_350 });
  });

  it('says a stanza it cannot reach before the piece ends', () => {
    // Line 2's cue is on bar 4 (3 s), but line 1 ends at 1.45 s and pauses 2 s after it, to 3.45 s:
    // past bar 4's start, so line 2 would begin on bar 5 — the piece's next round. It is said, not
    // read.
    const late = [cues[0]!, cue(1, 0, { pause: 2, natural: { onset: 'follows', slot: null, at: null } }),
      cue(2, 1, { natural: { onset: 'breath', slot: 3, at: 3 } })];
    const { r, spoken, told } = reader();
    r.play(segment(late, cast));
    run(r, 0, 3_500);
    expect(spoken.map(([id]) => id)).toEqual([1, 2]);
    expect(told.filter((t) => t.op === 'note')).toEqual([
      { op: 'note', text: 'not read this time round: line 3, the piece ends before its turn', loss: false },
    ]);
  });
});

describe('a hand-over and a change of setting', () => {
  it('cut the old reading at the new piece\'s first frame, and read the new one from there', () => {
    const { v, spoken, cuts, told } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'fitted';
    reader.held.set('k', { id: 4, lead: 0, seconds: 0.6 });
    const line = [cue(0, 0, { fitted: { slot: 0, at: 0, rate: 1 } })];
    reader.play(segment(line, new Map([[0, 'k']])));
    run(reader, 0, 1_000);
    // The edit takes over at 1.25 s, from its own bar 2 (1 s in): its bar 1 next comes at 4.25 s.
    reader.swap(segment(line, new Map([[0, 'k']]), { serial: 2, outStart: 1_250, pieceStart: 1_000 }));
    run(reader, 1_000, 5_000);
    expect(cuts).toEqual([0, 1_250, 4_250]);
    expect(told.filter((t) => t.op === 'cut').map((t) => t.op === 'cut' && t.at)).toEqual([0, 1_250, 4_250]);
    expect(spoken).toEqual([[4, 0, 0, 0], [4, 4_250, 4_250, 0]]);
    expect(reader.serials()).toEqual([2]);
  });

  it('keep the breath across a hand-over: the new piece\'s stanza waits its silent bar after the old one\'s last line', () => {
    // Stanza 1 is line 0 alone (0–0.6 s, bar 1). The edit takes over at 1.5 s from its own bar 2;
    // its stanza 2 is cued on bar 3 (2 s), but under `breath` a whole bar must be silent after bar
    // 1, where the old piece's last line ended: bar 3 qualifies, so 2 s — and had the old line run
    // into bar 2, bar 4. Here line 0 ends in bar 1, so the carried end changes nothing; with it
    // ending at 1.2 s (bar 2) the new stanza waits for bar 4, 3 s.
    const stanzas = [
      cue(0, 0, { pause: 0.25, natural: { onset: 'arrives', slot: 0, at: 0 } }),
      cue(1, 1, { natural: { onset: 'breath', slot: 2, at: 2 } }),
    ];
    const run2 = (length: number) => {
      const recorded = voice();
      recorded.v.speak = (id, start, speech, hold) => { recorded.spoken.push([id, start, speech, hold]); return start + length; };
      const reader = new Reader(RATE, recorded.v);
      reader.pace = 'natural';
      reader.held.set('a', { id: 1, lead: 0, seconds: length / RATE });
      reader.held.set('b', { id: 2, lead: 0, seconds: 0.6 });
      const cast: Cast = new Map([[0, 'a'], [1, 'b']]);
      reader.play(segment(stanzas, cast));
      run(reader, 0, 1_500);
      reader.swap(segment(stanzas, cast, { serial: 2, outStart: 1_500, pieceStart: 1_500 }));
      run(reader, 1_500, 3_500);
      return recorded.spoken;
    };
    expect(run2(600)).toEqual([[1, 0, 0, 0], [2, 2_000, 2_000, 0]]);
    // Ending at 1.2 s, in bar 2 — and cut at the hand-over, 1.5 s, which is in bar 2 too.
    expect(run2(1_200)).toEqual([[1, 0, 0, 0], [2, 3_000, 3_000, 0]]);
  });

  it('a change of pace lets go of what is placed from the frontier, and reads at the new pace', () => {
    const { v, cuts } = voice();
    const reader = new Reader(RATE, v);
    reader.pace = 'natural';
    reader.play(segment([], new Map()));
    run(reader, 0, 1_000);
    reader.recast(new Map(), 'fitted', false);
    expect(cuts).toEqual([1_000]);
    expect(reader.pace).toBe('fitted');
  });

  it('wants every recording the pieces playing and to play are cast with', () => {
    const { v } = voice();
    const reader = new Reader(RATE, v);
    reader.play(segment([], new Map([[0, 'a'], [1, null]])));
    reader.swap(segment([], new Map([[0, 'b']]), { serial: 2 }));
    expect([...reader.wanted()].sort()).toEqual(['a', 'b']);
    reader.recast(new Map([[1, new Map([[0, 'c']])]]), 'natural', false);
    expect([...reader.wanted()].sort()).toEqual(['b', 'c']);
  });
});
