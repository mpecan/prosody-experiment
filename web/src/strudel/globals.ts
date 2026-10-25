// Strudel, as the page uses it. `@strudel/web` (loaded by index.html) puts every export of
// `@strudel/core` and `@strudel/webaudio` on the global scope once `initStrudel()` has run; the
// compiled pattern is plain code calling those globals, evaluated with an indirect `eval`. These are
// the few the page calls itself, typed as far as it uses them.

/** A point in Strudel's time: a fraction of cycles. */
export interface Fraction {
  valueOf(): number;
}

/** One event of a pattern. */
export interface Hap {
  whole: { begin: Fraction; end: Fraction } | null;
  value: Record<string, unknown> | null;
  hasOnset(): boolean;
}

export interface Pattern {
  play(): void;
  /** `dominant` true makes the callback the pattern's only output (@strudel/core pattern.mjs). */
  onTrigger(fn: (hap: Hap, currentTime: number, cps: number, targetTime: number) => void, dominant: boolean): Pattern;
  queryArc(begin: number, end: number): Hap[];
  /** The same pattern with every event's value mapped — at query time, so `fn` may read state. */
  withValue(fn: (value: Record<string, unknown>) => Record<string, unknown>): Pattern;
}

export interface Scheduler {
  pattern: Pattern | null;
  lastBegin: number;
  lastEnd: number;
}

export interface Repl {
  setCps(cps: number): void;
  scheduler: Scheduler;
}

interface Sound {
  onTrigger: (time: number, value: Record<string, unknown> | undefined, ...rest: unknown[]) => unknown;
  data?: { type?: string; nudged?: boolean };
}

interface Strudel {
  initStrudel(options: { prebake: () => Promise<unknown> }): Promise<Repl>;
  samples(map: string, base: string, options: Record<string, unknown>): Promise<unknown>;
  setStringParser(parser: (text: string) => Pattern): void;
  mini(text: string): Pattern;
  soundMap: { get(): Record<string, Sound> };
  registerSound(name: string, onTrigger: Sound['onTrigger'], data: Record<string, unknown>): void;
  getSuperdoughAudioController(): { output?: { destinationGain?: GainNode } };
  getAudioContext(): AudioContext;
  superdough(value: Record<string, unknown>, time: number, duration: number): Promise<unknown>;
  initAudio(): Promise<void>;
  stack(...patterns: Pattern[]): Pattern;
  hush?: () => void;
}

/** Strudel's globals. Only defined once `@strudel/web` has loaded, and most once `initStrudel` has run. */
export const strudel = globalThis as unknown as Strudel;

/** Evaluates generated code in the global scope, where Strudel's functions are. */
export const evaluate = <T = Pattern>(code: string): T => (0, eval)(code) as T;

/** The audio context Strudel plays in. */
export const audioContext = () => strudel.getAudioContext();
