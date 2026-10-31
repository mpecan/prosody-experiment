// The Prosody player's render worker (`docs/player.md` §5). It holds the synth (`synth.js` and
// `synth_bg.wasm` beside the page: crates/prosody-synth-wasm), the samples it has been sent, and the
// Performance playing, and keeps about half a second of stems rendered ahead of what the worklet has
// played, posting each block straight to the worklet. The page's thread is not on that path.
//
//   from the page:  { op: 'init', bindings, rate, block, lead, port }   → { op: 'ready' } or { op: 'error' }
//                   { op: 'drum', id, left, right, rate }
//                   { op: 'zone', id, left, right, rate, root, low, high, loopStart, loopEnd }
//                   { op: 'play', notes, bars, seconds, generation }    from the top
//                   { op: 'swap', notes, bars, seconds }                at the next bar
//                   { op: 'stop', generation }
//   to the page:    { op: 'placement', outStart, pieceStart, serial }   where a piece plays from
//                   { op: 'losses', stolen, missing }
//                   { op: 'error', message }
//   to the worklet: { flush, generation }, then { data, frame, frames, generation } blocks
//   from it:        { played, generation }

interface Player {
  add_drum(id: number, left: Float32Array, right: Float32Array, rate: number): void;
  add_zone(id: number, left: Float32Array, right: Float32Array, rate: number, root: number, low: number, high: number,
    loopStart: number, loopEnd: number): void;
  play(notes: Float64Array, bars: Float64Array, seconds: number): void;
  swap(notes: Float64Array, bars: Float64Array, seconds: number): void;
  stop(): void;
  render(out: Float32Array): void;
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
let worklet: MessagePort | null = null;
let block = 2048;
let lead = 24_000;
let playing = false;
let generation = 0;
/** The render frame the worklet has played up to, as it last said. */
let played = 0;
let losses = { stolen: 0, missing: 0 };

function say(message: Record<string, unknown>): void {
  scope.postMessage(message);
}

/** Renders blocks until the lead is full, each straight to the worklet. */
function pump(): void {
  if (!player || !worklet || !playing) return;
  while (player.frontier - played < lead) {
    const frame = player.frontier;
    const data = new Float32Array(8 * block);
    player.render(data);
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
}

async function handle(data: Record<string, unknown>): Promise<void> {
  switch (data.op) {
    case 'init': {
      const bindings = await (import(/* @vite-ignore */ data.bindings as string) as Promise<Bindings>);
      await bindings.default();
      block = data.block as number;
      lead = data.lead as number;
      player = new bindings.Player(data.rate as number, block);
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
      player?.add_zone(data.id as number, data.left as Float32Array, data.right as Float32Array, data.rate as number,
        data.root as number, data.low as number, data.high as number, data.loopStart as number, data.loopEnd as number);
      return;
    case 'play':
      if (!player) return;
      player.stop();
      restart(data.generation as number);
      player.play(data.notes as Float64Array, data.bars as Float64Array, data.seconds as number);
      playing = true;
      placed(data.serial as number);
      pump();
      return;
    case 'swap':
      if (!player) return;
      player.swap(data.notes as Float64Array, data.bars as Float64Array, data.seconds as number);
      placed(data.serial as number);
      return;
    case 'stop':
      playing = false;
      player?.stop();
      restart(data.generation as number);
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
