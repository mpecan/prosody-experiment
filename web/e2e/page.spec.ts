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
  await page.click('#tabScore');
  await expect(page.locator('#scoreRows tr.total').first()).toContainText('interest');
});

test('reads an example poem in its own language, from its recordings', async ({ page }) => {
  await open(page);
  await page.selectOption('#preset', 'preseren-zdravljica');
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
