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
const asked = ['/site.json', '/presets.json', '/pattern', '/explain', '/score', '/reading', '/trace', '/performance', '/readings'];

// The page makes its sound with the Prosody player (src/player, docs/player.md); Strudel's runtime
// is not shipped. `@strudel/core`, `@strudel/mini` and `@strudel/transpiler` are dev dependencies for
// the parity test alone (test/parity.test.ts), which evaluates the pattern text the page shows.

export default defineConfig({
  base: './',
  plugins: [preact()],
  server: {
    proxy: Object.fromEntries(asked.map((path) => [path, compiler])),
  },
  // The Prosody player's render worker and worklet (src/player) are built as modules of their own:
  // the worker imports the synth's bindings from beside the page at run time, and an AudioWorklet
  // loads a module.
  worker: { format: 'es' },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  test: {
    environment: 'happy-dom',
    include: ['test/**/*.test.{ts,tsx}'],
    setupFiles: ['test/setup.ts'],
    // The parity test (test/parity.test.ts) evaluates patterns with @strudel/core. Left to Node,
    // its import of @kabelsalat/web resolves to that package's `main`, a browser bundle with no
    // named exports, and fails; through Vite it resolves to `module`, the ES build.
    server: { deps: { inline: [/@strudel\//, /@kabelsalat\//] } },
  },
});
