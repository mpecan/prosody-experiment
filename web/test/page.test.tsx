// The page as a reader meets it: rendered whole, with a compiler that answers by rote.
import { fireEvent, render, screen, waitFor, within } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { restorePage } from '../src/actions';
import { App } from '../src/components/App';
import { startControls } from '../src/controls';
import { compile } from '../src/piece';
import { engines, startReading } from '../src/reading';
import { loadSettings } from '../src/settings';
import { folder, language, poem, preset, rollShown, settings, sheetOpen } from '../src/store';
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
  folder.value = 'mixer';
  sheetOpen.value = false;
  rollShown.value = true;
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
  const folders = () => document.getElementById('folders')!;

  it('open one at a time, swap, and fold on their own tab again, remembered', async () => {
    start();
    expect(folders().classList.contains('open')).toBe(false);
    fireEvent.click(screen.getByRole('tab', { name: 'Sound' }));
    await waitFor(() => expect(folders().classList.contains('open')).toBe(true));
    expect(document.getElementById('folderSound')!.hidden).toBe(false);
    expect(document.getElementById('folderReading')!.hidden).toBe(true);
    expect(screen.getByRole('tab', { name: 'Sound' }).getAttribute('aria-selected')).toBe('true');
    fireEvent.click(screen.getByRole('tab', { name: 'Reading' }));
    await waitFor(() => expect(document.getElementById('folderReading')!.hidden).toBe(false));
    expect(document.getElementById('folderSound')!.hidden).toBe(true);
    expect(localStorage.getItem('prosody.folder')).toBe('reading');
    fireEvent.click(screen.getByRole('tab', { name: 'Reading' }));
    await waitFor(() => expect(folders().classList.contains('open')).toBe(false));
    // Folded, the folder is still the one shown beside the poem on a wide screen.
    expect(document.getElementById('folderReading')!.hidden).toBe(false);
  });

  it('fold and open by the handle', async () => {
    start();
    const handle = screen.getByRole('button', { name: 'Open the settings' });
    fireEvent.click(handle);
    await waitFor(() => expect(folders().classList.contains('open')).toBe(true));
    expect(document.getElementById('folderMixer')!.hidden).toBe(false);
    fireEvent.click(screen.getByRole('button', { name: 'Fold the settings away' }));
    await waitFor(() => expect(folders().classList.contains('open')).toBe(false));
  });

  it('come back as they were left, by the old saved ids too', () => {
    localStorage.setItem('prosody.folder', 'folderMappings');
    start();
    expect(document.getElementById('folderMappings')!.hidden).toBe(false);
    expect(folders().classList.contains('open')).toBe(true);
  });
});

describe('the roll', () => {
  it('is shown or hidden from the top bar, whichever folder is open, and remembered', async () => {
    start();
    fireEvent.click(screen.getByRole('tab', { name: 'Mappings' }));
    const band = document.getElementById('rollband')!;
    const toggle = document.getElementById('rollSwitch')!;
    expect(band.hidden).toBe(false);
    expect(toggle.getAttribute('aria-pressed')).toBe('true');
    fireEvent.click(toggle);
    await waitFor(() => expect(band.hidden).toBe(true));
    expect(localStorage.getItem('prosody.roll')).toBe('hidden');
    expect(document.getElementById('folderMappings')!.hidden).toBe(false);
  });

  it('stays hidden on the next visit once hidden', () => {
    localStorage.setItem('prosody.roll', 'hidden');
    start();
    expect(document.getElementById('rollband')!.hidden).toBe(true);
  });

  it('was a folder once: a reader who left it open finds it shown, the sheet folded', () => {
    localStorage.setItem('prosody.folder', 'folderRoll');
    start();
    expect(document.getElementById('rollband')!.hidden).toBe(false);
    expect(document.getElementById('folders')!.classList.contains('open')).toBe(false);
  });
});

