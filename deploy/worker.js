// The one piece of code that runs on Cloudflare: it keeps the site at a single secure address and
// otherwise hands every request to the static files (`assets` in wrangler.jsonc). The page itself
// does all its work in the visitor's browser.
//
// - `http://…` and `www.prosody.rocks` move, for good (301), to `https://prosody.rocks`, path and
//   query kept. www is never served from: one address, one origin for the browser's caches.
// - Every answer on https says so for a year (HSTS), so a browser that has been here once never
//   tries plain http again. Not `includeSubDomains`: it would bind every other name in the zone.
// - Any other host (wrangler dev on localhost, a preview) is served as it comes.
// - A shared poem's link (`/?s=…`) gets the poem's own title and description in the page's head, for
//   its link preview (deploy/preview.js); the link is read as it passes and nothing is kept.

import { previewOf, sharedIn, withPreview } from './preview.js';
import { HSTS, redirectFor } from './redirect.js';

export default {
  async fetch(request, env) {
    const canonical = env?.CANONICAL_HOST || (new URL(request.url).hostname.startsWith('staging.') ? 'staging.prosody.rocks' : 'prosody.rocks');
    const moved = redirectFor(request.url, canonical);
    if (moved) return Response.redirect(moved, 301);
    let response = await env.ASSETS.fetch(request);
    const url = new URL(request.url);
    if (request.method === 'GET' && (url.pathname === '/' || url.pathname === '/index.html') && url.searchParams.has('s')
      && response.ok && (response.headers.get('content-type') ?? '').startsWith('text/html')) {
      const shared = await sharedIn(request.url);
      if (shared) {
        const page = withPreview(await response.text(), previewOf(shared, canonical), request.url);
        const headers = new Headers(response.headers);
        headers.delete('content-length');
        headers.delete('etag');
        response = new Response(page, { status: response.status, headers });
      }
    }
    if (url.protocol !== 'https:') return response;
    const secured = new Response(response.body, response);
    secured.headers.set('Strict-Transport-Security', HSTS);
    return secured;
  },
};
