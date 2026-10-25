/// <reference types="vitest/config" />
// The page, built into dist/ for `prosody-web` to serve and `scripts/build-site.sh` to copy beside
// the compiler. Paths are relative (`base: './'`), so the static page works from a subdirectory of
// a host as well as from `/`.
//
// `npm run dev` serves the page with hot reload and sends its questions — the page's data and every
// compiled view, and the recorded readings — to a `prosody-web` running on localhost:4747
// (`cargo run -p prosody-web`), which compiles natively: the page's data then says "server".
import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

const compiler = 'http://127.0.0.1:4747';
const asked = ['/site.json', '/presets.json', '/pattern', '/explain', '/score', '/reading', '/trace', '/readings'];

export default defineConfig({
  base: './',
  plugins: [preact()],
  server: {
    proxy: Object.fromEntries(asked.map((path) => [path, compiler])),
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  test: {
    environment: 'happy-dom',
    include: ['test/**/*.test.{ts,tsx}'],
    setupFiles: ['test/setup.ts'],
  },
});
