// The folders: a side panel beside the poem on a wide screen; on a phone, a sheet along the bottom
// edge that a tab opens and the open folder's tab, or the handle, folds away. Every folder stays on
// the page while another is shown, so what is set in it and how far it is scrolled are kept.
import type { ComponentChildren } from 'preact';

import { foldSheet, openFolder, toggleFolder } from '../actions';
import { folder, sheetOpen, type Folder } from '../store';
import { MappingsFolder } from './folders/MappingsFolder';
import { MixerFolder } from './folders/MixerFolder';
import { ReadingFolder } from './folders/ReadingFolder';
import { SoundFolder } from './folders/SoundFolder';
import { ViewsFolder } from './folders/ViewsFolder';

/** Each folder: its name, its element ids' stem, its label, its tab's layer colour. */
export const TABS: [Folder, string, string, string][] = [
  ['mixer', 'Mixer', 'Mixer', 'var(--pulse)'],
  ['sound', 'Sound', 'Sound', 'var(--ornament)'],
  ['reading', 'Reading', 'Reading', 'var(--rhyme)'],
  ['mappings', 'Mappings', 'Mappings', 'var(--bass)'],
  ['views', 'Views', 'Under the hood', 'var(--chords)'],
];

function Panel({ id, children }: { id: Folder; children: ComponentChildren }) {
  const stem = TABS.find(([name]) => name === id)?.[1] ?? id;
  return (
    <section class="folder" id={'folder' + stem} role="tabpanel" aria-labelledby={'tab' + stem} hidden={folder.value !== id}>
      {children}
    </section>
  );
}

export function Folders() {
  const open = sheetOpen.value;
  return (
    <aside id="folders" class={open ? 'folders open' : 'folders'} aria-label="Settings">
      <button type="button" class="handle" aria-expanded={open} aria-controls="sheet"
        aria-label={open ? 'Fold the settings away' : 'Open the settings'}
        onClick={() => (open ? foldSheet() : openFolder(folder.value))}>
        <span />
      </button>
      <div class="tabs" role="tablist" aria-label="Settings">
        {TABS.map(([id, stem, label, colour]) => (
          <button key={id} role="tab" id={'tab' + stem} aria-controls={'folder' + stem}
            aria-selected={folder.value === id} style={{ '--t': colour }} onClick={() => toggleFolder(id)}>
            {label}
          </button>
        ))}
      </div>
      <div class="sheet" id="sheet">
        <Panel id="mixer"><MixerFolder /></Panel>
        <Panel id="sound"><SoundFolder /></Panel>
        <Panel id="reading"><ReadingFolder /></Panel>
        <Panel id="mappings"><MappingsFolder /></Panel>
        <Panel id="views"><ViewsFolder /></Panel>
      </div>
    </aside>
  );
}
