// The Prosody player's render worker (`docs/player.md` §5). It holds the synth (`synth.js` and
// `synth_bg.wasm` beside the page: crates/prosody-synth-wasm), the samples it has been sent, and the
// Performance playing, and keeps about half a second of stems rendered ahead of what the worklet has
// played, posting each block straight to the worklet. The page's thread is not on that path.
//
// It also times the reading (reader.ts): before each block it places the lines whose cues fall in it
// on the speech stem — the recorded lines the page has sent, by render key — and tells the page when
// a line the browser's own voice reads is due.
//
//   from the page:  { op: 'init', bindings, rate, block, lead, port }   → { op: 'ready' } or { op: 'error' }
//                   { op: 'drum', id, left, right, rate }
//                   { op: 'zone', id, position, left, right, rate, root, low, high, loopStart, loopEnd }
//                   { op: 'play', notes, bars, seconds, generation, serial, cues, cast, pace, duck }   from the top
//                   { op: 'swap', notes, bars, seconds, serial, cues, cast }                          at the next bar
//                   { op: 'stop', generation }
//                   { op: 'cast', casts: [serial, [line, key | null][]][], pace, quiet }   who reads each line
//                   { op: 'line', key, pcm, rate, lead }                 a recorded line, mono
//                   { op: 'said', chain, end }                           the browser's voice finished a line
//                   { op: 'failed', key }                                a line could not be rendered
//                   { op: 'duck', level }                                the music's level under a line
//   to the page:    { op: 'placement', outStart, pieceStart, serial }   where a piece plays from
//                   { op: 'losses', stolen, missing }
//                   { op: 'want', keys }                                 the recorded lines to send
//                   { op: 'dropped', keys }                              the recorded lines let go
//                   { op: 'reading', generation, told }                  what the reading did (reader.ts `Told`)
//                   { op: 'error', message }
//   to the worklet: { flush, generation }, then { data, frame, frames, generation } blocks: five
//                   stereo stems planar, then the duck (mixer.ts `PER_FRAME` numbers a frame)
//   from it:        { played, generation }
import type { PerformedCue } from '../answers';
import { PER_FRAME } from './mixer';
import { frameOf, Reader, type Cast, type Pace, type Segment } from './reader';

interface Player {
  add_drum(id: number, left: Float32Array, right: Float32Array, rate: number): void;
  add_zone(id: number, position: number, left: Float32Array, right: Float32Array, rate: number, root: number, low: number,
    high: number, loopStart: number, loopEnd: number): void;
  play(notes: Float64Array, bars: Float64Array, seconds: number): void;
  swap(notes: Float64Array, bars: Float64Array, seconds: number): void;
  stop(): void;
  render(out: Float32Array): void;
  add_line(id: number, pcm: Float32Array, rate: number): void;
  drop_line(id: number): void;
  speak(id: number, start: number, speech: number, hold: number): number;
  cut_speech(at: number): void;
  set_duck(level: number): void;
  readonly frontier: number;
  readonly out_start: number;
  readonly piece_start: number;
  readonly stolen: number;
  readonly missing: number;
}
interface Bindings {
  default: () => Promise<unknown>;
  Player: new (rate: number, block: number) => Player;
}

const scope = self as unknown as {
  postMessage(message: unknown): void;
  onmessage: ((event: MessageEvent) => void) | null;
};

let player: Player | null = null;
let reader: Reader | null = null;
let worklet: MessagePort | null = null;
let rate = 48_000;
let block = 2048;
let lead = 24_000;
let playing = false;
let generation = 0;
/** The render frame the worklet has played up to, as it last said. */
let played = 0;
let losses = { stolen: 0, missing: 0 };
/** Numbers the synth's lines are free to take: a line let go gives its number back. */
const freeLines: number[] = [];
let nextLine = 0;

function say(message: Record<string, unknown>): void {
  scope.postMessage(message);
}

/** Renders blocks until the lead is full, each straight to the worklet, the reading placed first. */
function pump(): void {
  if (!player || !worklet || !playing) return;
  while (player.frontier - played < lead) {
    const frame = player.frontier;
    reader?.before(frame, frame + block);
    const data = new Float32Array(PER_FRAME * block);
    player.render(data);
    reader?.at(player.frontier);
    worklet.postMessage({ data, frame, frames: block, generation }, [data.buffer]);
  }
  const now = { stolen: player.stolen, missing: player.missing };
  if (now.stolen !== losses.stolen || now.missing !== losses.missing) {
    losses = now;
    say({ op: 'losses', ...now });
  }
}

function placed(serial: number): void {
  if (player) say({ op: 'placement', outStart: player.out_start, pieceStart: player.piece_start, serial });
}

/** A new start of play: everything queued is dropped, and the lead is counted from the frontier. */
function restart(next: number): void {
  generation = next;
  worklet?.postMessage({ flush: true, generation });
  played = player?.frontier ?? 0;
  reader?.at(played);
}

/** Who reads each line, as the page sends it. */
function castOf(entries: [number, string | null][] | undefined): Cast {
  return new Map(entries ?? []);
}

