#!/usr/bin/env node
// The store screenshots and Google Play's feature graphic, made from the built site in Chromium, so
// a change to the page is one command away from a new set (`mise run store:screenshots`).
//
// Each scene is the page at a phone's size, driven into a state (a poem playing, a folder open),
// photographed, then set under a caption on the icon's dark ground, at exactly the pixel size the
// store asks for. Every image is written without an alpha channel: App Store Connect refuses
// screenshots that have one, and Play wants 24-bit images.
//
//   node scripts/store-screenshots.mjs                 both stores
//   node scripts/store-screenshots.mjs android|ios     one
//
// The Android set is shot from target/site (every reading engine, as the Android app has them),
// the iOS set from target/site-ios (without the neural voices, as the iOS app is built:
// `mise run site:ios`). Both must have been built first. In the app the page reads with the
// phone's own voice, which a browser has not got: the reading scene stands in a small fake of the
// app's bridge (`__TAURI_INTERNALS__`) that answers only the voice list, so its menu is the app's.
// No scene plays through that fake; the playing scenes play through the page's own player, which
// draws the same page.
import { spawn } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { opaque } from './opaque-png.mjs';

const root = resolve(import.meta.dirname, '..');
const require = createRequire(join(root, 'web/package.json'));
const { chromium } = require('playwright');

// ── What each store asks for ─────────────────────────────────────────────────────────────────────
//
// iOS: the 6.9-inch iPhone size (1320 × 2868), the one App Store Connect requires; it scales those
// down for the smaller iPhones. The app is iPhone-only, so no iPad set. Android: 1080 × 1920, 9:16,
// the shape Play wants for an app to be shown in its recommendations (no side over twice the other).
// The page is shot at a phone's width in CSS pixels and scaled to fit under the caption.
const TARGETS = {
  ios: {
    site: 'target/site-ios',
    out: 'store-assets/ios/screenshots/en-US',
    canvas: { width: 1320, height: 2868 },
    caption: 520,
    phone: { width: 390, height: 768, scale: 3 },
    agent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148',
    // What iOS gives for its voices (SpeechPlugin.swift): names, tiers, the phone's choice first.
    voices: {
      voices: [
        { id: 'com.apple.voice.premium.en-US.Ava', lang: 'en-US', name: 'Ava', tier: 'Premium', rank: 40, good: true },
        { id: 'com.apple.voice.premium.en-AU.Karen', lang: 'en-AU', name: 'Karen', tier: 'Premium', rank: 40, good: true },
        { id: 'com.apple.voice.enhanced.en-GB.Daniel', lang: 'en-GB', name: 'Daniel', tier: 'Enhanced', rank: 30, good: true },
        { id: 'com.apple.voice.compact.sl-SI.Tina', lang: 'sl-SI', name: 'Tina', tier: null, rank: 20, good: true },
      ],
      default: 'com.apple.voice.premium.en-US.Ava',
      engine: 'AVSpeechSynthesizer',
      improve: null,
    },
  },
  android: {
    site: 'target/site',
    out: 'store-assets/android/en-US/images/phoneScreenshots',
    canvas: { width: 1080, height: 1920 },
    caption: 400,
    phone: { width: 390, height: 640, scale: 2.4 },
    agent: 'Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36',
    // What Android's TextToSpeech gives (SpeechPlugin.kt): ids only, ranked by quality.
    voices: {
      voices: [
        { id: 'en-us-x-iob-local', lang: 'en-US', rank: 400, good: true },
        { id: 'en-gb-x-gbd-local', lang: 'en-GB', rank: 400, good: true },
        { id: 'en-au-x-aub-local', lang: 'en-AU', rank: 400, good: true },
        { id: 'sl-si-language', lang: 'sl-SI', rank: 300, good: true },
      ],
      default: 'en-us-x-iob-local',
      engine: 'com.google.android.tts',
      improve: null,
    },
  },
};

// The roll's colours (styles.css): one to each scene, for its caption's rule.
const PULSE = '#c9a6ff', ORNAMENT = '#6fd6c5', BASS = '#ffb86b', RHYME = '#ff7aa8', CHORDS = '#8fb3ff';

