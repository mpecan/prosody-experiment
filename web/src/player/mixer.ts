// What the AudioWorklet does with the blocks the render worker sends (`docs/player.md` §5): queue
// them, mix each stem at its gain — mute and solo are a gain of 0, glided to over a few ms — through
// the master stage, and count the times the queue ran dry. Kept apart from the worklet itself so it
// can be tested without an audio thread; `worklet.ts` is the thin shell that runs it.
//
// The master is the chain the page built for Strudel (strudel/index.ts `limitTheMaster`), in its
// order, and the same as `prosody_synth::master` does for a file: the stems at their gains; the music
// (every stem but the reading's) at the duck the block carries; the glue compressor on the music and
// the reading together; the soft clip. The page fed its WaveShaper a quarter of the signal because a
// curve spans −1…1 and its curve covered ±4; computed on the sample itself here, that quarter and the
// curve's ×4 cancel.

/** The stems a block holds, each stereo (prosody_synth::STEMS): drums, bass, melody, chords and the
 * reading. */
export const STEMS = 5;
/** The reading's stem: the duck lowers the music under it, never it. */
export const SPEECH = 4;
/** Numbers a block holds a frame: two a stem, and the duck (prosody_synth `Stems::PER_FRAME`). */
export const PER_FRAME = 2 * STEMS + 1;

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
// the ceiling above it, flat beyond the range.
const CLIP_KNEE = 0.6, CLIP_CEILING = 0.95, CLIP_RANGE = 4;
export function softClip(x: number): number {
  const a = Math.min(Math.abs(x), CLIP_RANGE);
  const bend = CLIP_CEILING - CLIP_KNEE;
  const shaped = a <= CLIP_KNEE ? a : CLIP_KNEE + bend * Math.tanh((a - CLIP_KNEE) / bend);
  return x < 0 ? -shaped : shaped;
}

// The glue: the page's DynamicsCompressorNode settings (−6 dB, 8:1, a 6 dB knee, 3 ms attack, 150 ms
// release), as `prosody_synth::dynamics` computes a compressor — a quadratic knee from the threshold,
// the reduction smoothed in decibels (attack × ATTACK_SCALE, release × RELEASE_SCALE, fitted to
// Chrome's node), and Web Audio's makeup gain (1/curve(0 dBFS))^0.6. docs/player.md §11 has the
// measurements. The two must stay the same; player.test.ts pins this one with the numbers the
// Rust tests pin that one with.
const GLUE = { threshold: -6, ratio: 8, knee: 6, attack: 0.003, release: 0.15 };
const ATTACK_SCALE = 0.2, RELEASE_SCALE = 6;
/** The glue's static curve: level out for level in, in dB. */
export function glueCurve(x: number): number {
  const over = x - GLUE.threshold, slope = 1 / GLUE.ratio - 1;
  if (over <= 0) return x;
  if (over < GLUE.knee) return x + slope * over * over / (2 * GLUE.knee);
  return x + slope * (over - GLUE.knee / 2);
}
/** The glue's makeup, as a factor: 10^(−0.6·curve(0)/20) = 1.575 dB. */
export const MAKEUP = 10 ** (-0.6 * glueCurve(0) / 20);

/** The glue compressor's running state. */
export class Glue {
  private reduction = 0;
  private readonly attack: number;
  private readonly release: number;
  constructor(rate: number) {
    this.attack = Math.exp(-1 / (GLUE.attack * ATTACK_SCALE * rate));
    this.release = Math.exp(-1 / (GLUE.release * RELEASE_SCALE * rate));
  }
  /** The gain for a frame whose louder side is `level`. */
  gain(level: number): number {
    const x = 20 * Math.log10(Math.max(Math.abs(level), 1e-5));
    const wanted = glueCurve(x) - x;
    const a = wanted < this.reduction ? this.attack : this.release;
    this.reduction = a * this.reduction + (1 - a) * wanted;
    return 10 ** (this.reduction / 20) * MAKEUP;
  }
}

/** A rendered block: `frames` frames from render frame `frame`, the stems planar (each stem's left
 * side, then its right), then the duck. */
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
  private glue: Glue;

  /** A queue playing at `rate` frames a second (the glue's time constants are in seconds). */
  constructor(rate = 48_000) {
    this.glue = new Glue(rate);
  }

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
        const duck = at.block.data[2 * STEMS * at.block.frames + at.offset] ?? 1;
        for (let stem = 0; stem < STEMS; stem++) {
          const gain = this.gains[stem]! * fade * (stem === SPEECH ? 1 : duck);
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

  /** Fills `left` and `right` with the next frames: each stem at its gain (gliding to the `gains`
   * asked for, a 1/`GLIDE` step a frame), the music at the block's duck, plus any tail fading from a
   * flush, through the glue and the soft clip. Frames the queue cannot fill are silence, and a
   * quantum short of music counts one underrun. */
  mix(left: Float32Array, right: Float32Array, gains: readonly number[]): void {
    let short = false;
    for (let i = 0; i < left.length; i++) {
      let l = 0, r = 0;
      const block = this.blocks[0];
      if (block) {
        let sl = 0, sr = 0;
        for (let stem = 0; stem < STEMS; stem++) {
          const gain = this.glide(stem, gains[stem] ?? 1);
          if (gain === 0) continue;
          const base = stem * 2 * block.frames + this.offset;
          if (stem === SPEECH) {
            sl += gain * block.data[base]!;
            sr += gain * block.data[base + block.frames]!;
          } else {
            l += gain * block.data[base]!;
            r += gain * block.data[base + block.frames]!;
          }
        }
        const duck = block.data[2 * STEMS * block.frames + this.offset] ?? 1;
        l = l * duck + sl;
        r = r * duck + sr;
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
      const glue = this.glue.gain(Math.max(Math.abs(l), Math.abs(r)));
      left[i] = softClip(glue * l);
      right[i] = softClip(glue * r);
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
