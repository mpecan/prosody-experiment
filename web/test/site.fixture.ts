// A small site, shaped as `prosody-web` writes `site.json`, with values worked by hand: enough
// scales, palettes, presets and mappings to exercise the page, few enough to read.
import { setSite } from '../src/data';
import type { Palette, SiteData } from '../src/site';

const defaultPalette: Palette = {
  bank: 'tr909', bass: 'gm_acoustic_bass', bass_gain: '0.32', bass_octave: '2', chord_gain: '0.40',
  chord_sound: 'gm_pad_warm', compress: '0.30', delay: '0.15', density_level: '0.00', drive: '0.00',
  drums_gain: '0.28', high_cut: '4400', humanise: '0.20', kit: 'machine', melody: 'gm_epiano1',
  melody_gain: '0.75', melody_octave: '4', reverb: '0.25', ride_fallback: 'open_hat', room_size: '2.0',
  root: 'C', scale: 'minor', swing: '0.00', low: '110', high: '130', voicing: 'note',
};
const SOUND = ['bass', 'melody', 'kit', 'ride_fallback', 'voicing', 'bass_octave', 'melody_octave', 'chord_sound',
  'reverb', 'room_size', 'delay', 'high_cut', 'drive', 'compress', 'humanise', 'drums_gain', 'bass_gain',
  'melody_gain', 'chord_gain', 'density_level'];

export const fixture: SiteData = {
  transport: 'wasm',
  scales: ['major', 'minor', 'dorian', 'phrygian_dominant'],
  scaleSteps: {
    major: [0, 2, 4, 5, 7, 9, 11],
    minor: [0, 2, 3, 5, 7, 8, 10],
    dorian: [0, 2, 3, 5, 7, 9, 10],
    phrygian_dominant: [0, 1, 4, 5, 7, 8, 10],
  },
  banks: ['tr909', 'tr808', 'linn'],
  instruments: ['triangle', 'gm_acoustic_bass', 'gm_epiano1', 'gm_pad_warm', 'gm_vibraphone'],
  kits: ['machine', 'jazz'],
  voicings: ['note', 'seventh'],
  rideFallbacks: ['open_hat', 'crash'],
  mappings: [
    { name: 'held', what: 'an open rhyme sounds until its answer', variants: ['off', 'on'], default: 'on' },
    { name: 'bass', what: 'a foundation bass', variants: ['off', 'height', 'syllables', 'feet'], default: 'height' },
    { name: 'reading', what: 'the poem read aloud', variants: ['off', 'arrives', 'stanza', 'breath'], default: 'breath' },
    { name: 'pulse', what: 'how long a bar lasts', variants: ['elastic', 'steady'], default: 'steady' },
    { name: 'long-stanzas', what: 'long stanzas erode', variants: ['gradual', 'erode'], default: 'erode' },
  ],
  presets: [
    { name: 'blake-tyger', label: 'Blake — The Tyger', language: 'en', poem: 'Tyger Tyger, burning bright,\n', palette: defaultPalette },
    {
      name: 'preseren-zdravljica', label: 'Prešeren — Zdravljica', language: 'sl', poem: 'Spet trte so rodile,\n',
      palette: { ...defaultPalette, scale: 'dorian', root: 'D' },
    },
  ],
  palettes: {
    default: defaultPalette,
    jazz: { ...defaultPalette, kit: 'jazz', voicing: 'seventh', melody: 'gm_epiano1', scale: 'dorian', swing: '0.33', bank: 'linn' },
  },
  paletteKeys: {
    default: SOUND,
    jazz: [...SOUND, 'scale', 'swing', 'bank'],
  },
  seed: 'Tyger Tyger, burning bright,\nIn the forests of the night;\n',
};

/** Installs the fixture as the page's data. */
export function useFixture(): SiteData {
  setSite(fixture);
  return fixture;
}
