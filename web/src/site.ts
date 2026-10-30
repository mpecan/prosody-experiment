// The page's data, `site.json`: everything the page needs before it can ask anything. Written by
// `prosody-web` from the compiler's own tables (`site_json` in crates/prosody-web/src/lib.rs), so
// the controls cannot drift from what the style accepts — answered by the local server, or written
// beside the static page by `scripts/build-site.sh`.

/** Every control's value, by its id, which is its key in the style header — in header units. */
export type Palette = Record<string, string>;

/** A candidate mapping: a selector with variants, one of them its default. */
export interface MappingInfo {
  name: string;
  /** What it does, in a sentence. */
  what: string;
  variants: string[];
  default: string;
}

/** An example poem, with the palette its header set and the language it is read in. */
export interface Preset {
  name: string;
  label: string;
  /** A BCP 47 tag: Prešeren is `sl`, the rest `en`. */
  language: string;
  /** The poem, without its header: the palette goes to the controls. */
  poem: string;
  palette: Palette;
}

/** A named palette as the style picker shows it, with the mapping variants it suggests for A. */
export interface StyleInfo {
  name: string;
  /** What it sounds like, in a few words. */
  what: string;
  /** Mapping → variant: what its genre would have the poem do. Only ever a suggestion, set on A. */
  suggests: Record<string, string>;
}

/** A layer of the pattern, and the colour its events carry. */
export interface Layer {
  name: string;
  colour: string;
}

export interface SiteData {
  /** Where the page's questions go: the local server, or the compiler beside the page. */
  transport: 'server' | 'wasm';
  scales: string[];
  /** Each scale's semitones above its root, for the note circle. */
  scaleSteps: Record<string, number[]>;
  banks: string[];
  instruments: string[];
  kits: string[];
  voicings: string[];
  rideFallbacks: string[];
  /** In the order the core lists them. */
  mappings: MappingInfo[];
  presets: Preset[];
  /** The named palettes, every control's value, from `Style::preset`. */
  palettes: Record<string, Palette>;
  /** The controls each named palette sets (`style::preset_keys`). */
  paletteKeys: Record<string, string[]>;
  /** The pattern's layers, in lane order. */
  layers: Layer[];
  /** Every named palette, in order, for the style picker. */
  styles: StyleInfo[];
  /** The poem the page opens with. */
  seed: string;
}

/** The page's data, fetched from beside the page. */
export async function loadSite(): Promise<SiteData> {
  const response = await fetch(new URL('./site.json', document.baseURI));
  if (!response.ok) throw new Error(`${response.status} for site.json`);
  return (await response.json()) as SiteData;
}
