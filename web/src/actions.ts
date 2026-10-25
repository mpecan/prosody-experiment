// What the writer does on the page, beyond a single engine: choosing a poem or a palette, turning a
// control, opening a folder, the theme and the notices.
import { CONTROLS, choosePalette, presetOf, setControl, setControls } from './controls';
import { compile, piece, schedule } from './piece';
import { readIn } from './reading';
import { fitRoll, buildLit, watch } from './watch';
import { load, save } from './storage';
import { folder, noticeShown, poem, preset, rollBars, theme, welcomeOpen, type Folder, type Theme } from './store';

/** An example poem, with its palette, read in its own language. */
export function choosePreset(name: string): void {
  const chosen = presetOf(name);
  preset.value = name;
  if (!chosen) return;
  poem.value = chosen.poem;
  setControls(chosen.palette, CONTROLS);
  if (watch.poemBox) watch.poemBox.scrollTop = 0;
  piece.fromTheTop = true;   // a new poem starts at its first bar, so the reading starts at its first line
  // A preset is read in its own language, with a voice that speaks it (Prešeren in Slovenian).
  readIn(chosen.language);
  buildLit();
  void compile();
}

/** A named palette from the Sound menu. */
export function pickPalette(name: string): void {
  if (choosePalette(name)) schedule();
}

/** One palette control turned. */
export function turnControl(id: string, value: string): void {
  setControl(id, value);
  schedule();
}

/** The poem edited. */
export function write(text: string): void {
  poem.value = text;
  buildLit();
  schedule();
}

// ---- Light and dark ----
// As the system says (auto), or always light, or always dark: the reader's choice, remembered.
export const THEMES: Theme[] = ['auto', 'light', 'dark'];
export function setTheme(next: Theme): void {
  theme.value = next;
  if (next === 'auto') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = next;
  save('prosody.theme', next);
}
export function nextTheme(): void {
  setTheme(THEMES[(THEMES.indexOf(theme.value) + 1) % THEMES.length] ?? 'auto');
}

// ---- Folders ----
//
// One open at a time, or none: a tab opens its folder, another tab swaps it, the open one's tab
// closes it. Which is open is remembered per browser.
export const FOLDERS: Folder[] = ['roll', 'sound', 'mappings', 'reading', 'views'];
export function openFolder(id: Folder | null): void {
  folder.value = id;
  save('prosody.folder', id || '');
}
export function toggleFolder(id: Folder): void {
  openFolder(folder.value === id ? null : id);
}

/** How many bars the roll shows, remembered. */
export function setRollBars(bars: string): void {
  rollBars.value = bars;
  fitRoll();
  save('prosody.rollBars', bars);
}

// ---- The welcome and the notice ----
export function closeWelcome(): void {
  welcomeOpen.value = false;
  save('prosody.welcomed', '1');
  watch.poemBox?.focus({ preventScroll: true });
}
export function dismissNotice(): void {
  noticeShown.value = false;
  save('prosody.notice', 'dismissed');
}

/** What this browser remembered of the page's own settings. Storage that cannot be read starts
 * from the defaults, the welcome and the notice shown. */
export function restorePage(): void {
  const saved = load('prosody.theme');
  setTheme(THEMES.includes(saved as Theme) ? saved as Theme : 'auto');
  const open = load('prosody.folder');
  // Folders were saved by their element ids before the page was components.
  const legacy: Record<string, Folder> = { folderRoll: 'roll', folderSound: 'sound', folderMappings: 'mappings', folderReading: 'reading', folderViews: 'views' };
  const id = (open && (legacy[open] ?? open)) as Folder | null;
  if (id && FOLDERS.includes(id)) folder.value = id;
  const bars = load('prosody.rollBars');
  if (bars && ['2', '4', '8', '16'].includes(bars)) rollBars.value = bars;
  noticeShown.value = load('prosody.notice') !== 'dismissed';
  welcomeOpen.value = !load('prosody.welcomed');
}
