// What the page shows, as signals: the components render from these, and the engine modules (the
// compiler's answers, Strudel, the reading) write them. State the screen never shows lives in those
// modules as plain variables, as it did when the page was one script.
import { computed, signal } from '@preact/signals';

import type { Palette } from './site';

export type Side = 'A' | 'B';
/** A variant for every mapping, by name. */
export type Setting = Record<string, string>;

/** The text box: the poem being written. */
export const poem = signal('');
/** The example poem last chosen from the menu, or '' for none. */
export const preset = signal('');
/** Every palette control's value, by id — its key in the style header, in header units. */
export const controls = signal<Palette>({});
/** The two settings being compared. */
export const settings = signal<Record<Side, Setting>>({ A: {}, B: {} });
/** What a style set on A, mapping by mapping: the variant A had, the one the style set, and the
 * style — so Mappings can say so and put A back. */
export const suggested = signal<Record<string, { from: string; to: string; style: string }>>({});
/** Which setting is playing and shown. */
export const hearing = signal<Side>('A');
export const playing = signal(false);

// The status bar: what the music is doing, and after it what the reading is doing.
export const statusText = signal('ready');
export const statusBad = signal(false);
export const readingNote = signal('');
export const readingNoteBad = signal(false);
export const statusLine = computed(() => ({
  text: statusText.value + (readingNote.value ? ' · ' + readingNote.value : ''),
  bad: statusBad.value || readingNoteBad.value,
}));

/** Says what the music is doing. */
export function status(text: string, bad = false): void {
  statusText.value = text;
  statusBad.value = bad;
}

// Under the hood: the pattern heard (with its reading's cue patterns), each word's annotation, and
// both settings measured.
export const patternView = signal('');
export const explainView = signal('');
export interface Metric {
  name: string;
  low: number;
  high: number;
  value: number;
  fit: number;
}
export interface Score {
  metrics: Metric[];
  interest: number | null;
  legibility: number | null;
  overall: number | null;
}
/** Both settings measured; `measuring` while a measurement is out, null when there is nothing. */
export const scores = signal<{ A: Score | null; B: Score | null } | 'measuring'>({ A: null, B: null });
export const profile = signal('machine');
export type View = 'pattern' | 'explain' | 'score';
export const view = signal<View>('pattern');

// The folders, the roll, the theme and the notices.
export type Folder = 'mixer' | 'sound' | 'reading' | 'mappings' | 'views';
/** The folder shown: in the side panel on a wide screen, in the sheet on a phone. */
export const folder = signal<Folder>('mixer');
/** On a phone, whether the sheet is open or folded down to its tabs. */
export const sheetOpen = signal(false);
/** Whether the roll is shown, whichever folder is open. */
export const rollShown = signal(true);
export type Theme = 'auto' | 'light' | 'dark';
export const theme = signal<Theme>('auto');
export const noticeShown = signal(false);
export const welcomeOpen = signal(false);
/** How many bars the roll shows. */
export const rollBars = signal('4');
/** The beat the Play button breathes on, in seconds, while it plays under the steady pulse. */
export const beat = signal<number | null>(null);

// The reading's settings and its menus (reading/index.ts `showVoices`).
export type EngineName = 'browser' | 'kokoro' | 'piper' | 'kitten' | 'recorded';
export type Pace = 'natural' | 'fitted';
export interface ReadingSettings {
  engine: EngineName;
  pace: Pace;
  level: number;
  /** The voice chosen per engine and language. */
  voices: Partial<Record<EngineName, Record<string, string>>>;
}
export const readingSettings = signal<ReadingSettings>({ engine: 'recorded', pace: 'natural', level: 1, voices: {} });
/** The language the poem is read in. */
export const language = signal('en');
export interface Option {
  value: string;
  label: string;
}
export interface ReadingMenus {
  engines: Option[];
  languages: Option[];
  voices: Option[];
  voice: string;
  voiceDisabled: boolean;
}
export const readingMenus = signal<ReadingMenus>({ engines: [], languages: [], voices: [], voice: '', voiceDisabled: false });
/** Whether the reading ducks the music while a stanza is read. */
export const duck = signal(true);

// The dialogs that ask and wait.
export interface DownloadQuestion {
  title: string;
  text: string;
  yes: string;
  answer: (yes: boolean) => void;
}
export const downloadQuestion = signal<DownloadQuestion | null>(null);
export interface LoaderState {
  title: string;
  text: string;
  progress: string;
  /** 0–1, or null for a sweep when the size is not known. */
  fraction: number | null;
  failed: boolean;
  /** A voice's download can be cancelled; the instruments' cannot. */
  cancellable: boolean;
  hide: string;
}
export const loader = signal<LoaderState | null>(null);
