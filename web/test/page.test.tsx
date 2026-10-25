// The page as a reader meets it: rendered whole, with a compiler that answers by rote.
import { fireEvent, render, screen, waitFor, within } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { restorePage } from '../src/actions';
import { App } from '../src/components/App';
import { startControls } from '../src/controls';
import { compile } from '../src/piece';
import { engines, startReading } from '../src/reading';
import { loadSettings } from '../src/settings';
import { folder, language, poem, preset, settings } from '../src/store';
import { useFixture } from './site.fixture';

// What the compiler answers, for any poem: a pattern of two bars, a reading of one line, a trace
// that lights "Tyger" (bytes 0..5), and a score of one metric.
const SUMMARY = '2 bars from 2 lines, 3 voices, 10 events';
vi.mock('../src/compiler', () => ({
  ask: vi.fn(async (path: string, query: string) => {
    const side = new URLSearchParams(query).get('set')?.includes('pulse:elastic') ? 'B' : 'A';
    const json = (value: unknown) => new Response(JSON.stringify(value));
    switch (path) {
      case '/pattern': return new Response(`// compiled from a poem by prosody\n// ${SUMMARY} (${side})\nstack()`);
      case '/explain': return new Response('Tyger  pulse');
      case '/reading': return json({
        pattern: 'silence', natural: 'silence', bars: [0, 1, 2], cpm: 30, fill: 0.9, unread: [], natural_unread: [],
        cues: [], lines: [{ line: 0, stanza: 0, start: 0, end: 28, from: 0, to: 28, syllables: 7, pace: 1, pause: 0.5, ends: 'comma', words: [], text: 'Tyger Tyger, burning bright,', onset: 'breath', slot: 0 }],
      });
      case '/trace': return json({ pattern: 'silence', bars: [0, 1, 2], cpm: 30, marks: [{ start: 0, end: 5, colour: '#c9a6ff', fresh: true, melody: false, bass: false }] });
      case '/score': return json({ metrics: [{ name: 'density', low: 0.2, high: 0.6, value: side === 'A' ? 0.4 : 0.7, fit: side === 'A' ? 1 : 0.5 }], interest: 0.8, legibility: 0.9, overall: 0.72 });
      default: return new Response('no such thing here', { status: 404 });
    }
  }),
}));

/** The page started as main.tsx starts it, rendered. */
function start() {
  const site = useFixture();
  startControls();
  settings.value = loadSettings();
  poem.value = site.seed;
  language.value = 'en';
  folder.value = null;
  preset.value = '';
  restorePage();
  startReading();
  return render(<App />);
}

beforeEach(() => {
  // No recordings and no models here: the recorded engine's manifest is a 404.
  vi.stubGlobal('fetch', vi.fn(async () => new Response('', { status: 404 })));
  for (const engine of Object.values(engines)) if (engine.name !== 'browser') engine.state = 'idle';
  engines.recorded.manifest = null;
});

describe('a first visit', () => {
  it('opens on the definition of prosody, with the source and a contact, until it is closed', async () => {
    start();
    const welcome = document.getElementById('welcome') as HTMLDialogElement;
    expect(welcome.open).toBe(true);
    expect(welcome.textContent).toContain('the patterns of rhythm and sound in poetry');
    expect(within(welcome).getByText('github.com/mpecan/prosody-experiment').getAttribute('href'))
      .toBe('https://github.com/mpecan/prosody-experiment');
    expect(within(welcome).getByText('me@prosody.rocks').getAttribute('href')).toBe('mailto:me@prosody.rocks');
    fireEvent.click(within(welcome).getByText('Start listening'));
    await waitFor(() => expect(welcome.open).toBe(false));
    expect(localStorage.getItem('prosody.welcomed')).toBe('1');
  });

  it('says at the top that it is an experiment, and stays closed once seen', () => {
    localStorage.setItem('prosody.welcomed', '1');
    start();
    expect(screen.getByText('an experiment')).toBeTruthy();
    expect((document.getElementById('welcome') as HTMLDialogElement).open).toBe(false);
  });
});

describe('the folders', () => {
  it('open one at a time, swap, and close on their own tab again, remembered', async () => {
    start();
    const sheet = document.getElementById('sheet')!;
    expect(sheet.hidden).toBe(true);
    fireEvent.click(screen.getByRole('tab', { name: 'Sound' }));
    await waitFor(() => expect(sheet.hidden).toBe(false));
    expect(document.getElementById('folderSound')!.hidden).toBe(false);
    expect(document.getElementById('folderReading')!.hidden).toBe(true);
    expect(screen.getByRole('tab', { name: 'Sound' }).getAttribute('aria-selected')).toBe('true');
    fireEvent.click(screen.getByRole('tab', { name: 'Reading' }));
    await waitFor(() => expect(document.getElementById('folderReading')!.hidden).toBe(false));
    expect(document.getElementById('folderSound')!.hidden).toBe(true);
    expect(localStorage.getItem('prosody.folder')).toBe('reading');
    fireEvent.click(screen.getByRole('tab', { name: 'Reading' }));
    await waitFor(() => expect(sheet.hidden).toBe(true));
  });

  it('come back as they were left, by the old saved ids too', () => {
    localStorage.setItem('prosody.folder', 'folderMappings');
    start();
    expect(document.getElementById('folderMappings')!.hidden).toBe(false);
  });
});

