// The palette controls. Every control's id is its key in the style header, and every value is in
// header units, so the controls' values are a header the compiler reads back (`query`).
import { site } from './data';
import { controls, preset } from './store';
import type { Palette } from './site';

/** The sound and the mix: what a named palette sets. */
export const SOUND = ['bass', 'melody', 'kit', 'ride_fallback', 'voicing', 'bass_octave', 'melody_octave',
  'chord_sound', 'reverb', 'room_size', 'delay', 'high_cut', 'drive', 'compress', 'humanise', 'drums_gain',
  'bass_gain', 'melody_gain', 'chord_gain', 'density_level'];
/** Every control. */
export const CONTROLS = ['scale', 'root', 'bank', 'low', 'high', 'swing', ...SOUND];
export const ROOTS = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

/** The choices a menu control offers. */
export function optionsOf(id: string): string[] {
  switch (id) {
    case 'scale': return site.scales;
    case 'root': return ROOTS;
    case 'bank': return site.banks;
    case 'bass':
    case 'melody':
    case 'chord_sound': return site.instruments;
    case 'kit': return site.kits;
    case 'voicing': return site.voicings;
    case 'ride_fallback': return site.rideFallbacks;
    default: return [];
  }
}

const same = (a: string | undefined, b: string | undefined) =>
  (isNaN(Number(a)) ? a === b : Number(a) === Number(b));

/** The named palette the sound controls currently match, or "custom". */
export function paletteOf(values: Palette): string {
  const match = Object.entries(site.palettes)
    .find(([name, palette]) => (site.paletteKeys[name] ?? []).every((id) => same(values[id], palette[id])));
  return match ? match[0] : 'custom';
}

/** Sets the controls named in `ids` to their values in `values`. */
export function setControls(values: Palette, ids: string[]): void {
  const next = { ...controls.value };
  for (const id of ids) if (id in values) next[id] = values[id];
  controls.value = next;
}

/** One control changed by the writer. */
export function setControl(id: string, value: string): void {
  controls.value = { ...controls.value, [id]: value };
}

/** A named palette chosen from the menu: its own keys only — the sound and mix, and jazz's scale,
 * swing and bank. Root and tempo stay. */
export function choosePalette(name: string): boolean {
  const values = site.palettes[name];
  if (!values) return false;
  setControls(values, site.paletteKeys[name] ?? []);
  return true;
}

/** An example poem chosen from the menu: the poem and the palette its header set. */
export function presetOf(name: string) {
  return site.presets.find((p) => p.name === name);
}

export function startControls(): void {
  setControls(site.palettes.default ?? {}, CONTROLS);
  preset.value = '';
}
