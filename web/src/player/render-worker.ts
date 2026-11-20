// The Prosody player's render worker (`docs/player.md` §5). It holds the player (`synth.js` and
// `synth_bg.wasm` beside the page: crates/prosody-synth-wasm), which keeps about half a second of
// stems rendered ahead of what the worklet has played, and posts each block straight to the worklet.
// The page's thread is not on that path.
//
// Everything that decides — rendering ahead, when each line of the reading is read, which recorded
// lines to ask for — is the player's own (crates/prosody-player, which the app's native player runs
// too, `docs/player.md` §13). This only moves the page's messages into it, its blocks to the worklet
// and what it says back to the page.
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
//                   { op: 'reading', generation, told }                  what the reading did
//                   { op: 'error', message }
//   to the worklet: { flush, generation }, then { data, frame, frames, generation } blocks: five
//                   stereo stems planar, then the duck (mixer.ts `PER_FRAME` numbers a frame)
//   from it:        { played, generation }
//
// `notes` and `cues` are records (notes.ts); `cast` is [line, render key or null for the browser's
// voice][].
import { PER_FRAME } from './mixer';

interface Player {
  add_drum(id: number, left: Float32Array, right: Float32Array, rate: number): void;
  add_zone(id: number, position: number, left: Float32Array, right: Float32Array, rate: number, root: number, low: number,
    high: number, loopStart: number, loopEnd: number): void;
  cast_of(serial: number, lines: Uint32Array, keys: string[], browser: Uint32Array): void;
  play(generation: number, serial: number, notes: Float64Array, bars: Float64Array, seconds: number, cues: Float64Array,
    pace: string, duck: number): void;
  swap(serial: number, notes: Float64Array, bars: Float64Array, seconds: number, cues: Float64Array): void;
  stop(generation: number): void;
  recast(pace: string, quiet: boolean): void;
  line(key: string, pcm: Float32Array, rate: number, lead: number): void;
  said(chain: number, end: number): void;
  failed(key: string): void;
  set_duck(level: number): void;
  played(generation: number, frame: number): void;
  take_flush(): number | undefined;
  due(): boolean;
  render(out: Float32Array): number;
  drain(): string;
  /** The start of play the blocks rendered now belong to: the last flush's. */
  readonly generation: number;
}
interface Bindings {
  default: () => Promise<unknown>;
  Player: new (rate: number, block: number, lead: number) => Player;
}

const scope = self as unknown as {
  postMessage(message: unknown): void;
  onmessage: ((event: MessageEvent) => void) | null;
};

let player: Player | null = null;
let worklet: MessagePort | null = null;
let block = 2048;
/** The lead's own clock (`pump` every 20 ms), running only while something plays: once a Stop's
 * flush has gone there is nothing to render, and a timer left running woke the worker 50 times a
 * second for as long as the tab was open (the app's native player parks likewise, docs/player.md
 * §13). */
let ticking: ReturnType<typeof setInterval> | undefined;

function tick(on: boolean): void {
  if (on && ticking === undefined) ticking = setInterval(pump, 20);
  else if (!on && ticking !== undefined) {
    clearInterval(ticking);
    ticking = undefined;
  }
}

function say(message: unknown): void {
  scope.postMessage(message);
}

/** What the player has to tell the page. */
function tell(): void {
  if (!player) return;
  for (const event of JSON.parse(player.drain()) as unknown[]) say(event);
}

/** A restart said to the worklet first, then blocks until the lead is full, each straight to the
 * worklet, then what the player has to say. */
function pump(): void {
  if (!player || !worklet) return;
  const flush = player.take_flush();
  if (flush !== undefined) worklet.postMessage({ flush: true, generation: flush });
  while (player.due()) {
    const data = new Float32Array(PER_FRAME * block);
    const frame = player.render(data);
    worklet.postMessage({ data, frame, frames: block, generation: player.generation }, [data.buffer]);
  }
  tell();
}

/** Gives the player who reads piece `serial`'s lines. */
function castOf(serial: number, entries: [number, string | null][] | undefined): void {
  const keyed = (entries ?? []).filter((entry): entry is [number, string] => entry[1] !== null);
  const browser = (entries ?? []).filter(([, key]) => key === null).map(([line]) => line);
  player!.cast_of(serial, Uint32Array.from(keyed.map(([line]) => line)), keyed.map(([, key]) => key), Uint32Array.from(browser));
}

async function handle(data: Record<string, unknown>): Promise<void> {
  switch (data.op) {
    case 'init': {
      const bindings = await (import(/* @vite-ignore */ data.bindings as string) as Promise<Bindings>);
      await bindings.default();
      block = data.block as number;
      player = new bindings.Player(data.rate as number, block, data.lead as number);
      worklet = data.port as MessagePort;
      worklet.onmessage = ({ data: message }: MessageEvent) => {
        player?.played(message.generation as number, message.played as number);
        pump();
      };
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
      if (!player) return;
      castOf(data.serial as number, data.cast as [number, string | null][]);
      player.play(data.generation as number, data.serial as number, data.notes as Float64Array, data.bars as Float64Array,
        data.seconds as number, data.cues as Float64Array, data.pace as string, data.duck as number);
      pump();
      // Messages can be late, a worklet's included; while it plays the lead is kept full on a
      // clock of its own too.
      tick(true);
      return;
    }
    case 'swap':
      if (!player) return;
      castOf(data.serial as number, data.cast as [number, string | null][]);
      player.swap(data.serial as number, data.notes as Float64Array, data.bars as Float64Array, data.seconds as number,
        data.cues as Float64Array);
      tell();
      return;
    case 'stop':
      if (!player) return;
      player.stop(data.generation as number);
      // The flush goes to the worklet now; after it, nothing is due until the next Play.
      pump();
      tick(false);
      return;
    case 'cast':
      if (!player) return;
      for (const [serial, lines] of data.casts as [number, [number, string | null][]][]) castOf(serial, lines);
      player.recast(data.pace as string, !!data.quiet);
      tell();
      return;
    case 'line':
      player?.line(data.key as string, data.pcm as Float32Array, data.rate as number, data.lead as number);
      tell();
      return;
    case 'said':
      player?.said(data.chain as number, data.end as number);
      tell();
      return;
    case 'failed':
      player?.failed(data.key as string);
      tell();
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
