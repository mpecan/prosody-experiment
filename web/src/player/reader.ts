// The reading, timed in the render worker (`docs/player.md` §5, "The reading"): where each line of
// the poem is read on the music's clock, as the worker renders ahead.
//
// The cues come from the Performance (`PerformedCue`): for each line, the bar it is read in at the
// fitted pace, and the bar it may begin in at the natural pace, both in seconds from the piece's
// start. Before rendering a block the worker asks this module which cues fall in it, and the module
// places each line on the synth's speech stem (`Voice.speak`) — a rendered line at its frame, exactly
// — or, for a line the browser's own voice reads, tells the page when it is due, since that voice
// cannot be placed in a stream; the page reads it off the playhead, unsynchronised (`docs/player.md`
// §5). Who reads each line, and with which rendering, the page decides (`Cast`): it knows which
// engines have loaded and which lines they have rendered.
//
// The natural pace's chaining is the page's from before the player (reading/index.ts `naturalCue`
// and friends, SPEC.md §4.7), moved here unchanged in what it decides: a line follows the last by
// its pause; a stanza begins where its cue says the music has reached it, and under `breath` not
// before a whole silent bar has passed since the last stanza's last line ended. It needs each line's
// spoken length, which only the rendered line has — which is why it runs where the line is placed.
// Times are seconds on the output's clock (output frames ÷ rate); "now" is the render frontier, the
// first frame not yet rendered, so a late line starts there, never in what is already queued.
//
// A rendered line is placed when the block its recording begins in is about to be rendered, not as
// soon as the chain knows when it falls: a stanza's held lines would otherwise all be placed in one
// go, and a long stanza would overflow the synth's placements (prosody_synth `MAX_PLACED`). Placed
// that late it starts on the same frame, since nothing before that block is rendered meanwhile.
import type { PerformedCue } from '../answers';

/** A recorded line the synth holds: its number there, the quiet margin before its speech, and its
 * length, both in seconds. */
export interface Held {
  id: number;
  lead: number;
  seconds: number;
}

/** Who reads each line of a piece, by line number: the render key of the recording to place, or
 * null for the browser's own voice. A line it leaves out is not read. */
export type Cast = Map<number, string | null>;

/** What the reader tells the page. Frames are output frames; `serial` names the piece. */
export type Told =
  | { op: 'spoken'; serial: number; line: number; pace: Pace; start: number; end: number; over: number; late: number }
  | { op: 'say'; serial: number; line: number; pace: Pace; at: number; late: boolean; chain: number }
  | { op: 'cut'; at: number }
  | { op: 'wait'; serial: number; line: number; key: string }
  | { op: 'note'; text: string; loss: boolean }
  /** Where the reading is: the line it will want next, by number, or null once the round's are read. */
  | { op: 'at'; serial: number; pace: Pace; line: number | null };

export type Pace = 'fitted' | 'natural';

/** What the reader drives: the synth's speech stem, and the page. */
export interface Voice {
  /** Places held line `id`, its recording from frame `start`, its speech from `speech`, the music
   * kept down `hold` frames after it; gives the frame after its last, or null if the synth refused
   * it (and said why) — a line that is not heard is not told as spoken. */
  speak(id: number, start: number, speech: number, hold: number): number | null;
  /** Cuts every line off at frame `at`. */
  cut(at: number): void;
  tell(told: Told): void;
}

/** A piece on the output's clock: from output frame `outStart` it plays from its own `pieceStart`. */
export interface Segment {
  serial: number;
  outStart: number;
  pieceStart: number;
  /** Its length in frames: where it loops round. */
  length: number;
  /** Where its bars begin, in seconds, and its length last. */
  bars: number[];
  cues: PerformedCue[];
  cast: Cast;
}

/** A cue reached in a block: on output frame `out`, in round `round` of its piece. */
interface Reached {
  out: number;
  round: number;
  cue: PerformedCue;
}