// A short poem of our own for "write your own", so no one else's words are in the listing.
const OWN_POEM = `Rain on the roof at four,
a tap I can't ignore;
the gutter keeps the time,
the window finds the rhyme.

The kettle starts to sing,
the radiator ring,
and all the house, in turn,
has something it can learn.`;

// ── The scenes, in the order the stores show them ────────────────────────────────────────────────

const SCENES = [
  {
    name: '01-poem-plays',
    title: 'A poem becomes music',
    line: 'Stresses become the beat, rhymes become notes, and repeated lines return as loops.',
    accent: PULSE,
    async act(page) {
      await choosePoem(page, 'blake-tyger');
      await play(page);
    },
  },
  {
    name: '02-every-sound-a-word',
    title: 'Every sound traces back to a word',
    line: 'The words light up as they play. Under the hood, see what each one became.',
    accent: CHORDS,
    async act(page) {
      await choosePoem(page, 'carroll-jabberwocky');
      await play(page);
      await openFolder(page, 'Under the hood');
      await page.locator('#views').getByRole('button', { name: 'What each word became' }).click();
    },
  },
  {
    name: '03-write-your-own',
    title: 'Write your own',
    line: 'Change a word and hear exactly what it changed. Nothing else in the piece moves.',
    accent: ORNAMENT,
    theme: 'dark',
    async act(page) {
      await page.locator('textarea#poem').fill(OWN_POEM);
      await page.waitForFunction(() => document.querySelector('#status')?.textContent?.includes('bars'));
      await play(page);
    },
  },
  {
    name: '04-hear-it-two-ways',
    title: 'Hear it two ways',
    line: 'Set how rhyme, stress and refrain are heard on A and on B, and switch between them.',
    accent: RHYME,
    async act(page) {
      await choosePoem(page, 'poe-annabel-lee');
      await play(page);
      await openFolder(page, 'Mappings');
    },
  },
  {
    name: '05-choose-its-sound',
    title: 'Choose its sound',
    line: 'Piano, vibraphone, a jazz trio or bare waves. The style changes the sound, never the structure.',
    accent: BASS,
    async act(page) {
      await choosePoem(page, 'dickinson-because');
      await play(page);
      await openFolder(page, 'Sound');
    },
  },
  {
    name: '06-mix-the-band',
    title: 'Mix the band and the voice',
    line: 'Every layer on its own fader: mute it, solo it, watch it move.',
    accent: PULSE,
    theme: 'dark',
    async act(page) {
      await choosePoem(page, 'hopkins-pied-beauty');
      await play(page);
      await openFolder(page, 'Mixer');
    },
  },
  {
    name: '07-a-voice-reads-it',
    title: 'A voice reads it over the music',
    line: 'Recorded readings of the example poems, or your phone’s own voices for anything you write.',
    accent: ORNAMENT,
    app: true,
    async act(page) {
      await choosePoem(page, 'shakespeare-sonnet-18');
      await openFolder(page, 'Reading');
      // The phone's own voice chosen, by name: the menu below the engines.
      await page.locator('#voice').evaluate((menu) => menu.closest('.patch')?.scrollIntoView({ block: 'end' }));
      await page.waitForTimeout(300);
    },
  },
];

async function choosePoem(page, name) {
  await page.locator('select#preset').selectOption(name);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent?.includes('bars'));
}

async function play(page) {
  await page.click('#play');
  await page.locator('#play').filter({ hasText: 'Playing' }).waitFor({ timeout: 60_000 });
  // Far enough in that the roll has moved and a few words are lit.
  await page.waitForTimeout(5_000);
}

async function openFolder(page, tab) {
  await page.getByRole('tab', { name: tab }).click();
  await page.waitForTimeout(800);
}

// ── Shooting a scene ────────────────────────────────────────────────────────────────────────────

