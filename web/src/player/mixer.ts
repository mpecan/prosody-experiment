// What the AudioWorklet does with the blocks the render worker sends (`docs/player.md` §5): queue
// them, mix each stem at its gain — mute and solo are a gain of 0, heard at once — through the
// master's soft clip, and count the times the queue ran dry. Kept apart from the worklet itself so
// it can be tested without an audio thread; `worklet.ts` is the thin shell that runs it.

/** The stems a block holds, each stereo (prosody_synth::STEMS). */
export const STEMS = 4;

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

  /** A block, in the order rendered. The first after a flush says where the count starts. */
  push(block: Block): void {
    if (!this.started && this.blocks.length === 0) {
      this.played = block.frame;
      this.offset = 0;
    }
    this.started = true;
    this.blocks.push(block);
  }

  /** Drops everything queued: after a stop. */
  flush(): void {
    this.blocks = [];
    this.offset = 0;
    this.started = false;
  }

  /** Frames queued ahead of the playhead. */
  get queued(): number {
    return this.blocks.reduce((sum, block) => sum + block.frames, 0) - this.offset;
  }

  /** Fills `left` and `right` with the next frames, each stem at its gain, through the soft clip.
   * Frames the queue cannot fill are silence, and a quantum short of music counts one underrun. */
  mix(left: Float32Array, right: Float32Array, gains: readonly number[]): void {
    let at = 0;
    const wanted = left.length;
    while (at < wanted && this.blocks.length > 0) {
      const block = this.blocks[0]!;
      const take = Math.min(wanted - at, block.frames - this.offset);
      for (let i = 0; i < take; i++) {
        let l = 0, r = 0;
        for (let stem = 0; stem < STEMS; stem++) {
          const gain = gains[stem] ?? 1;
          if (gain === 0) continue;
          const base = stem * 2 * block.frames + this.offset + i;
          l += gain * block.data[base]!;
          r += gain * block.data[base + block.frames]!;
        }
        left[at + i] = softClip(l);
        right[at + i] = softClip(r);
      }
      at += take;
      this.offset += take;
      this.played += take;
      if (this.offset >= block.frames) {
        this.blocks.shift();
        this.offset = 0;
      }
    }
    if (at < wanted) {
      left.fill(0, at);
      right.fill(0, at);
      if (this.started) this.underruns += 1;
    }
  }
}