/** The natural pace's chain: which line is next, and what it waits for. */
export interface Chain {
  serial: number;
  segment: Segment;
  seconds: number[];
  /** The natural lines, in reading order. */
  lines: PerformedCue[];
  next: number;
  round: number | null;
  wrapTo: number | null;
  /** The output time this round's bar 1 began. */
  origin: number;
  /** Index into `lines` → the output time its cue fired, this round. */
  anchors: Map<number, number>;
  prevEnd: number | null;
  prevPause: number;
  /** Waiting on the page: for the browser's voice to finish a line, or for a line to be rendered. */
  busy: false | 'said' | string;
}

/** Bar `k` of the looping piece (negative or past the end: the rounds before and after) on the
 * output's clock, and the bar playing at output time `t`. */
export function barTime(c: Pick<Chain, 'seconds' | 'origin'>, k: number): number {
  const bars = c.seconds, slots = bars.length - 1, length = bars[slots] ?? 0;
  const round = Math.floor(k / slots);
  return c.origin + round * length + (bars[k - round * slots] ?? 0);
}
export function barAt(c: Pick<Chain, 'seconds' | 'origin'>, t: number): number {
  const bars = c.seconds, slots = bars.length - 1, length = bars[slots] ?? 0;
  const round = Math.floor((t - c.origin) / length);
  const within = t - c.origin - round * length;
  let k = 0;
  while (k + 1 < slots && (bars[k + 1] ?? Infinity) <= within + 1e-9) k += 1;
  return k + round * slots;
}

/** A time in seconds as the nearest frame, as the synth turns a note's time into one
 * (prosody_synth `Seconds::frames`). */
export const frameOf = (seconds: number, rate: number) => Math.round(seconds * rate);

export class Reader {
  /** The pieces playing and to play: the first is heard, a second takes over at its `outStart`. */
  private segments: Segment[] = [];
  private chain: Chain | null = null;
  private chains = 0;
  /** Where the old piece's last line ended, and its pause, carried into the new piece's chain at a
   * hand-over: a stanza of the new piece still waits its silent bar after the old one's last line. */
  private carry: { prevEnd: number; prevPause: number } | null = null;
  pace: Pace = 'natural';
  /** The recorded lines the synth holds, by render key. */
  readonly held = new Map<string, Held>();
  /** The first frame not yet rendered: what "now" is. */
  private frontier = 0;
  /** While a block is being prepared (before()), the frame after it: a line whose recording begins
   * before this is placed now, a later one waits for its own block. */
  private until: number | null = null;
  /** The chain stopped at a held line whose block has not come yet: before() resumes it. */
  private deferred = false;

  constructor(private readonly rate: number, private readonly voice: Voice) {}

  /** A piece from its top, from the frontier: nothing before it is read on. */
  play(segment: Segment): void {
    this.segments = [segment];
    this.dropChain();
    this.carry = null;
  }

  /** A piece taking over at its `outStart`: a second hand-over before the first has happened
   * replaces it, as the synth's does. */
  swap(segment: Segment): void {
    this.segments = [this.segments[0], segment].filter((s): s is Segment => !!s);
  }

  /** Nothing playing. */
  stop(): void {
    this.segments = [];
    this.dropChain();
    this.carry = null;
  }

  private dropChain(): void {
    this.chain = null;
    this.deferred = false;
  }

  /** The pieces playing and to play, by serial: whose casts the page keeps up to date. */
  serials(): number[] {
    return this.segments.map((segment) => segment.serial);
  }

  /** Every render key some piece playing or to play may place. */
  wanted(): Set<string> {
    const keys = new Set<string>();
    for (const segment of this.segments) for (const key of segment.cast.values()) if (key) keys.add(key);
    return keys;
  }

