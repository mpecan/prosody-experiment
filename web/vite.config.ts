/// <reference types="vitest/config" />
// The page, built into dist/ for `prosody-web` to serve and `scripts/build-site.sh` to copy beside
// the compiler. Paths are relative (`base: './'`), so the static page works from a subdirectory of
// a host as well as from `/`.
//
// `npm run dev` serves the page with hot reload and sends its questions — the page's data and every
// compiled view, and the recorded readings — to a `prosody-web` running on localhost:4747
// (`cargo run -p prosody-web`), which compiles natively: the page's data then says "server".
import { readFileSync } from 'node:fs';
import { defineConfig, type Plugin } from 'vite';
import preact from '@preact/preset-vite';

const compiler = 'http://127.0.0.1:4747';
const asked = ['/site.json', '/presets.json', '/pattern', '/explain', '/score', '/reading', '/trace', '/performance', '/readings'];

// Strudel, served from beside the page rather than a CDN, so the phone app plays with no network
// and the page plays the exact versions package-lock.json pins. The files are copied as published:
// `@strudel/web` finds its clock worker beside its own script, and the other two import Strudel by
// bare name, which the page's import map answers (public/strudel-globals.mjs).
const vendored: Record<string, string> = {
  'vendor/strudel-web.js': '@strudel/web/dist/index.js',
  'vendor/assets/clockworker-ZDiUtESR.js': '@strudel/web/dist/assets/clockworker-ZDiUtESR.js',
  'vendor/strudel-draw.mjs': '@strudel/draw/draw.mjs',
  'vendor/strudel-soundfonts.mjs': '@strudel/soundfonts/dist/index.mjs',
};
const source = (pkgPath: string) => readFileSync(new URL(`./node_modules/${pkgPath}`, import.meta.url));

function vendor(): Plugin {
  return {
    name: 'vendor-strudel',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const pkgPath = vendored[(request.url ?? '').split('?')[0].replace(/^\//, '')];
        if (!pkgPath) return next();
        response.setHeader('Content-Type', 'text/javascript');
        response.end(source(pkgPath));
      });
    },
    generateBundle() {
      for (const [fileName, pkgPath] of Object.entries(vendored)) {
        this.emitFile({ type: 'asset', fileName, source: source(pkgPath) });
      }
    },
  };
}

export default defineConfig({
  base: './',
  plugins: [preact(), vendor()],
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
    // The parity test (test/parity.test.ts) evaluates patterns with @strudel/core. Left to Node,
    // its import of @kabelsalat/web resolves to that package's `main`, a browser bundle with no
    // named exports, and fails; through Vite it resolves to `module`, the ES build.
    server: { deps: { inline: [/@strudel\//, /@kabelsalat\//] } },
  },
});
