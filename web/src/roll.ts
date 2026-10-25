// The lane roll: one lane a layer — pulse, ornament, bass, rhyme, chords — each event a bar in its
// layer's colour, scrolling past a playhead in the middle. Within a pitched lane a higher note sits
// higher; a drum lane's hits sit on its middle line. An event sounding now is lit; one the mixer
// silences is drawn faint, still there, since muting changes what is heard and not what the poem
// is. Drawn from the events Strudel's drawer hands over each frame, so the roll and the lit poem
// read one clock.
import type { Hap } from './strudel/globals';

/** A lane: its layer's name and colour. */
export interface Lane {
  name: string;
  colour: string;
}

/** An event's bar, in canvas pixels. */
export interface Rect {
  lane: number;
  x: number;
  y: number;
  width: number;
  height: number;
  colour: string;
  /** Sounding at the playhead. */
  active: boolean;
  /** Silenced by the mixer. */
  muted: boolean;
}

/** Where the lanes are drawn: the canvas less the gutter their names sit in. */
export interface Frame {
  width: number;
  height: number;
  gutter: number;
}

const time = (at: { valueOf(): number }) => at.valueOf();

/** Every event's bar, laid out: x from its span on the clock between `from` and `to`, y from its
 * lane and, in a pitched lane, its note against the lowest and highest note in view there. */
export function laneRects(haps: Hap[], lanes: Lane[], now: number, [from, to]: [number, number], frame: Frame): Rect[] {
  const laneHeight = frame.height / Math.max(1, lanes.length);
  const span = to - from || 1;
  const width = frame.width - frame.gutter;
  const laneOf = (hap: Hap) => lanes.findIndex((lane) => lane.colour === hap.value?.color);
  // Each lane's range of notes in view, so its notes spread over its height.
  const range = new Map<number, [number, number]>();
  for (const hap of haps) {
    const lane = laneOf(hap);
    const note = Number(hap.value?.note);
    if (lane < 0 || !Number.isFinite(note)) continue;
    const [low, high] = range.get(lane) ?? [note, note];
    range.set(lane, [Math.min(low, note), Math.max(high, note)]);
  }
  const rects: Rect[] = [];
  for (const hap of haps) {
    const lane = laneOf(hap);
    if (lane < 0 || !hap.whole) continue;
    const [begin, end] = [time(hap.whole.begin), time(hap.whole.end)];
    if (end < now + from || begin > now + to) continue;
    const x = frame.gutter + ((begin - now - from) / span) * width;
    const w = Math.max(2, ((end - begin) / span) * width - 1);
    const note = Number(hap.value?.note);
    const [low, high] = range.get(lane) ?? [0, 0];
    // A bar a third of its lane tall; within the lane, up to a third above or below its middle.
    const height = Math.max(3, laneHeight / 3);
    const at = Number.isFinite(note) && high > low ? (note - low) / (high - low) - 0.5 : 0;
    const y = lane * laneHeight + laneHeight / 2 - height / 2 - at * (laneHeight - height) * 0.8;
    rects.push({
      lane, x, y, width: w, height, colour: lanes[lane]!.colour,
      active: begin <= now && now < end, muted: !!hap.value?.muted,
    });
  }
  return rects;
}

/** Paints the lanes, their names, the bar lines, the events and the playhead. */
export function paintLanes(ctx: CanvasRenderingContext2D, rects: Rect[], lanes: Lane[], bars: number[], frame: Frame, scale: number): void {
  const { width, height, gutter } = frame;
  const laneHeight = height / Math.max(1, lanes.length);
  ctx.save();
  ctx.fillStyle = '#18171f';
  ctx.fillRect(0, 0, width, height);
  lanes.forEach((lane, i) => {
    if (i % 2) { ctx.fillStyle = 'rgba(255, 255, 255, 0.025)'; ctx.fillRect(gutter, i * laneHeight, width - gutter, laneHeight); }
    ctx.fillStyle = lane.colour;
    ctx.globalAlpha = 0.85;
    ctx.font = `${Math.round(11 * scale)}px system-ui, sans-serif`;
    ctx.textBaseline = 'middle';
    ctx.fillText(lane.name, 8 * scale, i * laneHeight + laneHeight / 2);
    ctx.globalAlpha = 1;
  });
  ctx.strokeStyle = 'rgba(230, 227, 239, 0.16)';
  ctx.lineWidth = Math.max(1, scale);
  for (const x of bars) {
    ctx.beginPath(); ctx.moveTo(Math.round(x) + 0.5, 0); ctx.lineTo(Math.round(x) + 0.5, height); ctx.stroke();
  }
  for (const rect of rects) {
    ctx.globalAlpha = rect.muted ? 0.18 : rect.active ? 1 : 0.55;
    ctx.fillStyle = rect.colour;
    ctx.shadowColor = rect.active && !rect.muted ? rect.colour : 'transparent';
    ctx.shadowBlur = rect.active && !rect.muted ? 10 * scale : 0;
    ctx.fillRect(rect.x, rect.y, rect.width, rect.height);
  }
  ctx.globalAlpha = 1;
  ctx.shadowBlur = 0;
  const playhead = gutter + (width - gutter) / 2;
  ctx.fillStyle = '#e6e3ef';
  ctx.fillRect(Math.round(playhead) - scale, 0, 2 * scale, height);
  ctx.restore();
}