  /** New casts from the page. A change of pace, or `quiet` (a change of voice, engine or language),
   * lets go of everything placed from the frontier on, and the reading picks up at the next cue. */
  recast(casts: Map<number, Cast>, pace: Pace, quiet: boolean): void {
    for (const segment of this.segments) segment.cast = casts.get(segment.serial) ?? segment.cast;
    if (quiet || pace !== this.pace) {
      this.pace = pace;
      this.cutAt(this.frontier);
      this.dropChain();
      this.carry = null;
      return;
    }
    // A line waited for that its piece no longer casts with that key (dropped: the engine or voice
    // changed) is asked for again from whoever reads it now.
    const chain = this.chain;
    if (chain && typeof chain.busy === 'string' && chain.busy !== 'said' && !this.wanted().has(chain.busy)) {
      chain.busy = false;
      this.advance();
    }
  }

  /** A recorded line the synth now holds: a line waiting for it is read. */
  arrived(key: string): void {
    if (this.chain?.busy === key) {
      this.chain.busy = false;
      this.advance();
    }
  }

  /** A line the page could not render: the chain goes on without it. */
  failed(key: string): void {
    const chain = this.chain;
    if (chain?.busy !== key) return;
    chain.prevPause = chain.lines[chain.next]?.pause ?? 0;
    chain.next += 1;
    chain.busy = false;
    this.advance();
  }

  /** The browser's voice finished line `chain`'s current line, as the playhead stood at `end`. */
  said(chain: number, end: number): void {
    const c = this.chain;
    if (!c || c.serial !== chain || c.busy !== 'said') return;
    c.prevEnd = end / this.rate;
    c.prevPause = c.lines[c.next]?.pause ?? 0;
    c.next += 1;
    c.busy = false;
    this.advance();
  }

  /** Moves "now" to `frontier`, as the worker renders. */
  at(frontier: number): void {
    this.frontier = frontier;
  }

  /** Before output frames `from..to` are rendered: every cue in them, in order, on the piece that
   * plays there — the old one up to a hand-over, and the new one from it, where the old reading is
   * cut off (its chain was the old piece's). */
  before(from: number, to: number): void {
    this.frontier = from;
    const [now, next] = this.segments;
    if (!now) return;
    if (next && next.outStart < to) {
      const at = Math.max(from, next.outStart);
      this.block(now, from, at);
      this.cutAt(at);
      const old = this.chain;
      this.carry = old && old.prevEnd !== null
        ? { prevEnd: Math.min(old.prevEnd, at / this.rate), prevPause: old.prevPause }
        : null;
      this.dropChain();
      this.segments = [next];
      this.block(next, at, to);
    } else {
      this.block(now, from, to);
    }
    this.until = null;
  }

  /** Output frames `from..to` of `segment`: the lines the chain held back for them placed first
   * (each falls before any cue reached here, so a round's lines are read before the seam), then
   * the cues in them. */
  private block(segment: Segment, from: number, to: number): void {
    this.until = to;
    if (this.deferred) {
      this.deferred = false;
      this.advance();
    }
    this.reach(segment, from, to);
  }

  private cutAt(at: number): void {
    this.voice.cut(at);
    this.voice.tell({ op: 'cut', at });
  }

  /** Every cue of the pace read, of `segment`, that falls on output frames `from..to`. */
  private reach(segment: Segment, from: number, to: number): void {
    if (to <= from || segment.length <= 0) return;
    const reached: Reached[] = [];
    const p0 = segment.pieceStart + (from - segment.outStart);
    const p1 = p0 + (to - from);
    for (const cue of segment.cues) {
      const at = this.pace === 'fitted' ? cue.fitted?.at : cue.natural?.at;
      if (at === null || at === undefined) continue;
      const frame = frameOf(at, this.rate);
      for (let round = Math.floor((p0 - frame) / segment.length); round * segment.length + frame < p1; round++) {
        const position = round * segment.length + frame;
        if (position >= p0) reached.push({ out: segment.outStart + position - segment.pieceStart, round, cue });
      }
    }
    reached.sort((a, b) => a.out - b.out || a.cue.line - b.cue.line);
    for (const { out, round, cue } of reached) {
      if (this.pace === 'fitted') this.fitted(segment, cue, out);
      else this.natural(segment, cue, out, round);
    }
  }

  // ---- The fitted pace: a line a bar ----

