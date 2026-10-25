// What the writer does on the page, beyond a single engine: choosing a poem or a palette, turning a
// control, opening a folder, the theme and the notices.
import { CONTROLS, choosePalette, presetOf, setControl, setControls } from './controls';
import { site } from './data';
import { suggest } from './settings';
import { compile, piece, schedule } from './piece';
import { applyVoiceLevel, readIn } from './reading';
import { current, linkTo, settingsOf, type Shared } from './share';
import { fitRoll, buildLit, watch } from './watch';
import { load, save } from './storage';
import {
  folder, hearing, muted, noticeShown, poem, preset, rollBars, rollShown, settings, shareLink, sharedOpened, sheetOpen, soloed,
  theme, welcomeOpen,
  type Channel, type Folder, type Theme,
} from './store';

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

/** A style from the Sound folder: its palette, and on A the mappings it suggests. */
export function pickStyle(name: string): void {
  if (!choosePalette(name)) return;
  const style = site.styles.find((s) => s.name === name);
  if (style) suggest(name, style.suggests);
  schedule();
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
// One folder at a time. On a wide screen the folders are a side panel beside the poem, always
// showing one; on a phone they are a sheet along the bottom edge, which a tab opens and the open
// folder's tab, or the handle, folds away. Which is open, and whether, is remembered per browser.
export const FOLDERS: Folder[] = ['mixer', 'sound', 'reading', 'mappings', 'views'];
export function openFolder(id: Folder): void {
  // Another folder starts at its top, not where the last one was scrolled to.
  if (folder.value !== id) document.getElementById('sheet')?.scrollTo(0, 0);
  folder.value = id;
  sheetOpen.value = true;
  save('prosody.folder', id);
}
export function toggleFolder(id: Folder): void {
  if (folder.value === id && sheetOpen.value) foldSheet();
  else openFolder(id);
}
export function foldSheet(): void {
  sheetOpen.value = false;
  save('prosody.folder', '');
}

// ---- Mute and solo ----
/** Mutes or unmutes a channel; heard at once (mix.ts). */
export function toggleMute(channel: Channel): void {
  muted.value = { ...muted.value, [channel]: !muted.value[channel] };
  applyVoiceLevel();
}
/** Solos or unsolos a channel: with any soloed, only the soloed are heard. */
export function toggleSolo(channel: Channel): void {
  soloed.value = { ...soloed.value, [channel]: !soloed.value[channel] };
  applyVoiceLevel();
}

// ---- Sharing ----
/** A shared creation, opened in place of what the page had — without saving over this browser's own
 * settings, which come back on a plain visit. */
export function openShared(shared: Shared): void {
  const { sound, sides } = settingsOf(shared);
  poem.value = shared.poem;
  setControls(sound, CONTROLS);
  settings.value = sides;
  hearing.value = shared.hear ?? 'A';
  if (shared.lang) readIn(shared.lang);
  sharedOpened.value = true;
}
/** Opens the share dialog with a link to the page as it is now. */
export async function openShare(): Promise<void> {
  shareLink.value = { link: await linkTo(current()), copied: false };
}
/** Copies the link, and says so. */
export async function copyShare(): Promise<void> {
  const open = shareLink.value;
  if (!open) return;
  try {
    await navigator.clipboard.writeText(open.link);
    shareLink.value = { ...open, copied: true };
  } catch {
    // No clipboard here: the link is selected in its box, to copy by hand.
  }
}

// ---- The roll ----
/** Shows or hides the roll, remembered. */
export function showRoll(shown: boolean): void {
  rollShown.value = shown;
  save('prosody.roll', shown ? 'shown' : 'hidden');
  if (shown) fitRoll();
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
  // Folders were saved by their element ids before the page was components, and the roll was one.
  const open = load('prosody.folder');
  const legacy: Record<string, Folder> = {
    folderSound: 'sound', folderMappings: 'mappings', folderReading: 'reading', folderViews: 'views',
    folderRoll: 'mixer', roll: 'mixer',
  };
  const id = (open && (legacy[open] ?? open)) as Folder | null;
  if (id && FOLDERS.includes(id)) {
    folder.value = id;
    sheetOpen.value = open !== 'roll' && open !== 'folderRoll';
  }
  rollShown.value = load('prosody.roll') !== 'hidden';
  const bars = load('prosody.rollBars');
  if (bars && ['2', '4', '8', '16'].includes(bars)) rollBars.value = bars;
  noticeShown.value = load('prosody.notice') !== 'dismissed';
  welcomeOpen.value = !load('prosody.welcomed');
}
