// Checks deploy/worker.js before it is deployed: every way in reaches https://prosody.rocks, and
// what is served there says to stay on https. Run by scripts/deploy.sh; needs only node.
import assert from 'node:assert/strict';
import worker from './worker.js';
import { escape, MOST, previewOf, sharedIn, withPreview } from './preview.js';
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

const stagingCases = [
  ['http://staging.prosody.rocks/', 'https://staging.prosody.rocks/'],
  ['http://staging.prosody.rocks/index.html?x=1', 'https://staging.prosody.rocks/index.html?x=1'],
  ['https://www.staging.prosody.rocks/', 'https://staging.prosody.rocks/'],
  ['http://www.staging.prosody.rocks/prosody_bg.wasm', 'https://staging.prosody.rocks/prosody_bg.wasm'],
  ['https://staging.prosody.rocks/', null],
];
for (const [from, to] of stagingCases) assert.equal(redirectFor(from, 'staging.prosody.rocks'), to, from);

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

const stagingEnv = { CANONICAL_HOST: 'staging.prosody.rocks', ASSETS: { fetch: async () => new Response('staging-page', { status: 200 }) } };
const stagingMoved = await worker.fetch(new Request('http://staging.prosody.rocks/a'), stagingEnv);
assert.equal(stagingMoved.status, 301);
assert.equal(stagingMoved.headers.get('location'), 'https://staging.prosody.rocks/a');
const stagingServed = await worker.fetch(new Request('https://staging.prosody.rocks/'), stagingEnv);
assert.equal(stagingServed.status, 200);
assert.equal(stagingServed.headers.get('strict-transport-security'), HSTS);
assert.equal(await stagingServed.text(), 'staging-page');

// A shared poem's preview. A link is made here as the page makes it (web/src/share.ts): JSON,
// deflated raw, base64url.
async function linkTo(shared) {
  const reader = new Blob([JSON.stringify(shared)]).stream().pipeThrough(new CompressionStream('deflate-raw')).getReader();
  const parts = [];
  for (;;) { const { done, value } = await reader.read(); if (done) break; parts.push(...value); }
  const code = btoa(String.fromCharCode(...parts)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
  return `https://prosody.rocks/?s=${code}`;
}
const tyger = await linkTo({ v: 1, poem: 'Tyger Tyger, burning bright,\nIn the forests of the night;\n\nWhat immortal hand or eye,\nCould frame thy fearful symmetry?\nIn what distant deeps\n', style: 'jazz' });
assert.deepEqual(previewOf(await sharedIn(tyger)), {
  title: 'Tyger Tyger, burning bright, — prosody',
  description: 'In the forests of the night; / What immortal hand or eye, / Could frame thy fearful symmetry? — A poem played as music, jazz, on prosody.rocks.',
});

// A base poem link resolves from the catalog without carrying the full text.
const baseTyger = await linkTo({ v: 1, base: 'blake-tyger', style: 'jazz' });
assert.deepEqual(previewOf(await sharedIn(baseTyger)), {
  title: 'Tyger Tyger, burning bright, — prosody',
  description: 'In the forests of the night; / What immortal hand or eye, / Could frame thy fearful symmetry? — A poem played as music, jazz, on prosody.rocks.',
});
assert.ok(baseTyger.length < 90, `baseTyger URL length ${baseTyger.length} should be under 90`);

// Slices and patches resolve from the catalog as well.
const slicedWhitman = await linkTo({ v: 1, base: 'whitman-song-of-myself', slice: [0, 3] });
assert.deepEqual(previewOf(await sharedIn(slicedWhitman)), {
  title: 'I celebrate myself, and sing myself, — prosody',
  description: 'And what I assume you shall assume, / For every atom belonging to me as good belongs to you. — A poem played as music, on prosody.rocks.',
});

const patchedTyger = await linkTo({ v: 1, base: 'blake-tyger', patch: { 0: 'Lion Lion, burning bright,' } });
assert.deepEqual(previewOf(await sharedIn(patchedTyger)), {
  title: 'Lion Lion, burning bright, — prosody',
  description: 'In the forests of the night; / What immortal hand or eye, / Could frame thy fearful symmetry? — A poem played as music, on prosody.rocks.',
});

// One line, the default style: a title and the site's line.
assert.deepEqual(previewOf({ poem: 'a single line', style: 'default' }),
  { title: 'a single line — prosody', description: 'A poem played as music, on prosody.rocks.' });
// Not a link to a poem: junk, a later format, an empty poem, an unknown base, or one made to unpack to too much.
for (const url of ['https://prosody.rocks/?s=junk', await linkTo({ v: 2, poem: 'x' }), await linkTo({ v: 1, poem: '  \n' }),
  await linkTo({ v: 1, base: 'non-existent-base-poem' }),
  await linkTo({ v: 1, poem: 'a'.repeat(4 * MOST) })]) {
  assert.equal(await sharedIn(url), null, url.slice(0, 60));
}
// A poem is text, never markup: in an attribute or in the title it is escaped.
assert.equal(escape(`"><script>alert('x')</script>&`), '&quot;&gt;&lt;script&gt;alert(&#39;x&#39;)&lt;/script&gt;&amp;');
const head = '<title>prosody</title><meta name="description" content="site"><meta property="og:title" content="site">'
  + '<meta property="og:description" content="site"><meta property="og:url" content="https://prosody.rocks/">';
const hostile = await linkTo({ v: 1, poem: '"><script>alert(1)</script>\nsecond' });
const rewritten = withPreview(head, previewOf(await sharedIn(hostile)), hostile);
assert.ok(!rewritten.includes('<script>'), rewritten);
assert.ok(rewritten.startsWith('<title>&quot;&gt;&lt;script&gt;alert(1)&lt;/script&gt; — prosody</title>'), rewritten);
assert.equal(rewritten.match(/content="second — A poem played as music, on prosody.rocks."/g).length, 2);
// Through the handler: the page for a shared link carries the poem's preview, and any other answer
// is the asset's own.
const pages = { ASSETS: { fetch: async () => new Response(head, { headers: { 'content-type': 'text/html; charset=utf-8' } }) } };
const previewed = await (await worker.fetch(new Request(tyger), pages)).text();
assert.ok(previewed.includes('<title>Tyger Tyger, burning bright, — prosody</title>'), previewed);
assert.ok(previewed.includes(`<meta property="og:url" content="${escape(tyger)}">`), previewed);
assert.equal(await (await worker.fetch(new Request('https://prosody.rocks/?s=junk'), pages)).text(), head);
assert.equal(await (await worker.fetch(new Request('https://prosody.rocks/'), pages)).text(), head);

console.log(`check-worker: ${cases.length + stagingCases.length} redirects, 5 answers and a shared poem's preview as expected`);
