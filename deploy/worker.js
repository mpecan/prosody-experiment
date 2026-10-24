// The one piece of code that runs on Cloudflare: it keeps the site at a single secure address and
// otherwise hands every request to the static files (`assets` in wrangler.jsonc). The page itself
// does all its work in the visitor's browser.
//
// - `http://…` and `www.prosody.rocks` move, for good (301), to `https://prosody.rocks`, path and
//   query kept. www is never served from: one address, one origin for the browser's caches.
// - Every answer on https says so for a year (HSTS), so a browser that has been here once never
//   tries plain http again. Not `includeSubDomains`: it would bind every other name in the zone.
// - Any other host (wrangler dev on localhost, a preview) is served as it comes.

import { HSTS, redirectFor } from './redirect.js';

export default {
  async fetch(request, env) {
    const moved = redirectFor(request.url);
    if (moved) return Response.redirect(moved, 301);
    const response = await env.ASSETS.fetch(request);
    if (new URL(request.url).protocol !== 'https:') return response;
    const secured = new Response(response.body, response);
    secured.headers.set('Strict-Transport-Security', HSTS);
    return secured;
  },
};
