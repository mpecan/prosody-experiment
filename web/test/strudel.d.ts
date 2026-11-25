// What the parity test (parity.test.ts) calls of Strudel's packages, which ship no types: their
// public API as Strudel documents it (the packages' READMEs, strudel.cc), nothing more.
declare module '@strudel/core' {
  /** Puts the given modules' exports in scope for `evaluate`. */
  export function evalScope(...modules: unknown[]): Promise<unknown>;
  /** Evaluates pattern code, transpiled, and gives back the pattern it ends with. */
  export function evaluate(code: string, transpiler: unknown): Promise<{ pattern: unknown }>;
}

declare module '@strudel/mini' {}

declare module '@strudel/transpiler' {
  /** Turns double-quoted strings into mini-notation, for `evaluate`. */
  export const transpiler: unknown;
}
