// The static page, end to end: the real compiler answering in the browser.
import { expect, test, type Page } from '@playwright/test';

/** The page, the welcome already seen. */
async function open(page: Page) {
  await page.addInitScript(() => localStorage.setItem('prosody.welcomed', '1'));
  await page.goto('/');
}

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
  // Strudel, the drum machines and the soundfonts come from their CDNs on the first Play.
  await expect(page.locator('#play')).toHaveText('▶ Playing', { timeout: 45_000 });
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
  await expect(page.locator('#play')).toHaveText('▶ Playing', { timeout: 45_000 });
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

test('shares a poem as a link that opens to the same piece, for someone else', async ({ page, browser }) => {
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
  await other.goto(link.replace('https://prosody.rocks', 'http://127.0.0.1:4802'));
  await expect(other.locator('#sharedNote')).toContainText('A shared poem.');
  await expect(other.locator('#poem')).toHaveValue(await page.locator('#poem').inputValue());
  await expect(other.locator('#hearB')).toHaveAttribute('aria-pressed', 'true');
  await expect(other.locator('#status')).toContainText(`B · ${summary}`);
  await expect(other.locator('#styles [data-style="techno"]')).toHaveAttribute('aria-pressed', 'true');
  // And its A carries what techno suggested for it.
  await other.getByRole('tab', { name: 'Mappings' }).click();
  await expect(other.locator('.mapping[data-name="bass"] [aria-label="bass, setting A"] button[aria-pressed="true"]')).toHaveText('feet');
});
