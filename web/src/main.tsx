// The page's start: its data first, then what this browser remembered, then the page itself, and
// the poem compiled.
import { render } from 'preact';

import { openShared, restorePage } from './actions';
import { App } from './components/App';
import { startControls } from './controls';
import { setSite, site } from './data';
import { compile } from './piece';
import { startReading } from './reading';
import { loadSettings } from './settings';
import { loadSite } from './site';
import { decode } from './share';
import { poem, settings, status } from './store';
import './styles.css';

async function start(): Promise<void> {
  const root = document.getElementById('app');
  if (!root) return;
  try {
    setSite(await loadSite());
  } catch (error) {
    root.textContent = `The page could not load its data (${(error as Error).message}). Is prosody-web running?`;
    return;
  }
  startControls();
  settings.value = loadSettings();
  poem.value = site.seed;
  restorePage();
  startReading();
  // A shared link opens its creation in place of the opening poem and the settings this browser
  // saved — without saving over them: they are this visitor's, and come back on a plain visit.
  const code = new URLSearchParams(location.search).get('s');
  const shared = code ? await decode(code) : null;
  if (shared) openShared(shared);
  render(<App />, root);
  status(code && !shared ? 'this link does not hold a poem this page can read' : 'ready', !!code && !shared);
  void compile();
}

void start();
