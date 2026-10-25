// What the controls' values are called on the page. The values themselves — `gm_acoustic_bass`,
// `tr909`, `phrygian_dominant` — are the style header's, and are what the page sends; these are
// only what a reader sees.

const NAMES: Record<string, string> = {
  // Strudel's own synths.
  karplus: 'Plucked string', fm: 'FM synth', subtractive: 'Subtractive synth', physical: 'Physical model',
  sine: 'Sine wave', sawtooth: 'Sawtooth wave', square: 'Square wave', triangle: 'Triangle wave',
  // General MIDI soundfonts.
  gm_acoustic_bass: 'Acoustic bass', gm_electric_bass_finger: 'Finger bass', gm_electric_bass_pick: 'Picked bass',
  gm_fretless_bass: 'Fretless bass', gm_synth_bass_1: 'Synth bass 1', gm_synth_bass_2: 'Synth bass 2',
  gm_epiano1: 'Electric piano', gm_epiano2: 'Electric piano 2', gm_piano: 'Piano', gm_vibraphone: 'Vibraphone',
  gm_marimba: 'Marimba', gm_celesta: 'Celesta', gm_music_box: 'Music box', gm_kalimba: 'Kalimba',
  gm_glockenspiel: 'Glockenspiel', gm_acoustic_guitar_nylon: 'Nylon guitar', gm_orchestral_harp: 'Harp',
  gm_pad_warm: 'Warm pad', gm_string_ensemble_1: 'String ensemble', gm_string_ensemble_2: 'Strings',
  gm_cello: 'Cello', gm_contrabass: 'Double bass', gm_harpsichord: 'Harpsichord', gm_church_organ: 'Church organ',
  gm_lead_2_sawtooth: 'Sawtooth lead', gm_flute: 'Flute', gm_synth_strings_1: 'Synth strings',
  gm_choir_aahs: 'Choir', gm_pad_halo: 'Halo pad',
  // Palettes whose names are not words.
  lofi: 'lo-fi',
  // Drum machines.
  tr909: 'Roland TR-909', tr808: 'Roland TR-808', tr707: 'Roland TR-707', linn: 'LinnDrum', casio: 'Casio',
  r8: 'Roland R-8',
  // Kits, ride stand-ins and voicings.
  machine: 'Kick and hat', ride: 'Ride and snare', open_hat: 'Open hat', crash: 'Crash',
  note: 'One note', seventh: 'Seventh chord',
};

/** What a value is called on the page: its name if it has one, else itself in plain words —
 * `harmonic_minor` is "harmonic minor". */
export function nameOf(value: string): string {
  return NAMES[value] ?? value.replace(/^gm_/, '').replace(/_/g, ' ');
}
