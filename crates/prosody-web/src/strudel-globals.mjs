// The modules `@strudel/soundfonts` imports, answered from the one Strudel already on the page.
//
// `@strudel/web` is a single bundle that puts every export of `@strudel/core` and
// `@strudel/webaudio` on `globalThis` once `initStrudel()` has run. `@strudel/soundfonts` is a
// separate package that imports those two by name. Loading them again from a CDN would give the
// soundfonts their own copy of Strudel, and they would register their instruments in a sound map
// the playing Strudel never reads — silence. The page's import map points both names here
// instead, so the soundfonts register into the Strudel that plays. Import this only after
// `initStrudel()` has resolved: the bindings below are read once, when the module evaluates.
//
// `sfumato` is the `.sf2` loader; this page uses only the General MIDI fonts, so its two entry
// points refuse rather than load a second synthesis engine.

const strudel = globalThis;

export const {
  freqToMidi,
  noteToMidi,
  getSoundIndex,
  Pattern,
  getPlayableNoteValue,
  getAudioContext,
  getADSRValues,
  getParamADSR,
  getVibratoOscillator,
  getPitchEnvelope,
  onceEnded,
  releaseAudioNode,
} = strudel;

export const registerSound = (...args) => strudel.registerSound(...args);

export function startPresetNote() {
  throw new Error('.sf2 soundfonts are not loaded on this page; use a gm_ instrument');
}

export function loadSoundfont() {
  throw new Error('.sf2 soundfonts are not loaded on this page; use a gm_ instrument');
}
