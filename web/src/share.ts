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
  poem: string;
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

/** The page as it is now, as a link carries it. */
export function current(): Shared {
  const start = startingSettings(site.mappings);
  const defaults = site.palettes.default ?? {};
  const sound = diff(Object.fromEntries(CONTROLS.map((id) => [id, controls.value[id] ?? ''])), defaults);
  const style = paletteOf(controls.value);
  const shared: Shared = { v: 1, poem: poem.value };
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
  const json = new TextEncoder().encode(JSON.stringify(shared));
  return base64url(await through(json, new CompressionStream('deflate-raw')));
}

/** The creation a link's `s` carries, read defensively: null for anything that is not one. */
export async function decode(code: string): Promise<Shared | null> {
  try {
    const json = await through(fromBase64url(code), new DecompressionStream('deflate-raw'), MOST);
    const value = JSON.parse(new TextDecoder().decode(json)) as Record<string, unknown>;
    if (value?.v !== 1 || typeof value.poem !== 'string') return null;
    const strings = (given: unknown, keep: (key: string, value: string) => boolean) => {
      if (!given || typeof given !== 'object') return undefined;
      const out = Object.fromEntries(Object.entries(given as Record<string, unknown>)
        .filter(([key, v]) => typeof v === 'string' && keep(key, v as string)));
      return Object.keys(out).length ? out as Record<string, string> : undefined;
    };
    const shared: Shared = { v: 1, poem: value.poem };
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
