// Sharing without storage. The compiler is deterministic (I1): a poem and its settings are the whole
// of a piece, and a few hundred bytes compressed. So a creation is shared as a link that carries it
// — `?s=` and the poem, its sound and its A/B settings, as compressed JSON in base64url — and
// nothing is kept anywhere: opening the link compiles it again in the visitor's browser. It is a
// query rather than a fragment so the site's worker can read it for the link's preview
// (deploy/preview.js), which it does without keeping it either.
//
// Only what differs from where a visit starts is written — the default palette, A at every default,
// B at what preceded each mapping — so a link stays short. A link from an older page reads as far
// as it can: anything this page no longer knows falls back, as a stale save does (settings.ts).
import { CONTROLS, paletteOf } from './controls';
import { site } from './data';
import { restore, startingSettings } from './settings';
import type { Palette } from './site';
import { controls, hearing, language, poem, settings, type Setting, type Side } from './store';

/** What a link carries. `v` is the format, for a later page to read an older link by. */
export interface Shared {
  v: 1;
  poem?: string;
  /** A known public base poem (e.g. from site.presets). */
  base?: string;
  /** A line range [start, end] into the base poem. */
  slice?: [number, number];
  /** Line replacements { [lineIndex]: newLineText } against base/slice. */
  patch?: Record<number, string>;
  /** The controls that differ from the default palette. */
  sound?: Palette;
  /** The variants that differ from where each side starts. */
  A?: Setting;
  B?: Setting;
  /** The side heard, when it is B. */
  hear?: Side;
  /** The language the poem is read in, when not English. */
  lang?: string;
  /** The style the sound matches, for the link's preview only. */
  style?: string;
}

/** The most a link may unpack to: far more than any poem, and a stop to a link made to explode. */
export const MOST = 64 * 1024;

const diff = <T extends Record<string, string>>(now: T, start: T): Partial<T> | undefined => {
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(now)) if (start[key] !== value) out[key] = value;
  return Object.keys(out).length ? out as Partial<T> : undefined;
};

/** Resolves a base poem by name from site presets, applying optional slice and line patch. */
export function resolveBase(
  base: string,
  slice?: [number, number],
  patch?: Record<number, string>,
): string | null {
  const preset = site?.presets?.find((p) => p.name === base);
  if (!preset) return null;
  let lines = preset.poem.split('\n');
  if (Array.isArray(slice) && slice.length === 2) {
    const [start, end] = slice;
    if (typeof start === 'number' && typeof end === 'number' && start >= 0 && end <= lines.length && start <= end) {
      lines = lines.slice(start, end);
    }
  }
  if (patch && typeof patch === 'object') {
    for (const [idxStr, repl] of Object.entries(patch)) {
      const idx = Number(idxStr);
      if (Number.isInteger(idx) && idx >= 0 && idx < lines.length && typeof repl === 'string') {
        lines[idx] = repl;
      }
    }
  }
  return lines.join('\n');
}

export interface BaseMatch {
  base: string;
  slice?: [number, number];
  patch?: Record<number, string>;
}

/** Matches text against known presets to find exact, sliced, or patched representations. */
export function matchBase(text: string): BaseMatch | null {
  if (!site?.presets || !text) return null;

  // 1. Exact match against a preset poem
  for (const preset of site.presets) {
    if (preset.poem === text) {
      return { base: preset.name };
    }
  }

  const lines = text.split('\n');
  const nonEmptyLines = lines.filter((l) => l.trim().length > 0);

  // 2. Contiguous slice (at least 2 non-empty lines, >= 40 chars total)
  if (nonEmptyLines.length >= 2 && text.length >= 40) {
    for (const preset of site.presets) {
      const baseLines = preset.poem.split('\n');
      if (lines.length >= baseLines.length) continue;

      for (let start = 0; start <= baseLines.length - lines.length; start++) {
        let match = true;
        for (let i = 0; i < lines.length; i++) {
          if (baseLines[start + i] !== lines[i]) {
            match = false;
            break;
          }
        }
        if (match) {
          return { base: preset.name, slice: [start, start + lines.length] };
        }
      }
    }
  }

  // 3. Small line patch on exact poem (same line count, <= 3 differing lines and <= 25% of lines)
  for (const preset of site.presets) {
    const baseLines = preset.poem.split('\n');
    if (lines.length === baseLines.length && lines.length >= 4) {
      const patch: Record<number, string> = {};
      let diffCount = 0;
      for (let i = 0; i < lines.length; i++) {
        if (baseLines[i] !== lines[i]) {
          diffCount++;
          patch[i] = lines[i];
          if (diffCount > 3) break;
        }
      }
      if (diffCount > 0 && diffCount <= 3 && diffCount / lines.length <= 0.25) {
        return { base: preset.name, patch };
      }
    }
  }

  // 4. Small line patch on a slice (>= 4 lines, exactly 1 differing line)
  if (nonEmptyLines.length >= 4 && text.length >= 80) {
    for (const preset of site.presets) {
      const baseLines = preset.poem.split('\n');
      if (lines.length >= baseLines.length) continue;

      for (let start = 0; start <= baseLines.length - lines.length; start++) {
        const patch: Record<number, string> = {};
        let diffCount = 0;
        for (let i = 0; i < lines.length; i++) {
          if (baseLines[start + i] !== lines[i]) {
            diffCount++;
            patch[i] = lines[i];
            if (diffCount > 1) break;
          }
        }
        if (diffCount === 1) {
          return { base: preset.name, slice: [start, start + lines.length], patch };
        }
      }
    }
  }

  return null;
}

