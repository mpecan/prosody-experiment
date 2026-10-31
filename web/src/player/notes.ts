// A Performance as the player reads it: the sampled sounds it needs, numbered, and every note and
// every cue of its reading as a flat record of numbers (crates/prosody-player/src/records.rs lays the
// records out; a test on each side pins them). The player decides nothing here — this only moves
// numbers.
import type { Performance, PerformedCue, PerformedNote, PerformedSound } from '../answers';

/** Numbers a note's record holds. */
export const STRIDE = 22;
/** Numbers a cue's record holds. */
export const CUE_STRIDE = 7;
/** The channels a note can play on, in the synth's order: a channel's index is its stem. */
export const CHANNELS = ['drums', 'bass', 'melody', 'chords'] as const;
/** Every stem, in the synth's order, by the mixer's channel each answers to: the four a note plays
 * on, and the reading's (prosody_synth `Channel::Speech`). */
export const STEM_CHANNELS = [...CHANNELS, 'voice'] as const;
/** The vowels, in the synth's order. */
export const VOWELS = ['a', 'e', 'i', 'o', 'u'] as const;
/** The oscillator waves, in the synth's order. */
export const WAVES = ['sine', 'sawtooth', 'square', 'triangle'] as const;

/** A sampled sound: a drum of a kit, or a soundfont file. */
export type Sampled = { kind: 'drum'; bank: string; piece: string } | { kind: 'font'; file: string };

/** The name a sampled sound is known by, or null for an oscillator, which needs nothing loaded. */
export function sampledKey(sound: PerformedSound): string | null {
  if (sound.kind === 'drum') return `drum:${sound.bank}/${sound.piece}`;
  if (sound.kind === 'font') return `font:${sound.file}`;
  return null;
}

/** Every sampled sound a Performance uses, by key, with the keys its soundfont notes play (rounded),
 * so only the zones those keys need are decoded. In order of first use. */
export function sampledOf(performance: Performance): Map<string, { sound: Sampled; keys: Set<number> }> {
  const sounds = new Map<string, { sound: Sampled; keys: Set<number> }>();
  for (const note of performance.notes) {
    const key = sampledKey(note.sound);
    if (key === null) continue;
    let entry = sounds.get(key);
    if (!entry) {
      const sound: Sampled = note.sound.kind === 'drum'
        ? { kind: 'drum', bank: note.sound.bank, piece: note.sound.piece }
        : { kind: 'font', file: (note.sound as { file: string }).file };
      entry = { sound, keys: new Set() };
      sounds.set(key, entry);
    }
    if (note.pitch !== null) entry.keys.add(Math.max(0, Math.min(127, Math.round(note.pitch))));
  }
  return sounds;
}

/** A note's record: see crates/prosody-synth-wasm/src/lib.rs. `ids` numbers the sampled sounds;
 * a note whose sound has no number is written with an id past every loaded one, so the synth counts
 * it as missing rather than playing something else. */
function record(note: PerformedNote, tag: number, ids: Map<string, number>, out: Float64Array, at: number): void {
  const key = sampledKey(note.sound);
  const kind = note.sound.kind === 'drum' ? 0 : note.sound.kind === 'font' ? 1 : 2;
  const id = key === null
    ? Math.max(0, WAVES.indexOf((note.sound as { wave: string }).wave as typeof WAVES[number]))
    : ids.get(key) ?? 4095;
  out.set([
    tag,
    Math.max(0, CHANNELS.indexOf(note.channel)),
    kind,
    id,
    note.onset,
    note.length,
    note.pitch ?? NaN,
    note.gain,
    note.postgain,
    note.attack ?? NaN,
    note.release ?? NaN,
    note.lpf,
    note.crush ?? NaN,
    note.vowel === null ? NaN : VOWELS.indexOf(note.vowel as typeof VOWELS[number]),
    note.shape ?? NaN,
    note.room,
    note.roomsize,
    note.delay?.level ?? NaN,
    note.delay?.time ?? 0,
    note.delay?.feedback ?? 0,
    note.compressor?.threshold ?? NaN,
    note.compressor?.ratio ?? 1,
  ], at);
}

/** Every note of `performance` as records, in its order; a note's tag is its index. */
export function records(performance: Performance, ids: Map<string, number>): Float64Array {
  const out = new Float64Array(performance.notes.length * STRIDE);
  performance.notes.forEach((note, index) => record(note, index, ids, out, index * STRIDE));
  return out;
}

/** Where the bars start, in seconds, for the synth's bar-by-bar hand-over. */
export function barStarts(performance: Performance): Float64Array {
  return Float64Array.from(performance.bars.map((bar) => bar.at));
}

/** What a line waits for at the natural pace, in the player's order (prosody_player `Onset`). An
 * onset it does not name waits for its bar, as `at` does: the reader before step 5 anchored every
 * line that did not only follow. */
const ONSETS = ['at', 'breath', 'follows'] as const;

/** Every cue of a reading as records: see crates/prosody-player/src/records.rs. */
export function cueRecords(cues: PerformedCue[]): Float64Array {
  const out = new Float64Array(cues.length * CUE_STRIDE);
  cues.forEach((cue, index) => {
    const natural = cue.natural;
    out.set([
      cue.line,
      cue.stanza,
      cue.pause,
      cue.fitted?.slot ?? NaN,
      cue.fitted?.at ?? NaN,
      natural ? Math.max(0, ONSETS.indexOf(natural.onset as typeof ONSETS[number])) : NaN,
      natural?.at ?? NaN,
    ], index * CUE_STRIDE);
  });
  return out;
}
