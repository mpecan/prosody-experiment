// The static page, end to end: the real compiler answering in the browser, the Prosody player
// making the sound.
import { expect, test, type Page } from '@playwright/test';

/** The page, the welcome already seen. */
async function open(page: Page) {
  await page.addInitScript(() => localStorage.setItem('prosody.welcomed', '1'));
  await page.goto('/');
}

/** How the Prosody player is doing (src/player/index.ts `prosodyPlayer.stats`). */
type Stats = {
  underruns: number; playhead: number | null; missing: number; peak: number; speechPeak: number; spoken: number; onTime: number;
};
const stats = (page: Page) => page.evaluate(() => (globalThis as unknown as { prosodyPlayer: { stats(): Stats } }).prosodyPlayer.stats());

test('compiles the opening poem in the browser, under A and then B', async ({ page }) => {
  await open(page);
  // Blake: two quatrains, three passes each — the same summary the native compiler gives.
  await expect(page.locator('#status')).toContainText('A · 24 bars from 8 lines, 8 voices, 301 events');
  await page.click('#hearB');
  await expect(page.locator('#status')).toContainText(/^B · \d+ bars from 8 lines/);
  await page.getByRole('tab', { name: 'Under the hood' }).click();
  await expect(page.locator('#pattern')).toContainText('// compiled from a poem by prosody');
  await page.locator('#views').getByText('Score A vs B').click();
  await expect(page.locator('#scoreRows tr.total').first()).toContainText('interest');
});

test('reads an example poem in its own language, from its recordings', async ({ page }) => {
  await open(page);
  await page.selectOption('#preset', 'preseren-zdravljica');
  await expect(page.locator('#preset').locator('xpath=..').locator('.lcd-text')).toHaveText('Prešeren — Zdravljica');
  await expect(page.locator('#poem')).toHaveValue(/^Spet trte so rodile,/);
  await page.getByRole('tab', { name: 'Reading' }).click();
  await expect(page.locator('#language')).toHaveValue('sl');
  await expect(page.locator('#voice option:checked')).toContainText('lines recorded');
});

test('keeps the reader\'s theme and folder across a visit', async ({ page }) => {
  await open(page);
  await page.click('#theme');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await page.getByRole('tab', { name: 'Mappings' }).click();
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.locator('#folderMappings')).toBeVisible();
});

test('plays, lighting the poem as it goes, and stops', async ({ page }) => {
  await open(page);
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await expect(page.locator('#lit span.on').first()).toBeAttached({ timeout: 15_000 });
  await page.click('#stop');
  await expect(page.locator('#status')).toContainText('stopped');
  await expect(page.locator('#lit span.on')).toHaveCount(0);
});

test('on a wide screen the folders sit beside the poem, the roll under it whichever is open', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  const poem = await page.locator('#poem').boundingBox();
  const panel = await page.locator('#folders').boundingBox();
  expect(panel!.x).toBeGreaterThanOrEqual(poem!.x + poem!.width - 1);
  await page.getByRole('tab', { name: 'Mappings' }).click();
  await expect(page.locator('#folderMappings')).toBeVisible();
  await expect(page.locator('#roll')).toBeVisible();
  await page.click('#rollSwitch');
  await expect(page.locator('#rollband')).toBeHidden();
  await expect(page.locator('#folderMappings')).toBeVisible();
});

test('on a phone an open folder leaves the poem in view, and nothing scrolls sideways', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await open(page);
  await expect(page.locator('#sheet')).toBeHidden();
  // The transport is icons alone, on one row with the roll and A/B, each still named.
  for (const name of ['Play', 'Stop', 'Share']) await expect(page.getByRole('button', { name, exact: true })).toBeVisible();
  await expect(page.locator('#play .button-text')).toHaveCSS('width', '1px');
  const tops = await Promise.all(['#play', '#stop', '#share', '#rollSwitch', '#hearB'].map(async (id) => {
    const box = await page.locator(id).boundingBox();
    return Math.round(box!.y + box!.height / 2);
  }));
  expect(Math.max(...tops) - Math.min(...tops)).toBeLessThan(6);
  await page.getByRole('tab', { name: 'Sound' }).click();
  await expect(page.locator('#folderSound')).toBeVisible();
  const poem = await page.locator('.stage > main').boundingBox();
  expect(poem!.height).toBeGreaterThan(90);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  await page.getByRole('button', { name: 'Fold the settings away' }).click();
  await expect(page.locator('#sheet')).toBeHidden();
});

test('solos a layer while it plays: the others fall silent on the meters and faint on the roll', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await page.getByRole('tab', { name: 'Mixer' }).click();
  await page.getByRole('button', { name: 'Hear only bass' }).click();
  const meter = (channel: string) => page.locator(`.fader[data-channel="${channel}"]`).evaluate(
    (fader) => Number((fader as HTMLElement).style.getPropertyValue('--a') || 'NaN'));
  // The drums sound on every beat; soloed away, their meter falls and stays down.
  await expect.poll(() => meter('drums'), { timeout: 5_000 }).toBeLessThan(0.05);
  await expect.poll(async () => Math.max(...await Promise.all([0, 1, 2, 3].map(async () => {
    await page.waitForTimeout(150);
    return meter('bass');
  }))), { timeout: 10_000 }).toBeGreaterThan(0.5);
  await expect(page.locator('.channel[data-channel="drums"]')).toHaveClass(/quiet/);
  await page.click('#stop');
});

