// The page's clock under the Prosody player (`docs/player.md` §5, "The page's clock"): the playhead
// is the frames the worklet has played; the render worker says where each piece plays from on that
// count (a placement); so a frame is a time in a piece, and the piece's own notes and marks say what
// sounds then. Nothing here asks a scheduler: the roll, the meters and the lit poem read the
// Performance by time.
import type { Performance, PerformedMark, PerformedNote } from '../answers';

/** From output frame `outStart` on, `performance` plays from its own frame `pieceStart`. */
export interface Placement {
  outStart: number;
  pieceStart: number;
  performance: Performance;
}

/** The placement playing output frame `frame`, and where in its piece that is, in seconds counted
 * from the piece's first time round — it keeps rising as the piece loops, as Strudel's cycles do —
 * or null before anything plays. Placements are kept in the order they were made; a later one takes
 * over from its own `outStart`. */
export function pieceTime<P extends Placement>(frame: number, placements: P[], rate: number): { placement: P; time: number } | null {
  let found: P | null = null;
  for (const placement of placements) if (placement.outStart <= frame) found = placement;
  if (!found) return null;
  return { placement: found, time: (found.pieceStart + frame - found.outStart) / rate };
}

/** The placements still worth keeping once output frame `frame` plays: the one playing it and any
 * that take over later. Every live edit adds a placement, and each holds its Performance; without
 * this they would pile up until the next restart from the top, and `pieceTime` would scan them all
 * every frame. An edit superseded before it took over (two edits placed at the same bar) goes too
 * once that bar plays, since the later one at the same `outStart` is the one heard. Nothing is
 * dropped before anything plays. */
export function settle<P extends Placement>(placements: P[], frame: number): P[] {
  let current = -1;
  placements.forEach((placement, index) => { if (placement.outStart <= frame) current = index; });
  return current > 0 ? placements.slice(current) : placements;
}

/** A time folded into one time round the piece. */
export function within(time: number, seconds: number): number {
  return seconds > 0 ? ((time % seconds) + seconds) % seconds : time;
}

/** A Performance's notes and marks sorted by onset, with the longest of each, so what sounds at a
 * time is found by binary search rather than a scan of the whole piece. */
export interface Indexed {
  performance: Performance;
  notes: PerformedNote[];
  marks: { mark: PerformedMark; index: number }[];
  longestNote: number;
  longestMark: number;
}

export function indexed(performance: Performance): Indexed {
  const notes = [...performance.notes].sort((a, b) => a.onset - b.onset);
  const marks = performance.marks.map((mark, index) => ({ mark, index })).sort((a, b) => a.mark.onset - b.mark.onset);
  return {
    performance,
    notes,
    marks,
    longestNote: notes.reduce((longest, note) => Math.max(longest, note.length), 0),
    longestMark: marks.reduce((longest, { mark }) => Math.max(longest, mark.length), 0),
  };
}

/** The first index in `items` (sorted by `onset`) whose onset is at least `at`. */
function firstFrom<T>(items: T[], onset: (item: T) => number, at: number): number {
  let low = 0, high = items.length;
  while (low < high) {
    const mid = (low + high) >> 1;
    if (onset(items[mid]!) < at) low = mid + 1;
    else high = mid;
  }
  return low;
}

/** The marks lit at `time` (any time round): each with its index in the Performance's marks. Once the
 * piece has come round, a mark of the round before that runs over the loop's end is lit too: swing
 * (`docs/player.md` §9) can delay the last step of a slice to begin just before the piece's end and
 * end past it, and its mark would otherwise go dark the moment the piece comes round, as
 * `notesBetween` keeps it. (A step swung to begin past the end is the next round's: its onset is
 * already counted from that round's start, as Strudel plays it.) */
export function marksAt(piece: Indexed, time: number): { mark: PerformedMark; index: number }[] {
  const seconds = piece.performance.seconds;
  const at = within(time, seconds);
  const lit: { mark: PerformedMark; index: number }[] = [];
  // The marks sounding `at` into a round: those begun by then and not yet over.
  const sounding = (at: number) => {
    for (let i = firstFrom(piece.marks, (m) => m.mark.onset, at - piece.longestMark); i < piece.marks.length; i++) {
      const entry = piece.marks[i]!;
      if (entry.mark.onset > at) break;
      if (at < entry.mark.onset + entry.mark.length) lit.push(entry);
    }
  };
  if (seconds > 0 && time >= seconds) sounding(at + seconds);
  sounding(at);
  return lit;
}

/** A note placed on the clock that keeps rising: its onset plus `round` times round the piece. */
export interface Placed {
  note: PerformedNote;
  begin: number;
  end: number;
}

/** Every note sounding at some point in `from..to` (times that keep rising), each round of the
 * piece it falls in included — what the roll draws and the meters read. */
export function notesBetween(piece: Indexed, from: number, to: number): Placed[] {
  const length = piece.performance.seconds;
  const found: Placed[] = [];
  if (!(length > 0)) return found;
  const earliest = from - piece.longestNote;
  for (let round = Math.floor(earliest / length); round * length <= to; round++) {
    const base = round * length;
    for (let i = firstFrom(piece.notes, (n) => n.onset, earliest - base); i < piece.notes.length; i++) {
      const note = piece.notes[i]!;
      const begin = base + note.onset;
      if (begin > to) break;
      const end = begin + note.length;
      if (end >= from) found.push({ note, begin, end });
    }
  }
  return found;
}
