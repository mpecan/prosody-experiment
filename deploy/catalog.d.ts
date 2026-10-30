// Types for deploy/catalog.js.
export const CATALOG: Record<string, string>;
export function resolveBase(
  base: string,
  slice?: [number, number],
  patch?: Record<number, string>,
): string | null;
