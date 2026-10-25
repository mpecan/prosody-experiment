// The page's start: its data first, then what this browser remembered, then the page itself, and
// the poem compiled.
import { render } from 'preact';

import { restorePage } from './actions';
import { App } from './components/App';
import { startControls } from './controls';
import { setSite, site } from './data';
import { compile } from './piece';
import { startReading } from './reading';
import { loadSettings } from './settings';
import { loadSite } from './site';
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
  render(<App />, root);
  status('ready');
  void compile();
}

void start();
