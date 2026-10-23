// Invariant I1 across targets: the page's WebAssembly compiler must answer exactly what the native
// server answers. Run by `scripts/check-wasm.sh`, which builds both and starts the server.
//
//   node scripts/check-wasm.mjs SITE_DIR SERVER_URL EXAMPLES_DIR
//
// Asks both every view of every example under a handful of settings — defaults, other palettes,
// every candidate mapping's variants — and fails on the first byte that differs.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [site, server, examples] = process.argv.slice(2);
// The same generated bindings the page loads, given the module's bytes instead of its URL.
const bindings = await import(pathToFileURL(join(site, 'prosody.js')).href);
await bindings.default({ module_or_path: readFileSync(join(site, 'prosody_bg.wasm')) });

function ask(target, body) {
  const answer = bindings.respond(target, body);
  const reply = { status: answer.status, body: answer.body };
  answer.free();
  return reply;
}

const settings = [
  '',
  'scale=lydian&root=F%23&swing=0.33&humanise=0.5&density_level=0.5',
  'set=harmony:both,bass:feet,reading:stanza',
  'set=harmony:rhyme,bass:syllables&voicing=seventh',
  'set=harmony:cadence&off=outro,period&profile=jazz',
  'off=held,refrain,bass&profile=interest',
];
const views = ['/pattern', '/explain', '/reading', '/score'];

let compared = 0;
for (const file of readdirSync(examples).filter((name) => name.endsWith('.poem')).sort()) {
  const poem = readFileSync(join(examples, file), 'utf8');
  for (const query of settings) {
    for (const view of views) {
      const target = view + (query ? '?' + query : '');
      const served = await fetch(server + target, { method: 'POST', body: poem });
      const expected = { status: served.status, body: await served.text() };
      const got = ask(target, poem);
      if (got.status !== expected.status || got.body !== expected.body) {
        const at = [...got.body].findIndex((c, i) => c !== [...expected.body][i]);
        console.error(`check-wasm: ${file} ${target}: wasm ${got.status}, server ${expected.status}`);
        console.error(`  first difference at character ${at}:`);
        console.error(`  wasm:   ${JSON.stringify(got.body.slice(Math.max(0, at - 40), at + 40))}`);
        console.error(`  server: ${JSON.stringify(expected.body.slice(Math.max(0, at - 40), at + 40))}`);
        process.exit(1);
      }
      compared += 1;
    }
  }
}
console.log(`check-wasm: ${compared} answers byte-identical between the wasm and native builds`);
