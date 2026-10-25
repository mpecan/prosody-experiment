// End-to-end: the built static site (`../scripts/build-site.sh`), the real compiler as WebAssembly,
// in Chromium. Build the site first; these tests do not.
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: 'e2e',
  timeout: 60_000,
  retries: process.env.CI ? 1 : 0,
  use: { baseURL: 'http://127.0.0.1:4802', ...devices['Desktop Chrome'] },
  webServer: {
    command: 'node e2e/serve.mjs ../target/site 4802',
    url: 'http://127.0.0.1:4802/site.json',
    reuseExistingServer: !process.env.CI,
  },
});
