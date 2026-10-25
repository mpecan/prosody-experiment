// Serves the static site (`scripts/build-site.sh`, target/site by default) for the end-to-end
// tests, with the types a browser insists on: a module worker and an import map's target must be
// JavaScript, the compiler `application/wasm`.
//
//   node e2e/serve.mjs [DIR] [PORT]
import { createReadStream, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';

const root = resolve(process.argv[2] ?? join(import.meta.dirname, '../../target/site'));
const port = Number(process.argv[3] ?? 4802);
const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8', '.json': 'application/json; charset=utf-8', '.wasm': 'application/wasm', '.mp3': 'audio/mpeg',
};

createServer((request, response) => {
  const path = decodeURIComponent(new URL(request.url ?? '/', 'http://x').pathname);
  const file = normalize(join(root, path === '/' ? 'index.html' : path));
  let found = false;
  try { found = file.startsWith(root) && statSync(file).isFile(); } catch { found = false; }
  if (!found) { response.writeHead(404).end('no such thing here\n'); return; }
  response.writeHead(200, { 'Content-Type': TYPES[extname(file)] ?? 'application/octet-stream' });
  createReadStream(file).pipe(response);
}).listen(port, '127.0.0.1', () => console.log(`serving ${root} on http://127.0.0.1:${port}`));