test('shares a poem as a link that opens to the same piece, for someone else', async ({ page, browser, baseURL }) => {
  await open(page);
  await page.selectOption('#preset', 'carroll-jabberwocky');
  await expect(page.locator('#status')).toContainText('bars from');
  await page.getByRole('tab', { name: 'Sound' }).click();
  await page.locator('#styles [data-style="techno"]').click();
  await page.click('#hearB');
  await expect(page.locator('#status')).toContainText(/^B · /);
  const summary = (await page.locator('#status').textContent())!.split(' · ')[1];
  await page.click('#share');
  await expect(page.locator('#shareLink')).toHaveValue(/\?s=[A-Za-z0-9_-]+$/);
  const link = await page.locator('#shareLink').inputValue();
  // Someone else, with a browser of their own.
  const other = await (await browser.newContext()).newPage();
  await other.addInitScript(() => localStorage.setItem('prosody.welcomed', '1'));
  await other.goto(link.replace('https://prosody.rocks', baseURL!));
  await expect(other.locator('#sharedNote')).toContainText('A shared poem.');
  await expect(other.locator('#poem')).toHaveValue(await page.locator('#poem').inputValue());
  await expect(other.locator('#hearB')).toHaveAttribute('aria-pressed', 'true');
  await expect(other.locator('#status')).toContainText(`B · ${summary}`);
  await expect(other.locator('#styles [data-style="techno"]')).toHaveAttribute('aria-pressed', 'true');
  // And its A carries what techno suggested for it.
  await other.getByRole('tab', { name: 'Mappings' }).click();
  await expect(other.locator('.mapping[data-name="bass"] [aria-label="bass, setting A"] button[aria-pressed="true"]')).toHaveText('feet');
});

test('plays and reads from beside the page, with every other host refused', async ({ page, baseURL }) => {
  // The phone app plays offline: the player, its synth, the drums, the soundfonts (our own FLAC
  // zones, scripts/render-soundfonts.mjs) and the recorded readings are all beside the page, and
  // Strudel's runtime is not loaded at all. Every other host is refused here, so anything that still
  // came from one would fail — and nothing may be missing.
  const elsewhere = new Set<string>();
  const fonts: string[] = [];
  const drums: string[] = [];
  const asked: string[] = [];
  await page.route('**/*', (route) => {
    const url = new URL(route.request().url());
    if (url.origin === new URL(baseURL!).origin) {
      asked.push(url.pathname);
      if (url.pathname.includes('/soundfonts/')) fonts.push(url.pathname);
      if (url.pathname.includes('/drums/')) drums.push(url.pathname);
      return route.continue();
    }
    elsewhere.add(url.host);
    return route.abort();
  });
  await open(page);
  // Jazz: soundfont bass and electric piano, compressed, with room and echo, on the Virtuosity kit's
  // ride.
  await page.getByRole('tab', { name: 'Sound' }).click();
  await page.locator('#styles [data-style="jazz"]').click();
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await expect.poll(async () => (await stats(page)).peak, { timeout: 20_000 }).toBeGreaterThan(0.01);
  // The first line is read from its recording, on the player's reading stem.
  await expect.poll(async () => (await stats(page)).spoken, { timeout: 20_000 }).toBeGreaterThan(0);
  expect((await stats(page)).missing).toBe(0);
  expect(drums).toContain('/drums/virtuosity/rd.flac');
  expect(fonts.some((path) => path.endsWith('0320_FluidR3_GM_sf2_file.json'))).toBe(true);
  expect(fonts.some((path) => path.endsWith('.flac'))).toBe(true);
  expect(asked.some((path) => path.startsWith('/readings/') && path.endsWith('.mp3'))).toBe(true);
  expect(asked.filter((path) => /strudel|vendor/.test(path) && !path.endsWith('strudel.json'))).toEqual([]);
  expect([...elsewhere]).toEqual([]);
  await page.click('#stop');
});

test('every drum a pattern can ask for is beside the page, in a form the browser can play', async ({ page }) => {
  await open(page);
  const decoded = await page.evaluate(async () => {
    const site = await (await fetch('/site.json')).json() as { banks: string[]; drumSamples: string[] };
    const context = new OfflineAudioContext(2, 44_100, 44_100);
    return Promise.all(site.banks.flatMap((bank) => site.drumSamples.map(async (drum) => {
      const response = await fetch(`/drums/${bank}/${drum}.flac`);
      if (!response.ok) return `${bank}/${drum}: ${response.status}`;
      const audio = await context.decodeAudioData(await response.arrayBuffer());
      return audio.duration > 0.02 ? 'ok' : `${bank}/${drum}: ${audio.duration}s`;
    })));
  });
  expect(decoded.length).toBe(45);
  expect(decoded.filter((result) => result !== 'ok')).toEqual([]);
});