/** The piece just handed to the synth, as the reader times it. */
function segmentOf(data: Record<string, unknown>): Segment | null {
  if (!player) return null;
  const bars = [...(data.bars as Float64Array)];
  return {
    serial: data.serial as number,
    outStart: player.out_start,
    pieceStart: player.piece_start,
    length: frameOf(data.seconds as number, rate),
    bars: [...bars, data.seconds as number],
    cues: (data.cues as PerformedCue[] | undefined) ?? [],
    cast: castOf(data.cast as [number, string | null][] | undefined),
  };
}

/** Lets go of the recorded lines no piece playing or to play casts, and asks the page for those it
 * casts and the synth does not hold. */
function wantNow(): void {
  if (!player || !reader) return;
  const wanted = reader.wanted();
  const gone: string[] = [];
  for (const [key, held] of reader.held) {
    if (wanted.has(key)) continue;
    player.drop_line(held.id);
    freeLines.push(held.id);
    reader.held.delete(key);
    gone.push(key);
  }
  if (gone.length) say({ op: 'dropped', keys: gone });
  say({ op: 'want', keys: [...wanted].filter((key) => !reader!.held.has(key)) });
}

function startReader(): Reader {
  return new Reader(rate, {
    speak: (id, start, speech, hold) => {
      try {
        return player!.speak(id, start, speech, hold);
      } catch (error) {
        say({ op: 'error', message: 'a line of the reading could not be placed: ' + String((error as Error)?.message ?? error) });
        return null;
      }
    },
    cut: (at) => player?.cut_speech(at),
    tell: (told) => say({ op: 'reading', generation, told }),
  });
}

async function handle(data: Record<string, unknown>): Promise<void> {
  switch (data.op) {
    case 'init': {
      const bindings = await (import(/* @vite-ignore */ data.bindings as string) as Promise<Bindings>);
      await bindings.default();
      block = data.block as number;
      lead = data.lead as number;
      rate = data.rate as number;
      player = new bindings.Player(rate, block);
      reader = startReader();
      worklet = data.port as MessagePort;
      worklet.onmessage = ({ data: message }: MessageEvent) => {
        if (message.generation !== generation) return;
        played = message.played as number;
        pump();
      };
      // Messages can be late, a worklet's included; the lead is kept full on a clock of its own too.
      setInterval(pump, 20);
      say({ op: 'ready' });
      return;
    }
    case 'drum':
      player?.add_drum(data.id as number, data.left as Float32Array, data.right as Float32Array, data.rate as number);
      return;
    case 'zone':
      player?.add_zone(data.id as number, data.position as number, data.left as Float32Array, data.right as Float32Array,
        data.rate as number, data.root as number, data.low as number, data.high as number, data.loopStart as number, data.loopEnd as number);
      return;
    case 'play': {
      if (!player || !reader) return;
      player.stop();
      restart(data.generation as number);
      player.set_duck(data.duck as number);
      player.play(data.notes as Float64Array, data.bars as Float64Array, data.seconds as number);
      reader.pace = data.pace as Pace;
      const segment = segmentOf(data);
      if (segment) reader.play(segment);
      playing = true;
      placed(data.serial as number);
      wantNow();
      pump();
      return;
    }
    case 'swap': {
      // A hand-over with nothing playing is stale: a play() sent it after a Stop it had not
      // noticed. The synth would take it as a start of play, but nothing here would pump it, and
      // the page would wait on silence; a start of play comes as 'play'.
      if (!player || !reader || !playing) return;
      player.swap(data.notes as Float64Array, data.bars as Float64Array, data.seconds as number);
      const segment = segmentOf(data);
      if (segment) reader.swap(segment);
      placed(data.serial as number);
      wantNow();
      return;
    }
    case 'stop':
      playing = false;
      player?.stop();
      reader?.stop();
      restart(data.generation as number);
      // The recorded lines are kept: a Play straight after is most likely the same piece, and the
      // next piece handed over lets go of any it does not cast.
      return;
    case 'cast': {
      if (!reader) return;
      const casts = new Map((data.casts as [number, [number, string | null][]][]).map(([serial, lines]) => [serial, castOf(lines)]));
      reader.recast(casts, data.pace as Pace, !!data.quiet);
      wantNow();
      return;
    }
    case 'line': {
      if (!player || !reader) return;
      // Taken whether or not a piece casts it yet: the page sends a piece's lines just before the
      // piece, and the next `wantNow` lets go of any no piece casts.
      const key = data.key as string;
      if (reader.held.has(key)) return;
      const pcm = data.pcm as Float32Array;
      const lineRate = data.rate as number;
      const id = freeLines.pop() ?? nextLine++;
      player.add_line(id, pcm, lineRate);
      reader.held.set(key, { id, lead: data.lead as number, seconds: pcm.length / lineRate });
      reader.arrived(key);
      return;
    }
    case 'said':
      reader?.said(data.chain as number, data.end as number);
      return;
    case 'failed':
      reader?.failed(data.key as string);
      return;
    case 'duck':
      player?.set_duck(data.level as number);
      return;
  }
}

// One message at a time, in order: a zone must be loaded before the piece that plays it.
let queue: Promise<void> = Promise.resolve();
scope.onmessage = ({ data }: MessageEvent) => {
  queue = queue.then(() => handle(data)).catch((error: unknown) => {
    say({ op: 'error', message: String((error as Error)?.message ?? error) });
  });
};