/** The page as it is now, as a link carries it. */
export function current(): Shared {
  const start = startingSettings(site.mappings);
  const defaults = site.palettes.default ?? {};
  const sound = diff(Object.fromEntries(CONTROLS.map((id) => [id, controls.value[id] ?? ''])), defaults);
  const style = paletteOf(controls.value);
  const matched = matchBase(poem.value);
  const shared: Shared = { v: 1 };
  if (matched) {
    shared.base = matched.base;
    if (matched.slice) shared.slice = matched.slice;
    if (matched.patch) shared.patch = matched.patch;
  } else {
    shared.poem = poem.value;
  }
  if (sound) shared.sound = sound as Palette;
  const a = diff(settings.value.A, start.A);
  const b = diff(settings.value.B, start.B);
  if (a) shared.A = a as Setting;
  if (b) shared.B = b as Setting;
  if (hearing.value === 'B') shared.hear = 'B';
  if (language.value !== 'en') shared.lang = language.value;
  if (style !== 'custom') shared.style = style;
  return shared;
}

const base64url = (bytes: Uint8Array) =>
  btoa(String.fromCharCode(...bytes)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const fromBase64url = (text: string) =>
  Uint8Array.from(atob(text.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));

async function through(bytes: Uint8Array, stream: CompressionStream | DecompressionStream, most = Infinity): Promise<Uint8Array> {
  const reader = new Blob([bytes as BlobPart]).stream().pipeThrough(stream).getReader();
  const parts: Uint8Array[] = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > most) { await reader.cancel(); throw new Error('too long'); }
    parts.push(value);
  }
  const out = new Uint8Array(size);
  let at = 0;
  for (const part of parts) { out.set(part, at); at += part.length; }
  return out;
}

/** A creation as the `s` of a link. */
export async function encode(shared: Shared): Promise<string> {
  const payload = { ...shared };
  if (payload.base) {
    delete payload.poem;
  }
  const json = new TextEncoder().encode(JSON.stringify(payload));
  return base64url(await through(json, new CompressionStream('deflate-raw')));
}

/** The creation a link's `s` carries, read defensively: null for anything that is not one. */
export async function decode(code: string): Promise<Shared | null> {
  try {
    const json = await through(fromBase64url(code), new DecompressionStream('deflate-raw'), MOST);
    const value = JSON.parse(new TextDecoder().decode(json)) as Record<string, unknown>;
    if (value?.v !== 1) return null;

    let poemText: string | null = null;
    if (typeof value.poem === 'string' && value.poem.trim()) {
      poemText = value.poem;
    } else if (typeof value.base === 'string') {
      const slice = Array.isArray(value.slice) && value.slice.length === 2
        && typeof value.slice[0] === 'number' && typeof value.slice[1] === 'number'
        ? [value.slice[0], value.slice[1]] as [number, number]
        : undefined;
      const patch = value.patch && typeof value.patch === 'object'
        ? Object.fromEntries(
            Object.entries(value.patch as Record<string, unknown>)
              .filter(([k, v]) => /^\d+$/.test(k) && typeof v === 'string')
              .map(([k, v]) => [Number(k), v as string]),
          )
        : undefined;
      poemText = resolveBase(value.base, slice, patch);
    }
    if (!poemText || !poemText.trim()) return null;

    const strings = (given: unknown, keep: (key: string, value: string) => boolean) => {
      if (!given || typeof given !== 'object') return undefined;
      const out = Object.fromEntries(Object.entries(given as Record<string, unknown>)
        .filter(([key, v]) => typeof v === 'string' && keep(key, v as string)));
      return Object.keys(out).length ? out as Record<string, string> : undefined;
    };
    const shared: Shared = { v: 1, poem: poemText };
    if (typeof value.base === 'string') shared.base = value.base;
    if (Array.isArray(value.slice) && value.slice.length === 2 && typeof value.slice[0] === 'number' && typeof value.slice[1] === 'number') {
      shared.slice = [value.slice[0], value.slice[1]];
    }
    if (value.patch && typeof value.patch === 'object') {
      const patchObj = Object.fromEntries(
        Object.entries(value.patch as Record<string, unknown>)
          .filter(([k, v]) => /^\d+$/.test(k) && typeof v === 'string')
          .map(([k, v]) => [Number(k), v as string]),
      );
      if (Object.keys(patchObj).length > 0) shared.patch = patchObj;
    }
    // Only the controls the page has, as short words or numbers; the compiler checks the rest.
    const sound = strings(value.sound, (key, v) => CONTROLS.includes(key) && v.length <= 40);
    const a = strings(value.A, () => true);
    const b = strings(value.B, () => true);
    if (sound) shared.sound = sound;
    if (a) shared.A = a;
    if (b) shared.B = b;
    if (value.hear === 'B') shared.hear = 'B';
    if (typeof value.lang === 'string' && /^[a-z]{2,3}$/.test(value.lang)) shared.lang = value.lang;
    if (typeof value.style === 'string' && value.style in site.palettes) shared.style = value.style;
    return shared;
  } catch {
    return null;
  }
}

/** A link to `shared` on this page. */
export async function linkTo(shared: Shared): Promise<string> {
  const here = new URL(location.href);
  here.search = '';
  here.hash = '';
  here.searchParams.set('s', await encode(shared));
  return here.toString();
}

/** The settings a shared creation sets: its sound over the default palette, its sides over where
 * each starts — anything unknown falling back. */
export function settingsOf(shared: Shared): { sound: Palette; sides: Record<Side, Setting> } {
  const start = startingSettings(site.mappings);
  return {
    sound: { ...(site.palettes.default ?? {}), ...shared.sound },
    sides: {
      A: restore(site.mappings, start.A, { ...start.A, ...shared.A }, false),
      B: restore(site.mappings, start.B, { ...start.B, ...shared.B }, false),
    },
  };
}
