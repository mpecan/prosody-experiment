// Folders along the bottom edge: a tab each, one open at a time, the open one again to close. Every
// folder stays on the page while closed, so what is set in it and how far it is scrolled are kept.
import type { ComponentChildren } from 'preact';

import { toggleFolder } from '../actions';
import { folder, type Folder } from '../store';
import { MappingsFolder } from './folders/MappingsFolder';
import { ReadingFolder } from './folders/ReadingFolder';
import { RollFolder } from './folders/RollFolder';
import { SoundFolder } from './folders/SoundFolder';
import { ViewsFolder } from './folders/ViewsFolder';

/** Each folder: its name, its element ids' stem, its tab's layer colour. */
export const TABS: [Folder, string, string, string][] = [
  ['roll', 'Roll', 'Roll', 'var(--pulse)'],
  ['sound', 'Sound', 'Sound', 'var(--ornament)'],
  ['mappings', 'Mappings', 'Mappings', 'var(--bass)'],
  ['reading', 'Reading', 'Reading', 'var(--rhyme)'],
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
  return (
    <footer id="folders">
      <div class="tabs" role="tablist" aria-label="Settings">
        {TABS.map(([id, stem, label, colour]) => (
          <button key={id} role="tab" id={'tab' + stem} data-folder={'folder' + stem} aria-controls={'folder' + stem}
            aria-selected={folder.value === id} style={{ '--t': colour }} onClick={() => toggleFolder(id)}>
            {label}
          </button>
        ))}
      </div>
      <div class="sheet" id="sheet" hidden={folder.value === null}>
        <Panel id="roll"><RollFolder /></Panel>
        <Panel id="sound"><SoundFolder /></Panel>
        <Panel id="mappings"><MappingsFolder /></Panel>
        <Panel id="reading"><ReadingFolder /></Panel>
        <Panel id="views"><ViewsFolder /></Panel>
      </div>
    </footer>
  );
}