describe('the sound desk', () => {
  it('lights the root and tints the scale\'s own notes on the circle', async () => {
    start();
    fireEvent.click(screen.getByRole('tab', { name: 'Sound' }));
    fireEvent.change(document.getElementById('scale')!, { target: { value: 'phrygian_dominant' } });
    await waitFor(() => expect(document.getElementById('hubScale')!.textContent).toBe('phrygian dominant'));
    const tinted = [...document.querySelectorAll('#noteCircle button.in')].map((b) => b.textContent);
    // Phrygian dominant on C: C D♭ E F G A♭ B♭.
    expect(tinted).toEqual(['C', 'C♯', 'E', 'F', 'G', 'G♯', 'A♯']);
    expect(document.querySelector('#noteCircle button[aria-pressed="true"]')!.textContent).toBe('C');
    fireEvent.click(screen.getByRole('button', { name: 'Root A' }));
    await waitFor(() => expect(document.getElementById('hubRoot')!.textContent).toBe('A'));
  });

  it('turns a knob\'s arc with its value and calls the sound custom once it leaves every palette', async () => {
    start();
    const reverb = document.getElementById('reverb') as HTMLInputElement;
    expect((document.getElementById('palette') as HTMLSelectElement).value).toBe('default');
    fireEvent.input(reverb, { target: { value: '0.6' } });
    await waitFor(() => expect(document.getElementById('reverbOut')!.textContent).toBe('0.6'));
    expect(reverb.closest<HTMLElement>('.knob')!.style.getPropertyValue('--v')).toBe('0.6000');
    expect((document.getElementById('palette') as HTMLSelectElement).value).toBe('custom');
  });
});

describe('the mappings', () => {
  it('start A at the defaults and B at what preceded them, and mark a choice that is not the default', async () => {
    start();
    const choice = (side: string, name: string) =>
      document.querySelector<HTMLSelectElement>(`#mappings select[data-side="${side}"][data-name="${name}"]`)!;
    expect(choice('A', 'bass').value).toBe('height');
    expect(choice('B', 'bass').value).toBe('off');
    expect(choice('B', 'long-stanzas').value).toBe('erode');
    expect(choice('A', 'bass').querySelector('option[value="height"]')!.textContent).toBe('height (default)');
    expect(choice('A', 'bass').className).toBe('');
    fireEvent.change(choice('A', 'bass'), { target: { value: 'feet' } });
    await waitFor(() => expect(choice('A', 'bass').className).toBe('changed'));
    expect(JSON.parse(localStorage.getItem('prosody.settings')!).A.bass).toBe('feet');
  });
});

describe('the reading', () => {
  it('offers five engines, the recorded readings first chosen', () => {
    start();
    const engine = document.getElementById('engine') as HTMLSelectElement;
    expect([...engine.options].map((o) => o.value)).toEqual(['kokoro', 'piper', 'kitten', 'recorded', 'browser']);
    expect(engine.value).toBe('recorded');
  });

  it('asks before a neural voice downloads, and keeps the voice there was when told not to', async () => {
    start();
    fireEvent.change(document.getElementById('engine')!, { target: { value: 'kitten' } });
    const download = document.getElementById('download') as HTMLDialogElement;
    await waitFor(() => expect(download.open).toBe(true));
    expect(document.getElementById('downloadTitle')!.textContent).toBe('Kitten TTS (nano) needs a one-time download of about 25 MB.');
    fireEvent.click(screen.getByText('Keep the current voice'));
    await waitFor(() => expect(download.open).toBe(false));
    expect((document.getElementById('engine') as HTMLSelectElement).value).toBe('recorded');
    expect(engines.kitten.state).toBe('idle');
  });
});

describe('the poem compiled', () => {
  it('shows what A compiled to, switches to B, and measures both side by side', async () => {
    start();
    await compile();
    await waitFor(() => expect(document.getElementById('status')!.textContent).toContain(`A · ${SUMMARY} (A)`));
    expect(document.getElementById('pattern')!.textContent).toContain('// compiled from a poem by prosody');
    // The reading's cue pattern is shown under the music, apart from it.
    expect(document.getElementById('pattern')!.textContent).toContain('// ---- spoken by this page, not part of the pattern above ----');
    // The verse is cut where the trace lights it: "Tyger" is a piece of its own.
    expect(document.getElementById('lit')!.firstElementChild!.textContent).toBe('Tyger');
    fireEvent.click(document.getElementById('hearB')!);
    await waitFor(() => expect(document.getElementById('status')!.textContent).toContain(`B · ${SUMMARY} (B)`));
    expect(document.getElementById('hearB')!.getAttribute('aria-pressed')).toBe('true');
    await waitFor(() => expect(document.getElementById('scoreRows')!.textContent).toContain('density'));
    const row = document.querySelector('#scoreRows tr')!;
    // B's density is outside its band, and up on A's.
    expect([...row.children].map((cell) => cell.textContent)).toEqual(['density', '0.20–0.60', '0.400', '0.700', '+0.300']);
    expect(row.children[3]!.className).toBe('out');
  });

  it('chooses an example poem with its palette, read in its own language', async () => {
    start();
    fireEvent.input(document.getElementById('preset')!, { target: { value: 'preseren-zdravljica' } });
    await waitFor(() => expect((document.getElementById('poem') as HTMLTextAreaElement).value).toBe('Spet trte so rodile,\n'));
    expect(language.value).toBe('sl');
    expect((document.getElementById('language') as HTMLSelectElement).value).toBe('sl');
    expect(document.getElementById('hubRoot')!.textContent).toBe('D');
  });
});