test('every soundfont the compiler can name is beside the page, as our own zones', async ({ request }) => {
  // The page plays a soundfont from the file the compiler names for it (site.json), converted to
  // FLAC zones beside the page. A name with no file there would leave its notes unheard.
  const site = (await (await request.get('/site.json')).json()) as { soundfonts: Record<string, string> };
  const sounds = Object.entries(site.soundfonts);
  expect(sounds.length).toBeGreaterThan(20);
  for (const [sound, file] of sounds) {
    const manifest = await request.get(`/soundfonts/${file}.json`);
    expect(manifest.ok(), sound).toBe(true);
    const zones = ((await manifest.json()) as { zones: { file: string }[] }).zones;
    expect(zones.length, sound).toBeGreaterThan(0);
    expect((await request.head(`/soundfonts/${zones[0]!.file}`)).ok(), sound).toBe(true);
  }
});

test('the Prosody player keeps playing while the page is blocked for half a second', async ({ page }) => {
  // The failure measured on the Pixel (docs/player.md §1): a page busy for a moment made Strudel's
  // scheduler skip notes. Here the page's thread does nothing at all for 500 ms while the piece
  // plays; the render worker and the worklet are not on it, so the music must not notice.
  await open(page);
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await expect.poll(async () => (await stats(page)).playhead ?? -1, { timeout: 20_000 }).toBeGreaterThan(0.5);
  // It is making a sound.
  await expect.poll(async () => (await stats(page)).peak, { timeout: 5_000 }).toBeGreaterThan(0.01);
  // The poem lights from the Performance at the playhead.
  await expect(page.locator('#lit span.on').first()).toBeAttached({ timeout: 10_000 });
  const before = await stats(page);
  await page.evaluate(() => {
    const end = performance.now() + 500;
    while (performance.now() < end) { /* the page's thread, blocked */ }
  });
  await page.waitForTimeout(400);
  const after = await stats(page);
  expect(after.underruns).toBe(0);
  // Half a second blocked and 0.4 s waited: the playhead has moved on by about 0.9 s.
  expect(after.playhead! - before.playhead!).toBeGreaterThan(0.75);
  expect(after.missing).toBe(0);
  await expect(page.locator('#status')).not.toContainText('ran dry');
  await page.click('#stop');
  await expect(page.locator('#status')).toContainText('stopped');
  await expect(page.locator('#lit span.on')).toHaveCount(0);
});

test('reads the poem through the player: each recorded line on the reading stem, lit while it is heard', async ({ page }) => {
  // The recorded readings are the default engine; The Tyger is read from its recordings, the first
  // line where the music begins (SPEC.md §4.7). The render worker places the line on the reading's
  // stem at its frame (docs/player.md §5), so it is heard, and the page lights it at the playhead.
  await open(page);
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.getByRole('tab', { name: 'Reading' }).click();
  await expect(page.locator('#voice option:checked')).toContainText('lines recorded', { timeout: 15_000 });
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await expect.poll(async () => (await stats(page)).spoken, { timeout: 20_000 }).toBeGreaterThan(0);
  await expect.poll(async () => (await stats(page)).speechPeak, { timeout: 10_000 }).toBeGreaterThan(0.001);
  // Placed exactly on its cue, by the frame: the recording was in the worker before its bar.
  expect((await stats(page)).onTime).toBeGreaterThan(0);
  await expect(page.locator('#lit span.reading').first()).toBeAttached({ timeout: 10_000 });
  // Nothing says the reading is not played, and nothing went missing.
  await expect(page.locator('#status')).not.toContainText('not played');
  await expect(page.locator('#status')).not.toContainText('browser voice');
  await page.click('#stop');
  await expect(page.locator('#lit span.reading')).toHaveCount(0);
});

test('a muted voice is silent on the reading stem, and comes back when unmuted', async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await open(page);
  await expect(page.locator('#status')).toContainText('24 bars');
  await page.getByRole('tab', { name: 'Mixer' }).click();
  await page.getByRole('button', { name: 'Mute voice' }).click();
  await page.click('#play');
  await expect(page.locator('#play')).toHaveText('Playing', { timeout: 45_000 });
  await expect.poll(async () => (await stats(page)).spoken, { timeout: 20_000 }).toBeGreaterThan(0);
  // Placed, but at a gain of nothing: the reading's stem stays silent while the music sounds.
  await page.waitForTimeout(1_500);
  await stats(page);
  await page.waitForTimeout(1_000);
  const muted = await stats(page);
  expect(muted.peak).toBeGreaterThan(0.01);
  expect(muted.speechPeak).toBe(0);
  // The stanza is still being read: unmuted, it is heard again, within the 5 ms glide.
  await page.getByRole('button', { name: 'Mute voice' }).click();
  await expect.poll(async () => (await stats(page)).speechPeak, { timeout: 10_000 }).toBeGreaterThan(0.001);
  await page.click('#stop');
});