  private fitted(segment: Segment, cue: PerformedCue, out: number): void {
    // A line still sounding when the next is due is cut off at the bar.
    this.cutAt(out);
    const order = segment.cues.filter((c) => c.fitted);
    const after = order[order.indexOf(cue) + 1];
    this.voice.tell({ op: 'at', serial: segment.serial, pace: 'fitted', line: after ? after.line : null });
    // A line the cast leaves out is not read (a reading let go: a preset in another language).
    if (!segment.cast.has(cue.line)) return;
    const key = segment.cast.get(cue.line) ?? null;
    const held = key === null ? undefined : this.held.get(key);
    if (!held) {
      this.voice.tell({ op: 'say', serial: segment.serial, line: cue.line, pace: 'fitted', at: out, late: key !== null, chain: 0 });
      return;
    }
    const start = Math.max(0, out - frameOf(held.lead, this.rate));
    const end = this.voice.speak(held.id, start, out, 0);
    if (end === null) return;
    const slot = cue.fitted?.slot ?? 0;
    const bar = (segment.bars[slot + 1] ?? Infinity) - (segment.bars[slot] ?? 0);
    const over = held.seconds - 2 * held.lead - bar;
    this.voice.tell({ op: 'spoken', serial: segment.serial, line: cue.line, pace: 'fitted', start: out, end, over, late: 0 });
  }

  // ---- The natural pace: a line after the last ----

  private natural(segment: Segment, cue: PerformedCue, out: number, round: number): void {
    if (!this.chain || this.chain.segment !== segment) this.chain = this.newChain(segment);
    const c = this.chain;
    const index = c.lines.indexOf(cue);
    if (index < 0 || !cue.natural) return;
    const time = out / this.rate;
    const slotAt = cue.natural.at ?? 0;
    c.origin = time - slotAt;
    if (round !== c.round) {
      // The piece has come round: this round's cues begin afresh, from this one. While a line is
      // still being read (the browser's voice until it ends, a line until it is rendered) the move
      // waits for it to finish (`wrapTo`, taken in advance()), or its step on would pass the last
      // line and leave this round unread. The last line's end is kept, so a first stanza still
      // waits for its breath after a last one that ran over the loop's seam.
      if (c.round !== null && !c.busy) this.wrapRound(c, index);
      else if (c.round !== null) c.wrapTo = index;
      else c.next = index;
      c.round = round;
      c.anchors.clear();
    }
    c.anchors.set(index, time);
    // Nothing being read, and the line waited for is not this one: either the clock was joined
    // part-way through (a hand-over) or the reading had finished this round. Start here.
    if (!c.busy && c.next !== index && !c.anchors.has(c.next)) c.next = index;
    this.advance();
  }

  private newChain(segment: Segment): Chain {
    const carried = this.carry;
    this.carry = null;
    return {
      serial: ++this.chains,
      segment,
      seconds: segment.bars,
      lines: segment.cues.filter((cue) => cue.natural),
      next: 0, round: null, wrapTo: null, origin: 0,
      anchors: new Map(),
      prevEnd: carried?.prevEnd ?? null, prevPause: carried?.prevPause ?? 0,
      busy: false,
    };
  }

  // The chain moves to the new round's line `index`. A line of the old round not yet read will not
  // be — its round's cues are gone — and is said, not dropped in silence (CLAUDE.md §6).
  private wrapRound(c: Chain, index: number): void {
    if (c.next < c.lines.length) {
      const lost = c.lines.slice(c.next).map((l) => l.line + 1);
      this.voice.tell({ op: 'note', text: `not read last time round: line ${lost.join(', ')}, the piece came round before its turn`, loss: true });
    }
    c.next = index;
    c.wrapTo = null;
  }

  /** Reads on as far as it can, then says where the reading is. */
  private advance(): void {
    const c = this.chain;
    if (!c) return;
    this.advanceChain(c);
    this.voice.tell({ op: 'at', serial: c.segment.serial, pace: 'natural', line: c.lines[c.next]?.line ?? null });
  }

