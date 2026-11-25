// End-to-end: the built static site (`../scripts/build-site.sh`), the real compiler as WebAssembly,
// in Chromium. Build the site first; these tests do not.
import { defineConfig, devices } from '@playwright/test';

// Never an already-running server: one left over from another checkout answers on the same port
// with that checkout's site, and every test then passes or fails on someone else's build. A busy
// port fails loudly instead; `PROSODY_E2E_PORT` picks another.
const port = Number(process.env.PROSODY_E2E_PORT ?? 4802);
const origin = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: 'e2e',
  timeout: 60_000,
  retries: process.env.CI ? 1 : 0,
  use: { baseURL: origin, ...devices['Desktop Chrome'] },
  webServer: {
    command: `node e2e/serve.mjs ../target/site ${port}`,
    url: `${origin}/site.json`,
    reuseExistingServer: false,
  },
});
