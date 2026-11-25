// Writes notices.json into the built site: the software the site and the apps ship, each package
// with its licence and the licence texts it carries, for the page's Licences dialog
// (web/src/components/dialogs/Licences.tsx). Most of these licences (MIT, BSD, Apache-2.0, Zlib,
// Unicode) ask that their notice go with every copy, and the apps are copies.
//
//   node scripts/notices.mjs DIR
//
// What ships: every crate the compiler (prosody-wasm), the synth (prosody-synth-wasm) and the app
// (prosody-app, for Android and iOS) link, from `cargo metadata` — normal dependencies only, not
// build scripts' or tests' — and every npm package the page's bundle carries (web/package.json's
// dependencies, and theirs). Each package's licence files are read from where cargo and npm put
// its source; one that carries none is given its licence's standard text (below).
// The output is the same for the same lockfiles: packages in name order, each text stored once.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

const out = process.argv[2];
if (!out) {
  console.error('usage: node scripts/notices.mjs DIR');
  process.exit(2);
}
const root = join(dirname(new URL(import.meta.url).pathname), '..');

// LICENSE, LICENSE-MIT, LICENCE.txt, COPYING, NOTICE, UNLICENSE, OFL.txt …
const LICENCE_FILE = /^(licen[cs]e|copying|notice|unlicense|ofl)([-._].*)?$/i;

const texts = new Map();   // id → text
function textsIn(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && LICENCE_FILE.test(entry.name))
    .map((entry) => entry.name)
    .sort()
    .map((name) => {
      const text = readFileSync(join(dir, name), 'utf8').replace(/\r\n/g, '\n').trim();
      const id = createHash('sha256').update(text).digest('hex').slice(0, 16);
      texts.set(id, text);
      return id;
    });
}

// ---- Crates ----

const PLATFORMS = ['aarch64-linux-android', 'aarch64-apple-ios', 'wasm32-unknown-unknown'];
const ROOTS = ['prosody-app', 'prosody-wasm', 'prosody-synth-wasm'];
const metadata = JSON.parse(execFileSync('cargo', [
  'metadata', '--format-version', '1', '--locked', ...PLATFORMS.flatMap((p) => ['--filter-platform', p]),
], { cwd: root, maxBuffer: 256 * 1024 * 1024 }).toString());
const byId = new Map(metadata.packages.map((p) => [p.id, p]));
const nodes = new Map(metadata.resolve.nodes.map((n) => [n.id, n]));
const reached = new Set();
const todo = metadata.packages.filter((p) => ROOTS.includes(p.name)).map((p) => p.id);
if (todo.length !== ROOTS.length) throw new Error(`notices: expected the crates ${ROOTS.join(', ')}`);
while (todo.length) {
  const id = todo.pop();
  if (reached.has(id)) continue;
  reached.add(id);
  for (const dep of nodes.get(id)?.deps ?? []) {
    if (dep.dep_kinds.some((kind) => kind.kind === null)) todo.push(dep.pkg);
  }
}
const packages = [];
for (const id of reached) {
  const crate = byId.get(id);
  if (!crate.source) continue;   // the workspace's own crates: Prosody, under its own licence
  packages.push({
    kind: 'crate', name: crate.name, version: crate.version, licence: crate.license ?? crate.license_file ?? 'unstated',
    url: crate.repository ?? `https://crates.io/crates/${crate.name}`, texts: textsIn(dirname(crate.manifest_path)),
  });
}

// ---- npm ----

const web = join(root, 'web');
const seen = new Set();
const npmTodo = Object.keys(JSON.parse(readFileSync(join(web, 'package.json'), 'utf8')).dependencies ?? {});
while (npmTodo.length) {
  const name = npmTodo.pop();
  if (seen.has(name)) continue;
  seen.add(name);
  const dir = join(web, 'node_modules', name);
  const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
  npmTodo.push(...Object.keys(manifest.dependencies ?? {}));
  const repository = typeof manifest.repository === 'string' ? manifest.repository : manifest.repository?.url;
  packages.push({
    kind: 'npm', name, version: manifest.version, licence: manifest.license ?? 'unstated',
    url: (repository ?? `https://www.npmjs.com/package/${name}`).replace(/^git\+/, '').replace(/\.git$/, ''),
    texts: textsIn(dir),
  });
}

// ---- Packages whose source carries no licence file ----
//
// Some crates keep their licence files at their repository's root, outside the published package
// (the objc2, jni and ndk families). Each is given its licence's standard text, marked as such:
// Apache-2.0 or MPL-2.0 where its expression offers one (both texts name no copyright holder, and
// other packages here carry them word for word), otherwise MIT with the crate's authors as the
// copyright line. A package with none of the three fails the build: it needs a person to look.
const common = (pattern) => {
  const counts = new Map();
  for (const p of packages) for (const id of p.texts) if (pattern.test(texts.get(id))) counts.set(id, (counts.get(id) ?? 0) + 1);
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))[0]?.[0];
};
const APACHE = common(/^\s*Apache License\s+Version 2\.0, January 2004[\s\S]*END OF TERMS AND CONDITIONS/);
const MPL = common(/^\s*Mozilla Public License Version 2\.0/);
const MIT_BODY = readFileSync(join(root, 'LICENSE'), 'utf8').replace(/\r\n/g, '\n').split('\n').slice(3).join('\n').trim();
for (const p of packages.filter((each) => !each.texts.length)) {
  const offered = p.licence.split(/\s+OR\s+|\//).map((id) => id.trim());
  const authors = byId.get([...reached].find((id) => byId.get(id).name === p.name && byId.get(id).version === p.version))?.authors ?? [];
  let id;
  if (offered.includes('Apache-2.0') && APACHE) id = APACHE;
  else if (offered.includes('MPL-2.0') && MPL) id = MPL;
  else if (offered.includes('MIT')) {
    const holder = authors.length ? authors.map((a) => a.replace(/\s*<[^>]*>/, '')).join(', ') : `the ${p.name} authors`;
    const text = `MIT License\n\nCopyright (c) ${holder}\n\n${MIT_BODY}`;
    id = createHash('sha256').update(text).digest('hex').slice(0, 16);
    texts.set(id, text);
  } else {
    throw new Error(`notices: ${p.name} ${p.version} carries no licence file and its licence (${p.licence}) is not Apache-2.0, MPL-2.0 or MIT`);
  }
  p.texts = [id];
  p.standard = true;
}

packages.sort((a, b) => a.name.localeCompare(b.name, 'en') || a.version.localeCompare(b.version, 'en') || a.kind.localeCompare(b.kind));
// Prosody's own licence, and the CMU dictionary's (vendored in prosody-cmudict, so in no package
// list): the dialog shows them by name.
const own = (path) => {
  const text = readFileSync(join(root, path), 'utf8').replace(/\r\n/g, '\n').trim();
  const id = createHash('sha256').update(text).digest('hex').slice(0, 16);
  texts.set(id, text);
  return id;
};
const ours = { prosody: own('LICENSE'), cmudict: own('crates/prosody-cmudict/data/LICENSE') };

const sortedTexts = Object.fromEntries([...texts.entries()].sort(([a], [b]) => a.localeCompare(b)));
writeFileSync(join(out, 'notices.json'), JSON.stringify({ own: ours, packages, texts: sortedTexts }));
const standard = packages.filter((p) => p.standard).length;
console.log(`notices: ${packages.length} packages (${packages.filter((p) => p.kind === 'crate').length} crates, `
  + `${packages.filter((p) => p.kind === 'npm').length} npm), ${texts.size} licence texts, ${standard} given their licence's standard text, in ${out}/notices.json`);