async function shoot(browser, base, target, scene) {
  const { phone } = target;
  const context = await browser.newContext({
    viewport: { width: phone.width, height: phone.height }, deviceScaleFactor: phone.scale,
    isMobile: true, hasTouch: true, userAgent: target.agent,
    colorScheme: scene.theme === 'dark' ? 'dark' : 'light',
  });
  const page = await context.newPage();
  await page.addInitScript(({ theme, voices, app }) => {
    localStorage.setItem('prosody.welcomed', '1');
    localStorage.setItem('prosody.theme', theme);
    // A scene that does not play has nothing on the roll: it would be an empty band.
    if (app) localStorage.setItem('prosody.roll', 'hidden');
    if (app) {
      // The app's bridge, as far as the voice menu asks it; anything else is refused, loudly.
      Object.defineProperty(globalThis, '__TAURI_INTERNALS__', {
        value: {
          invoke: async (command) => {
            if (command === 'speech_voices') return voices;
            if (command === 'speech_release') return null;
            throw new Error(`store-screenshots: the fake bridge does not answer ${command}`);
          },
        },
      });
    }
  }, { theme: scene.theme ?? 'light', voices: target.voices, app: Boolean(scene.app) });
  const failures = [];
  page.on('pageerror', (error) => failures.push(error.message));
  await page.goto(base);
  await page.waitForFunction(() => document.querySelector('#status')?.textContent?.includes('bars'), null, { timeout: 60_000 });
  // The first visit's notice says what the captions say; the screenshot has no room for both.
  if (await page.locator('#noticeClose').isVisible()) await page.click('#noticeClose');
  await scene.act(page);
  const shot = await page.screenshot();
  if (await page.locator('#play').filter({ hasText: 'Playing' }).count()) await page.click('#stop');
  await context.close();
  if (failures.length) throw new Error(`${scene.name}: the page failed: ${failures.join('; ')}`);
  return shot;
}

// ── Setting it under its caption ─────────────────────────────────────────────────────────────────

const font = (file) => readFileSync(join(root, 'web/node_modules/@fontsource', file)).toString('base64');
const FONTS = `
  @font-face { font-family: Spectral; font-weight: 500; src: url(data:font/woff2;base64,${font('spectral/files/spectral-latin-500-normal.woff2')}); }
  @font-face { font-family: Spectral; font-weight: 500; font-style: italic; src: url(data:font/woff2;base64,${font('spectral/files/spectral-latin-500-italic.woff2')}); }
  @font-face { font-family: Spectral; font-weight: 400; src: url(data:font/woff2;base64,${font('spectral/files/spectral-latin-400-normal.woff2')}); }`;
const GROUND = '#15141c';

function frame(target, scene, shot) {
  const { canvas, caption, phone } = target;
  const width = phone.width * phone.scale;
  const height = phone.height * phone.scale;
  const left = (canvas.width - width) / 2;
  const unit = canvas.width / 1080; // the Android canvas is the measure
  return `<!doctype html><html><head><meta charset="utf-8"><style>${FONTS}
    html, body { margin: 0; width: ${canvas.width}px; height: ${canvas.height}px; overflow: hidden;
      background: radial-gradient(120% 70% at 50% 0%, #22202e 0%, ${GROUND} 60%, #100f16 100%); }
    .caption { position: absolute; left: ${left}px; right: ${left}px; top: 0; height: ${caption}px;
      display: flex; flex-direction: column; justify-content: center; color: #f3eee4; }
    .rule { width: ${64 * unit}px; height: ${7 * unit}px; border-radius: 99px; background: ${scene.accent}; margin-bottom: ${26 * unit}px; }
    h1 { font: 500 ${74 * unit}px/1.08 Spectral, serif; margin: 0 0 ${20 * unit}px; letter-spacing: -0.01em; text-wrap: balance; }
    p { font: 400 ${35 * unit}px/1.35 Spectral, serif; margin: 0; color: #bdb7cc; text-wrap: balance; }
    .phone { position: absolute; left: ${left}px; top: ${caption}px; width: ${width}px; height: ${height}px;
      border-radius: ${44 * unit}px; overflow: hidden; box-shadow: 0 ${30 * unit}px ${80 * unit}px rgba(0,0,0,.55), 0 0 0 ${3 * unit}px #2e2c3a; }
    .phone img { display: block; width: 100%; height: 100%; }
  </style></head><body>
    <div class="caption"><div class="rule"></div><h1>${scene.title}</h1><p>${scene.line}</p></div>
    <div class="phone"><img src="data:image/png;base64,${shot.toString('base64')}"></div>
  </body></html>`;
}

