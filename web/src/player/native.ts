// The app's native player (`docs/player.md` §13, design step 5): in the app the page's player runs
// in Rust, on a thread of the app and through the phone's own audio output, so it plays on with the
// screen off. The page drives it exactly as it drives the render worker and the worklet in a
// browser — the same messages — through Tauri's `invoke`:
//
//   player_open { block, lead }   → { rate }        once, when the player starts
//   player_post { message }                         a render worker's message (render-worker.ts)
//   player_mix  { gains, duck }                     a worklet's message (worklet.ts)
//   player_poll                   → { report, events, lost } as JSON
//   player_view { visible }                         the page shown or hidden
//
// Two things differ from the browser. A buffer cannot cross: Android's WebView hands Rust strings
// only (Tauri sends every call as JSON, since that WebView cannot read a request body), so a
// Float32Array or Float64Array goes as base64 of its bytes. And the page asks rather than being told:
// a paused WebView would pile up a call for every report while it slept, so it polls, and what
// happened meanwhile — where a piece took over, which lines were read — waits for it in the app.
//
// `invoke` is Tauri's own (`window.__TAURI_INTERNALS__`, what @tauri-apps/api's `invoke` calls), used
// directly rather than adding that package for one function.
import type { Engine } from './index';

/** How often the page asks how the native player is doing: about as often as the worklet tells it
 * (50 ms), which is what the page's clock is drawn from. */
const POLL = 40;
/** How often it asks while nothing plays: only a lock-screen Play or a failure can be waiting, and
 * the app keeps a Play for 5 s (host.rs `PLAY_KEPT`). Asking every 40 ms for nothing would keep a
 * core waking for as long as the app is open. */
const IDLE_POLL = 500;

interface Internals {
  invoke(command: string, args?: Record<string, unknown>): Promise<unknown>;
}

/** Whether the page is inside the app, with a native player to drive. */
export function inApp(): boolean {
  return typeof globalThis === 'object' && '__TAURI_INTERNALS__' in globalThis;
}

function invoke(command: string, args?: Record<string, unknown>): Promise<unknown> {
  return (globalThis as unknown as { __TAURI_INTERNALS__: Internals }).__TAURI_INTERNALS__.invoke(command, args);
}

/** A typed array's bytes, little-endian as every phone and browser lays them out, as base64. */
export function base64(array: ArrayBufferView): string {
  const bytes = new Uint8Array(array.buffer, array.byteOffset, array.byteLength);
  let binary = '';
  // In slices: String.fromCharCode takes its bytes as arguments, and an engine caps how many.
  for (let at = 0; at < bytes.length; at += 0x8000) binary += String.fromCharCode(...bytes.subarray(at, at + 0x8000));
  return btoa(binary);
}

/** A message as it crosses: each typed array as base64 (a NaN in one survives; JSON writes a NaN
 * number as null, which the app reads as none). */
export function wire(message: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(message).map(([key, value]) => [key, ArrayBuffer.isView(value) ? base64(value) : value]));
}

/** What the native player reports, as the worklet does, and the output's latency. */
export interface NativeReport {
  played: number;
  underruns: number;
  queued: number;
  started: boolean;
  generation: number;
  peak: number;
  speechPeak: number;
  latency: number;
  /** Times the phone's output itself ran short or over (an AAudio xrun). */
  glitches: number;
}

/** What the page gives the native player: what to do with its reports and with what it says. */
export interface Ears {
  /** A report, stamped with the page's clock (`performance.now()`, in seconds). */
  report(report: NativeReport & { time: number }): void;
  /** What the render worker would have said (index.ts `heard`), and the media buttons. */
  heard(message: Record<string, unknown>): void;
  /** Whether the page is playing: it asks often only then. */
  playing(): boolean;
}

/** Opens the native player, rendering `block` frames at a time `lead` seconds ahead. */
export async function startNative(block: number, lead: number, ears: Ears): Promise<Engine> {
  const { rate } = (await invoke('player_open', { block, lead })) as { rate: number };
  let latency = 0;
  // In order, one after another: Tauri may run two calls at once, and a zone must reach the player
  // before the piece that plays it.
  let queue: Promise<void> = Promise.resolve();
  const failed = (error: unknown) => ears.heard({ op: 'error', message: String((error as Error)?.message ?? error) });
  // Asked every POLL while playing, every IDLE_POLL while not; a message posted (a Play among them)
  // brings a slow ask forward, so the first reports of a piece are not half a second late.
  let asking = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let slow = false;
  const ask = () => {
    timer = undefined;
    asking = true;
    invoke('player_poll')
      .then((answer) => {
        const told = JSON.parse(answer as string) as { report: NativeReport; events: Record<string, unknown>[]; lost: number } | null;
        if (!told) return;
        latency = told.report.latency;
        ears.report({ ...told.report, time: performance.now() / 1000 });
        if (told.lost) console.warn(`[player] ${told.lost} message(s) from the native player were let go while the page was asleep`);
        for (const message of told.events) ears.heard(message);
      })
      .catch(failed)
      .finally(() => {
        asking = false;
        next();
      });
  };
  const next = (soon = false) => {
    if (asking || (timer !== undefined && !(slow && soon))) return;
    clearTimeout(timer);
    slow = !soon && !ears.playing();
    timer = setTimeout(ask, slow ? IDLE_POLL : POLL);
  };
  next();
  const post = (message: Record<string, unknown>) => {
    const crossing = wire(message);
    queue = queue.then(() => invoke('player_post', { message: crossing })).then(() => undefined, failed);
    next(true);
  };
  // Hidden (the screen off), the app keeps the lead wide enough for the largest buffer its output
  // has asked for; shown again, it narrows it to what the buffers now need, so a live edit is heard
  // about as soon as in a browser (`docs/player.md` §13).
  document.addEventListener('visibilitychange', () => {
    invoke('player_view', { visible: document.visibilityState === 'visible' }).catch(failed);
  });
  return {
    rate,
    post,
    mix: (message) => {
      queue = queue.then(() => invoke('player_mix', { gains: message.gains ?? null, duck: message.duck ?? null })).then(() => undefined, failed);
    },
    now: () => performance.now() / 1000,
    latency: () => latency,
    resume: async () => {},
  };
}
