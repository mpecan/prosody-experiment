// Where a request belongs (deploy/worker.js), apart from the worker so it can be checked on its own
// (deploy/check-worker.mjs): the Workers runtime lets a worker module export only its handlers.

export const CANONICAL = 'prosody.rocks';
export const HSTS = 'max-age=31536000';

// Where `url` belongs, if not where it is: the canonical host over https, or null.
export function redirectFor(url, canonical = CANONICAL) {
  const at = new URL(url);
  const ours = at.hostname === canonical || at.hostname === `www.${canonical}`;
  if (!ours) return null;
  if (at.protocol === 'https:' && at.hostname === canonical) return null;
  at.protocol = 'https:';
  at.hostname = canonical;
  at.port = '';
  return at.toString();
}
