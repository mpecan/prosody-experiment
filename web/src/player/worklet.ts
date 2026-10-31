// The Prosody player's AudioWorklet (`docs/player.md` §5): it only mixes. The render worker sends
// finished blocks of stems straight here over a MessageChannel; this queues them, mixes them at the
// mixer's stem gains through the master (mixer.ts), and says how far it has played — to the worker,
// which keeps about half a second rendered ahead of that, and to the page, which draws from it. The
// page's own thread is on neither path, so a busy page cannot make a note late. Built by Vite as a
// module of its own (`?worker&url`).
import { StemQueue } from './mixer';

// The AudioWorklet global scope, as far as this uses it: TypeScript's DOM library does not declare it.
declare const currentTime: number;
declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
}
declare function registerProcessor(name: string, processor: new () => AudioWorkletProcessor): void;

/** How often the worker hears how far the output has got: about every 20 ms at 48 kHz. */
const TELL_WORKER = 1024;
/** How often the page does: about every 50 ms. */
const TELL_PAGE = Math.round(sampleRate * 0.05);

class PlayerProcessor extends AudioWorkletProcessor {
  private queue = new StemQueue(sampleRate);
  private gains = [1, 1, 1, 1, 1];
  /** The music's level the page asks for while the browser's own voice reads a line. */
  private duck = 1;
  private worker: MessagePort | null = null;
  /** Which start of play the blocks wanted belong to: blocks still in flight from before a stop or
   * a restart are dropped, not heard. */
  private generation = 0;
  private sinceWorker = 0;
  private sincePage = 0;
  /** The loudest sample sent out since the page was last told: proof of sound, for the page. */
  private peak = 0;

  constructor() {
    super();
    this.port.onmessage = ({ data }: MessageEvent) => {
      if (data.worker) {
        this.worker = data.worker as MessagePort;
        // A flush comes down the same channel as the blocks, ahead of the new ones, so nothing
        // rendered before a stop or a restart can arrive after it.
        this.worker.onmessage = ({ data: message }: MessageEvent) => {
          if (message.flush) {
            this.generation = message.generation as number;
            this.queue.flush();
            this.tellPage();
          } else if (message.data && message.generation === this.generation) {
            this.queue.push(message);
          }
        };
      }
      if (data.gains) this.gains = data.gains as number[];
      if (typeof data.duck === 'number') this.duck = data.duck;
    };
  }

  process(_inputs: Float32Array[][], outputs: Float32Array[][]): boolean {
    const [left, right] = outputs[0] ?? [];
    if (!left) return true;
    this.queue.mix(left, right ?? new Float32Array(left.length), this.gains, this.duck);
    for (let i = 0; i < left.length; i++) this.peak = Math.max(this.peak, Math.abs(left[i]!), Math.abs(right?.[i] ?? 0));
    this.sinceWorker += left.length;
    this.sincePage += left.length;
    if (this.sinceWorker >= TELL_WORKER) {
      this.sinceWorker = 0;
      this.worker?.postMessage({ played: this.queue.played, generation: this.generation });
    }
    if (this.sincePage >= TELL_PAGE) {
      this.sincePage = 0;
      this.tellPage();
    }
    return true;
  }

  private tellPage(): void {
    this.port.postMessage({
      played: this.queue.played,
      underruns: this.queue.underruns,
      queued: this.queue.queued,
      started: this.queue.started,
      generation: this.generation,
      peak: this.peak,
      speechPeak: this.queue.speechPeak,
      time: currentTime,
    });
    this.peak = 0;
    this.queue.speechPeak = 0;
  }
}

registerProcessor('prosody-player', PlayerProcessor);
