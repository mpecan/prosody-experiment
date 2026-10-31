// What the AudioWorklet does with the blocks the render worker sends (`docs/player.md` §5): queue
// them, mix each stem at its gain — mute and solo are a gain of 0, glided to over a few ms — through
// the master's soft clip, and count the times the queue ran dry. Kept apart from the worklet itself
// so it can be tested without an audio thread; `worklet.ts` is the thin shell that runs it.

/** The stems a block holds, each stereo (prosody_synth::STEMS). */
export const STEMS = 4;

/** Frames a stem's gain takes to go all the way from 1 to 0 or back: about 5 ms at 48 kHz. A gain
 * switched from one sample to the next is a step in a sounding wave — a click on every mute and
 * solo. (Strudel's mute applies as each event is scheduled, so it has no step to hide; ours applies
 * mid-note, and the glide is what keeps that quiet.) */
export const GLIDE = 256;
/** Frames the music still queued fades out over when it is dropped, at a stop or a restart from the
 * top: about 5 ms, for the same reason. */
export const FADE = 256;

// The master's ceiling, as the page's Strudel master has it (strudel/index.ts `softClipCurve`) and
// as `prosody_synth::master` does for a file: transparent below the knee, bending smoothly towards
// the ceiling above it, flat beyond the range. The glue compressor ahead of it comes in step 3.
const CLIP_KNEE = 0.6, CLIP_CEILING = 0.95, CLIP_RANGE = 4;
export function softClip(x: number): number {
  const a = Math.min(Math.abs(x), CLIP_RANGE);
  const bend = CLIP_CEILING - CLIP_KNEE;
  const shaped = a <= CLIP_KNEE ? a : CLIP_KNEE + bend * Math.tanh((a - CLIP_KNEE) / bend);
  return x < 0 ? -shaped : shaped;
}

/** A rendered block: `frames` frames from render frame `frame`, the stems planar (each stem's left
 * side, then its right). */
export interface Block {
  frame: number;
  frames: number;
  data: Float32Array;
}

/** The blocks waiting to be heard, and how far through them the output has got. */
export class StemQueue {
  private blocks: Block[] = [];
  /** How far into the first block. */
  private offset = 0;
  /** The render frame the next output frame plays: the playhead, in the renderer's count. */
  played = 0;
  /** Whether anything has arrived since the last flush: until it has, silence is not a shortfall. */
  started = false;
  /** Output quanta that wanted music and found the queue empty. */
  underruns = 0;
  /** Each stem's gain now, gliding towards the one asked for. */
  private gains = new Array<number>(STEMS).fill(1);
  /** Set when a new start of play arrives: its first frame takes the gains asked for as they are,
   * since there is nothing sounding to glide from — a stem muted before Play is silent from the
   * start. */
  private fresh = true;
  /** The fade-out of what was queued at the last flush, mixed (before the clip), and how far
   * through it the output is. Allocated once: a flush happens on the audio thread. */
  private tailLeft = new Float32Array(FADE);
  private tailRight = new Float32Array(FADE);
  private tailFrames = 0;
  private tailAt = 0;

  /** A block, in the order rendered. The first after a flush says where the count starts. */
  push(block: Block): void {
    if (!this.started && this.blocks.length === 0) {
      this.played = block.frame;
      this.offset = 0;
      this.fresh = true;
    }
    this.started = true;
    this.blocks.push(block);
  }

  /** Drops everything queued, after a stop or for a restart from the top — but not abruptly: the
   * next `FADE` frames of it, at the gains they would have had, are kept as a tail that fades to
   * nothing under whatever comes next. A tail still fading from a flush before goes on fading. */
  flush(): void {
    let frames = 0;
    for (let k = 0; k < FADE; k++) {
      // The tail before, still to be heard: read at or ahead of where this one is written, so the
      // two can share the buffer.
      const before = this.tailAt + k < this.tailFrames;
      let l = before ? this.tailLeft[this.tailAt + k]! : 0;
      let r = before ? this.tailRight[this.tailAt + k]! : 0;
      const at = this.frameAt(k);
      if (at) {
        const fade = (FADE - k) / FADE;
        for (let stem = 0; stem < STEMS; stem++) {
          const gain = this.gains[stem]! * fade;
          if (gain === 0) continue;
          const base = stem * 2 * at.block.frames + at.offset;
          l += gain * at.block.data[base]!;
          r += gain * at.block.data[base + at.block.frames]!;
        }
      }
      if (at || before) frames = k + 1;
      this.tailLeft[k] = l;
      this.tailRight[k] = r;
    }
    this.tailFrames = frames;
    this.tailAt = 0;
    this.blocks = [];
    this.offset = 0;
    this.started = false;
  }

  /** The block and offset holding the `k`th frame from the playhead, if one is queued. */
  private frameAt(k: number): { block: Block; offset: number } | null {
    let offset = this.offset + k;
    for (const block of this.blocks) {
      if (offset < block.frames) return { block, offset };
      offset -= block.frames;
    }
    return null;
  }

  /** Frames queued ahead of the playhead. */
  get queued(): number {
    return this.blocks.reduce((sum, block) => sum + block.frames, 0) - this.offset;
  }

  /** Fills `left` and `right` with the next frames, each stem at its gain (gliding to the `gains`
   * asked for, a 1/`GLIDE` step a frame), plus any tail fading from a flush, through the soft clip.
   * Frames the queue cannot fill are silence, and a quantum short of music counts one underrun. */
  mix(left: Float32Array, right: Float32Array, gains: readonly number[]): void {
    let short = false;
    for (let i = 0; i < left.length; i++) {
      let l = 0, r = 0;
      const block = this.blocks[0];
      if (block) {
        for (let stem = 0; stem < STEMS; stem++) {
          const gain = this.glide(stem, gains[stem] ?? 1);
          if (gain === 0) continue;
          const base = stem * 2 * block.frames + this.offset;
          l += gain * block.data[base]!;
          r += gain * block.data[base + block.frames]!;
        }
        this.fresh = false;
        this.offset += 1;
        this.played += 1;
        if (this.offset >= block.frames) {
          this.blocks.shift();
          this.offset = 0;
        }
      } else {
        short = true;
      }
      if (this.tailAt < this.tailFrames) {
        l += this.tailLeft[this.tailAt]!;
        r += this.tailRight[this.tailAt]!;
        this.tailAt += 1;
      }
      left[i] = softClip(l);
      right[i] = softClip(r);
    }
    if (short && this.started) this.underruns += 1;
  }

  /** Stem `stem`'s gain for the next frame: a step of at most 1/`GLIDE` towards `target`, or the
   * target itself on the first frame of a start of play. */
  private glide(stem: number, target: number): number {
    const now = this.gains[stem]!;
    const next = this.fresh ? target
      : target > now ? Math.min(target, now + 1 / GLIDE) : Math.max(target, now - 1 / GLIDE);
    this.gains[stem] = next;
    return next;
  }
}
