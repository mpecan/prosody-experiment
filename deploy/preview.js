// A shared poem's link preview (web/src/share.ts makes the links). A link carries its creation in
// `?s=`: the poem and its settings, compressed JSON in base64url. Where the link is pasted — a chat,
// a post — the preview should be the poem's, not the site's, and the page cannot say so itself,
// since crawlers do not run it. So the worker reads the link as it passes, puts the poem's first
// lines in the page's title and description, and forgets it: nothing is stored.
//
// Apart from the worker so deploy/check-worker.mjs and the page's tests can check it on their own.

/** The most a link may unpack to, as the page allows (share.ts `MOST`). */
export const MOST = 64 * 1024;

function bytesOf(code) {
  const base64 = code.replace(/-/g, '+').replace(/_/g, '/');
  return Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
}

async function unpack(bytes) {
  const reader = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('deflate-raw')).getReader();
  const parts = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.length;
    if (size > MOST) { await reader.cancel(); throw new Error('too long'); }
    parts.push(value);
  }
  const out = new Uint8Array(size);
  let at = 0;
  for (const part of parts) { out.set(part, at); at += part.length; }
  return new TextDecoder().decode(out);
}

/** The poem and its style a link carries, or null for a link that carries none. */
export async function sharedIn(url) {
  const code = new URL(url).searchParams.get('s');
  if (!code || code.length > MOST) return null;
  try {
    const value = JSON.parse(await unpack(bytesOf(code)));
    if (value?.v !== 1 || typeof value.poem !== 'string' || !value.poem.trim()) return null;
    return { poem: value.poem, style: typeof value.style === 'string' ? value.style : null };
  } catch {
    return null;
  }
}

const clip = (text, most) => (text.length <= most ? text : text.slice(0, most - 1).trimEnd() + '…');

/** What the preview says: the poem's first line as its title, the next few as its description. */
export function previewOf({ poem, style }, canonical = 'prosody.rocks') {
  const lines = poem.split('\n').map((line) => line.trim()).filter(Boolean);
  const title = clip(lines[0] ?? 'A poem', 80);
  const more = clip(lines.slice(1, 4).join(' / '), 160);
  const what = `A poem played as music${style && style !== 'default' ? `, ${style}` : ''}, on ${canonical}.`;
  return { title: `${title} — prosody`, description: more ? `${more} — ${what}` : what };
}

/** `text` safe inside an HTML attribute or element. */
export function escape(text) {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

/** The page's HTML with the preview's title, description and address in place of the site's. */
export function withPreview(html, { title, description }, url) {
  const content = (tag, value) => (page) =>
    page.replace(new RegExp(`(<meta ${tag} content=")[^"]*(")`), `$1${escape(value)}$2`);
  return [
    (page) => page.replace(/<title>[^<]*<\/title>/, `<title>${escape(title)}</title>`),
    content('name="description"', description),
    content('property="og:title"', title),
    content('property="og:description"', description),
    content('property="og:url"', url),
  ].reduce((page, step) => step(page), html);
}
