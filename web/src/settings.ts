// The two settings being compared: a variant for every mapping, by name. A is the piece as
// specified — every default — and B the piece before any candidate mapping, each at its first
// variant, what preceded it (off; for the pulse, which has no off, elastic; for the stack,
// which has none either, crossing), until the writer says otherwise.
import { site } from './data';
import type { MappingInfo } from './site';
import { load, loadJson, save } from './storage';
import { settings, suggested, type Setting, type Side } from './store';

// One exception, `long-stanzas`: B starts where A does. Its first variant, `gradual`, is the
// quadratic arrangement erosion exists to remove (SPEC.md §4.6) — for "Patterns" 1,314 bars and a
// 4 MB pattern, which the static page's compiler takes about 5 s to write and 8 s to score, and
// Strudel seconds more to parse, on every edit, while the A side waits for it — so it is there
// to choose, not loaded behind every long poem.
export const B_STARTS_AS_A = ['long-stanzas'];

const defaults = (mappings: MappingInfo[], pick: (m: MappingInfo) => string): Setting =>
  Object.fromEntries(mappings.map((m) => [m.name, pick(m)]));

/** A's and B's starting variants. */
export function startingSettings(mappings: MappingInfo[]): Record<Side, Setting> {
  return {
    A: defaults(mappings, (m) => m.default),
    B: defaults(mappings, (m) => (B_STARTS_AS_A.includes(m.name) ? m.default : m.variants[0] ?? m.default)),
  };
}

// Saved settings come in two shapes. Today's: `{ A: { name: variant }, B: … }`. Before the
// mappings became selectors: `{ A: [names left out], B: … }`, with the reading's own on/off in
// `prosody.read`. The old shape is read as "those off, the rest at their defaults", and a
// reading switched off then is off on both sides now. Any name or variant the page no longer
// knows falls back to that side's start, so a stale save can never send a 400.
export function restore(mappings: MappingInfo[], start: Setting, saved: unknown, readOff: boolean): Setting {
  const out: Setting = {};
  for (const { name, variants, default: fallback } of mappings) {
    let variant: unknown;
    if (Array.isArray(saved)) variant = saved.includes(name) ? variants[0] : fallback;
    else if (saved && typeof saved === 'object') variant = (saved as Record<string, unknown>)[name];
    if (Array.isArray(saved) && name === 'reading' && readOff) variant = 'off';
    out[name] = typeof variant === 'string' && variants.includes(variant) ? variant : start[name] ?? fallback;
  }
  return out;
}

/** The settings this browser saved, or the starting ones. */
export function loadSettings(): Record<Side, Setting> {
  const start = startingSettings(site.mappings);
  const saved = loadJson('prosody.settings') as { A?: unknown; B?: unknown } | null;
  const readOff = load('prosody.read') === 'off';
  if (saved && typeof saved.A === 'object' && typeof saved.B === 'object') {
    return {
      A: restore(site.mappings, start.A, saved.A, readOff),
      B: restore(site.mappings, start.B, saved.B, readOff),
    };
  }
  // The old reading box could be unticked without any mapping ever being saved, so the reading's
  // own off must carry over by itself too. B starts with it off already.
  if (readOff) start.A.reading = 'off';
  return start;
}

/** One side's variant of one mapping, chosen by the writer, and remembered. A choice on A is the
 * writer's own from then on, no longer a style's suggestion. */
export function choose(side: Side, name: string, variant: string): void {
  settings.value = { ...settings.value, [side]: { ...settings.value[side], [name]: variant } };
  save('prosody.settings', JSON.stringify(settings.value));
  if (side === 'A' && name in suggested.value) {
    const { [name]: _taken, ...rest } = suggested.value;
    suggested.value = rest;
  }
}

/** A style's suggestions, set on A — each remembered with what A had, so it can be put back. A
 * mapping a style set before keeps the variant A had before any style. */
export function suggest(style: string, suggests: Record<string, string>): string[] {
  const changed: string[] = [];
  const next = { ...suggested.value };
  const a = { ...settings.value.A };
  for (const [name, variant] of Object.entries(suggests)) {
    if (!(name in a) || a[name] === variant) continue;
    next[name] = { from: next[name]?.from ?? a[name]!, to: variant, style };
    a[name] = variant;
    changed.push(name);
  }
  if (!changed.length) return changed;
  settings.value = { ...settings.value, A: a };
  suggested.value = next;
  save('prosody.settings', JSON.stringify(settings.value));
  return changed;
}

/** A as it was before any style suggested anything. */
export function putBack(): void {
  const a = { ...settings.value.A };
  for (const [name, { from }] of Object.entries(suggested.value)) a[name] = from;
  settings.value = { ...settings.value, A: a };
  suggested.value = {};
  save('prosody.settings', JSON.stringify(settings.value));
}