  private advanceChain(c: Chain): void {
    if (c.busy) return;
    if (c.wrapTo !== null) this.wrapRound(c, c.wrapTo);
    const lines = c.lines;
    while (c.next < lines.length) {
      const line = lines[c.next]!;
      const onset = line.natural?.onset ?? 'follows';
      let earliest = -Infinity;
      if (onset !== 'follows') {
        const anchor = c.anchors.get(c.next);
        if (anchor === undefined) {
          // Waiting for its bar. If a later line's cue has already fired this round, this one's
          // will not (the clock was joined after it): move on to that one.
          const after = [...c.anchors.keys()].filter((k) => k > c.next).sort((a, b) => a - b)[0];
          if (after === undefined) return;
          c.next = after;
          continue;
        }
        earliest = anchor;
      } else if (c.prevEnd === null) {
        // Joined in the middle of a stanza: there is no line before to follow, so this one waits for
        // the piece to come round, as a line before the clock's bar does at the fitted pace.
        c.next += 1;
        continue;
      }
      if (c.prevEnd !== null) {
        earliest = Math.max(earliest, c.prevEnd + c.prevPause);
        if (onset === 'breath') {
          // On a bar, and at least two bars after the one the last line ended in, so one whole bar
          // between is silent: the first bar from there that begins no earlier than `earliest`.
          let bar = Math.max(barAt(c, c.prevEnd) + 2, barAt(c, earliest));
          if (barTime(c, bar) < earliest - 1e-6) bar += 1;
          earliest = barTime(c, bar);
        }
      }
      const pieceEnd = c.origin + (c.seconds[c.seconds.length - 1] ?? 0);
      if (earliest >= pieceEnd - 1e-6) {
        const lost = lines.slice(c.next).filter((l) => l.stanza >= line.stanza).map((l) => l.line + 1);
        this.voice.tell({ op: 'note', text: `not read this time round: line ${lost.join(', ')}, the piece ends before its turn`, loss: false });
        c.next = lines.length;
        return;
      }
      if (!this.speakNatural(c, line, earliest) || c.busy) return;
    }
  }

  /** Reads `line`, or waits for it; false when its block has not come yet and it is left for
   * before() to place. */
  private speakNatural(c: Chain, line: PerformedCue, earliest: number): boolean {
    const now = this.frontier / this.rate;
    const nextLine = c.lines[c.next + 1];
    const last = !nextLine || nextLine.stanza !== line.stanza;
    // A line the cast leaves out is not read, and the next does not wait for it.
    if (!c.segment.cast.has(line.line)) {
      c.next += 1;
      return true;
    }
    const key = c.segment.cast.get(line.line) ?? null;
    const serial = c.segment.serial;
    if (key === null) {
      // Not placed in the stream, so not held to the frontier: the page speaks it when its playhead
      // reaches the time it is due, or at once if that has passed.
      c.busy = 'said';
      this.voice.tell({
        op: 'say', serial, line: line.line, pace: 'natural', at: frameOf(Math.max(earliest, 0), this.rate), late: false, chain: c.serial,
      });
      return true;
    }
    const held = this.held.get(key);
    if (!held) {
      c.busy = key;
      this.voice.tell({ op: 'wait', serial, line: line.line, key });
      return true;
    }
    const start = Math.max(earliest, now);
    const speech = frameOf(start, this.rate);
    const begins = Math.max(0, speech - frameOf(held.lead, this.rate));
    if (begins >= (this.until ?? this.frontier)) {
      this.deferred = true;
      return false;
    }
    const end = this.voice.speak(held.id, begins, speech, last ? 0 : frameOf(line.pause, this.rate));
    c.prevPause = line.pause;
    c.next += 1;
    // Refused (the worker has said why): not heard, so not told as spoken, and the next line follows
    // the last one heard by this one's pause, as after a line that could not be rendered.
    if (end === null) return true;
    this.voice.tell({ op: 'spoken', serial, line: line.line, pace: 'natural', start: speech, end, over: 0, late: start - earliest });
    c.prevEnd = end / this.rate;
    return true;
  }
}
