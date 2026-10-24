// Checks deploy/worker.js before it is deployed: every way in reaches https://prosody.rocks, and
// what is served there says to stay on https. Run by scripts/deploy.sh; needs only node.
import assert from 'node:assert/strict';
import worker from './worker.js';
import { HSTS, redirectFor } from './redirect.js';

// Hand-worked: the redirect keeps path and query, drops www and http, and leaves the rest alone.
const cases = [
  ['http://prosody.rocks/', 'https://prosody.rocks/'],
  ['http://prosody.rocks/index.html?x=1', 'https://prosody.rocks/index.html?x=1'],
  ['https://www.prosody.rocks/', 'https://prosody.rocks/'],
  ['http://www.prosody.rocks/prosody_bg.wasm', 'https://prosody.rocks/prosody_bg.wasm'],
  ['https://prosody.rocks/', null],
  ['http://localhost:8787/', null],
  ['https://prosody.workers.dev/', null],
];
for (const [from, to] of cases) assert.equal(redirectFor(from), to, from);

// Through the handler: a redirect is a 301 with the place; a page on https carries HSTS; a page
// on any other host is the asset's own answer.
const env = { ASSETS: { fetch: async () => new Response('page', { status: 200 }) } };
const moved = await worker.fetch(new Request('http://www.prosody.rocks/a'), env);
assert.equal(moved.status, 301);
assert.equal(moved.headers.get('location'), 'https://prosody.rocks/a');
const served = await worker.fetch(new Request('https://prosody.rocks/'), env);
assert.equal(served.status, 200);
assert.equal(served.headers.get('strict-transport-security'), HSTS);
assert.equal(await served.text(), 'page');
const local = await worker.fetch(new Request('http://localhost:8787/'), env);
assert.equal(local.headers.get('strict-transport-security'), null);

console.log(`check-worker: ${cases.length} redirects and 3 answers as expected`);