describe('the sound desk', () => {
  it('lights the root and tints the scale\'s own notes on the circle', async () => {
    start();
    fireEvent.click(screen.getByRole('tab', { name: 'Sound' }));
    await waitFor(() => expect(document.getElementById('folderSound')!.hidden).toBe(false));
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

describe('the patch displays', () => {
  it('show a readable name and step through the list, the real menu underneath', async () => {
    start();
    fireEvent.click(screen.getByRole('tab', { name: 'Sound' }));
    const bass = document.getElementById('bass') as HTMLSelectElement;
    const screenOf = (select: HTMLElement) => select.closest('.lcd')!.querySelector('.lcd-text')!.textContent;
    expect(bass.value).toBe('gm_acoustic_bass');
    expect(screenOf(bass)).toBe('Acoustic bass');
    fireEvent.click(screen.getByRole('button', { name: 'Next bass' }));
    await waitFor(() => expect(bass.value).toBe('gm_epiano1'));
    expect(screenOf(bass)).toBe('Electric piano');
    // From the first, ◀ goes round to the last.
    fireEvent.change(bass, { target: { value: 'triangle' } });
    fireEvent.click(screen.getByRole('button', { name: 'Previous bass' }));
    await waitFor(() => expect(bass.value).toBe('gm_vibraphone'));
  });

  it('pick an example poem from the top bar', async () => {
    start();
    const picker = document.getElementById('preset')!;
    expect(picker.closest('.lcd')!.querySelector('.lcd-text')!.textContent).toBe('Your own poem');
    fireEvent.click(screen.getByRole('button', { name: 'Next example poems' }));
    await waitFor(() => expect((document.getElementById('poem') as HTMLTextAreaElement).value).toBe('Tyger Tyger, burning bright,\n'));
  });
});

describe('the mixer', () => {
  it('sets the voice\'s level beside the band\'s, and remembers it', async () => {
    start();
    fireEvent.click(screen.getByRole('tab', { name: 'Mixer' }));
    const voice = document.getElementById('readingLevel') as HTMLInputElement;
    expect(document.getElementById('drums_gain')).toBeTruthy();
    fireEvent.input(voice, { target: { value: '1.5' } });
    await waitFor(() => expect(document.getElementById('readingLevelOut')!.textContent).toBe('1.50'));
    expect(JSON.parse(localStorage.getItem('prosody.reading')!).level).toBe(1.5);
  });
});

describe('the mappings', () => {
  const row = (name: string) => document.querySelector<HTMLElement>(`.mapping[data-name="${name}"]`)!;
  const chosen = (side: string, name: string) =>
    row(name).querySelector<HTMLElement>(`[aria-label="${name}, setting ${side}"] button[aria-pressed="true"]`)!.dataset.value;

  it('start A at the defaults and B at what preceded them, with the default marked', () => {
    start();
    expect(chosen('A', 'bass')).toBe('height');
    expect(chosen('B', 'bass')).toBe('off');
    expect(chosen('B', 'long-stanzas')).toBe('erode');
    expect(row('bass').querySelector('[aria-label="bass, setting A"] button.default')!.textContent).toBe('height');
  });

  it('glow where A and B differ, count them, and remember a choice', async () => {
    start();
    expect(row('long-stanzas').classList.contains('differ')).toBe(false);
    expect(row('bass').classList.contains('differ')).toBe(true);
    // held, bass, reading and pulse differ from the start; long-stanzas does not.
    expect(document.querySelector('#folderMappings .desk-head .hint')!.textContent).toBe('4 settings differ between A and B.');
    const b = row('bass').querySelector('[aria-label="bass, setting B"]')!;
    fireEvent.click(within(b as HTMLElement).getByText('height'));
    await waitFor(() => expect(row('bass').classList.contains('differ')).toBe(false));
    expect(document.querySelector('#folderMappings .desk-head .hint')!.textContent).toBe('3 settings differ between A and B.');
    expect(JSON.parse(localStorage.getItem('prosody.settings')!).B.bass).toBe('height');
  });

  it('say what a mapping does when asked', async () => {
    start();
    expect(row('bass').querySelector('.what')).toBeNull();
    fireEvent.click(within(row('bass')).getByRole('button', { name: 'What bass does', hidden: true }));
    await waitFor(() => expect(row('bass').querySelector('.what')!.textContent).toBe('a foundation bass'));
  });
});

describe('the reading', () => {
  const engine = () => document.getElementById('engine')!;

  it('offers five engines, the recorded readings first chosen', () => {
    start();
    expect([...engine().querySelectorAll('button')].map((b) => b.dataset.value)).toEqual(['kokoro', 'piper', 'kitten', 'recorded', 'browser']);
    expect(engine().querySelector<HTMLElement>('button[aria-pressed="true"]')!.dataset.value).toBe('recorded');
  });

  it('asks before a neural voice downloads, and keeps the voice there was when told not to', async () => {
    start();
    fireEvent.click(engine().querySelector('button[data-value="kitten"]')!);
    const download = document.getElementById('download') as HTMLDialogElement;
    await waitFor(() => expect(download.open).toBe(true));
    expect(document.getElementById('downloadTitle')!.textContent).toBe('Kitten TTS (nano) needs a one-time download of about 25 MB.');
    fireEvent.click(screen.getByText('Keep the current voice'));
    await waitFor(() => expect(download.open).toBe(false));
    expect(engine().querySelector<HTMLElement>('button[aria-pressed="true"]')!.dataset.value).toBe('recorded');
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
    fireEvent.change(document.getElementById('preset')!, { target: { value: 'preseren-zdravljica' } });
    await waitFor(() => expect((document.getElementById('poem') as HTMLTextAreaElement).value).toBe('Spet trte so rodile,\n'));
    expect(language.value).toBe('sl');
    await waitFor(() => expect((document.getElementById('language') as HTMLSelectElement).value).toBe('sl'));
    expect(document.getElementById('hubRoot')!.textContent).toBe('D');
  });
});
