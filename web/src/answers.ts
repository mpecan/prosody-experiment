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