async function compose(browser, html, size) {
  const page = await browser.newPage({ viewport: size, deviceScaleFactor: 1 });
  await page.setContent(html);
  await page.evaluate(() => document.fonts.ready);
  const png = await page.screenshot({ clip: { x: 0, y: 0, ...size } });
  await page.close();
  return opaque(png);
}

// ── Google Play's feature graphic (1024 × 500): the icon's roll and the name ────────────────────

function featureGraphic() {
  const icon = readFileSync(join(root, 'store-assets/icon/icon-full.svg'), 'utf8');
  return `<!doctype html><html><head><meta charset="utf-8"><style>${FONTS}
    html, body { margin: 0; width: 1024px; height: 500px; overflow: hidden;
      background: radial-gradient(90% 120% at 30% 50%, #1e1c27 0%, ${GROUND} 60%, #100f16 100%); }
    .roll { position: absolute; left: 40px; top: 30px; width: 440px; height: 440px; }
    .roll svg { width: 100%; height: 100%; }
    .name { position: absolute; left: 500px; top: 140px; right: 40px; color: #f3eee4; }
    .word { font: italic 500 112px/1 Spectral, serif; letter-spacing: -0.01em; }
    .word span:nth-child(1) { color: ${PULSE}; } .word span:nth-child(2) { color: ${BASS}; } .word span:nth-child(3) { color: ${RHYME}; }
    .line { font: 400 31px/1.3 Spectral, serif; color: #bdb7cc; margin-top: 22px; white-space: nowrap; }
  </style></head><body>
    <div class="roll">${icon.replace(/<rect width="1024" height="1024"[^>]*\/>/, '')}</div>
    <div class="name"><div class="word"><span>pro</span><span>so</span><span>dy</span></div>
      <div class="line">A poem becomes music.<br>Every sound comes from a word.</div></div>
  </body></html>`;
}

// ── The run ──────────────────────────────────────────────────────────────────────────────────────

function serve(dir, port) {
  const server = spawn(process.execPath, [join(root, 'web/e2e/serve.mjs'), join(root, dir), String(port)], { stdio: ['ignore', 'pipe', 'inherit'] });
  return new Promise((ready, failed) => {
    server.stdout.once('data', () => ready(server));
    server.once('exit', (code) => failed(new Error(`the server for ${dir} exited (${code}) — is it built?`)));
  });
}

const wanted = process.argv.slice(2);
const names = wanted.length ? wanted : Object.keys(TARGETS);
for (const name of names) if (!TARGETS[name]) throw new Error(`store-screenshots: no target "${name}" (android, ios)`);

const browser = await chromium.launch({ args: ['--autoplay-policy=no-user-gesture-required'] });
try {
  let port = 4840;
  for (const name of names) {
    const target = TARGETS[name];
    const server = await serve(target.site, ++port);
    try {
      const out = join(root, target.out);
      mkdirSync(out, { recursive: true });
      for (const scene of SCENES) {
        const shot = await shoot(browser, `http://127.0.0.1:${port}/`, target, scene);
        writeFileSync(join(out, `${scene.name}.png`), await compose(browser, frame(target, scene, shot), target.canvas));
        console.log(`store-screenshots: ${name} ${scene.name}`);
      }
    } finally {
      server.kill();
    }
    if (name === 'android') {
      const graphic = join(root, 'store-assets/android/en-US/images/featureGraphic.png');
      writeFileSync(graphic, await compose(browser, featureGraphic(), { width: 1024, height: 500 }));
      console.log('store-screenshots: android featureGraphic');
    }
  }
} finally {
  await browser.close();
}
