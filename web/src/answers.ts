// The shapes of the compiler's JSON answers (crates/prosody-web/src/lib.rs `reading_json`,
// `trace_json`).

/** A word of a spoken line: where it is in the text box, and how many syllables it sounds. */
export interface Word {
  from: number;
  to: number;
  syllables: number;
}

/** A line as the reading speaks it. `from`/`to` are UTF-16 positions in the text box. */
export interface SpokenLine {
  line: number;
  stanza: number;
  start: number;
  end: number;
  from: number;
  to: number;
  syllables: number;
  /** The line's natural pace: how much faster or slower than the voice's own speed. */
  pace: number;
  /** The pause after it, in seconds. */
  pause: number;
  ends: string;
  words: Word[];
  text: string;
}

/** A line at the fitted pace: read in its bar `slot`, at `rate`. */
export interface Cue extends SpokenLine {
  slot: number;
  rate: number;
}

/** A line at the natural pace: when it may begin (`arrives`, `breath`, `follows`), and the earliest
 * bar it may begin in. */
export interface NaturalLine extends SpokenLine {
  onset: 'arrives' | 'breath' | 'follows' | string;
  slot: number;
}

export interface Unread {
  line: number;
  start: number;
  end: number;
  why: string;
  text: string;
}

/** The reading of one setting (`prosody_emit::reading`): the cue patterns and the lines they speak. */
export interface Reading {
  /** The fitted pace's cue pattern: a line a bar. */
  pattern: string;
  /** The natural pace's cue pattern: the earliest bar each line may begin in. */
  natural: string;
  /** Where the bars fall, in cycles, and the piece's length last. */
  bars: number[];
  cpm: number;
  /** How much of its bar a fitted line may fill. */
  fill: number;
  cues: Cue[];
  lines: NaturalLine[];
  unread: Unread[];
  natural_unread: Unread[];
}

/** A run of the poem that lights while something sounds. */
export interface Mark {
  start: number;
  end: number;
  colour: string;
  /** Heard anew, rather than a line looping. */
  fresh: boolean;
  melody: boolean;
  bass: boolean;
}

/** Which syllable sounds when (`prosody_emit::trace`): a silent pattern of mark ids on the music's
 * clock. */
export interface Trace {
  pattern: string;
  /** Where the bars fall, in cycles, and the piece's length last. */
  bars: number[];
  cpm: number;
  marks: Mark[];
}

/** What makes a performed note's sound (`prosody_emit::perform::Sound`). */
export type PerformedSound =
  | { kind: 'drum'; bank: string; piece: string }
  | { kind: 'font'; name: string; variant: number; file: string }
  | { kind: 'wave'; wave: string };

/** The stems mute and solo act on. */
export type PerformedChannel = 'drums' | 'bass' | 'melody' | 'chords';

/** One note of the Performance, resolved: when, what, how loud, from which words. */
export interface PerformedNote {
  from: number;
  to: number;
  channel: PerformedChannel;
  /** The roll's lane: a layer's name (`site.layers`). */
  lane: string;
  slot: number;
  onset: number;
  length: number;
  sound: PerformedSound;
  pitch: number | null;
  /** `gain` × `velocity`. */
  gain: number;
  attack: number | null;
  release: number | null;
  lpf: number;
  /** Bit depth, or null for none. */
  crush: number | null;
  /** The formant vowel, or null for none. */
  vowel: string | null;
  /** The waveshaper's amount, or null for none. */
  shape: number | null;
  /** The reverb send, and the reverb's RT60 in seconds. */
  room: number;
  roomsize: number;
  /** The echo, or null for none. */
  delay: { level: number; time: number; feedback: number } | null;
  postgain: number;
  /** The layer's compressor, or null for none. */
  compressor: { threshold: number; ratio: number } | null;
}

/** A syllable lighting up while it sounds. */
export interface PerformedMark {
  from: number;
  to: number;
  lane: string;
  slot: number;
  onset: number;
  length: number;
  fresh: boolean;
  melody: boolean;
  bass: boolean;
}

/** A line of the reading on the piece's clock (`prosody_emit::perform::Cue`): where it is read at
 * each pace, `null` where that pace does not read it. */
export interface PerformedCue {
  line: number;
  stanza: number;
  from: number;
  to: number;
  /** The speed the natural pace asks a voice for, and the pause after the line there, in seconds. */
  pace: number;
  pause: number;
  /** The bar it is read in at the fitted pace, when that bar begins, and the rate it is fitted at. */
  fitted: { slot: number; at: number; rate: number } | null;
  /** What it waits for at the natural pace, and the bar it may begin in (`null` for a line that only
   * follows the one before). */
  natural: { onset: 'arrives' | 'breath' | 'follows' | string; slot: number | null; at: number | null } | null;
}

/** A line a pace leaves unread, and why. */
export interface PerformedUnread {
  line: number;
  from: number;
  to: number;
  why: string;
}

/** Every note of the piece, resolved (`docs/player.md` §3; `performance_json` in
 * crates/prosody-web/src/lib.rs). Times are seconds; the piece loops round at `seconds`. */
export interface Performance {
  seconds: number;
  cpm: number;
  bars: { slot: number; at: number }[];
  notes: PerformedNote[];
  marks: PerformedMark[];
  cues: PerformedCue[];
  unread: PerformedUnread[];
  natural_unread: PerformedUnread[];
}
