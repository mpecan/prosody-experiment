import { afterEach, describe, expect, it, vi } from 'vitest';

import { ask, compilerThreadSource } from '../src/compiler';
import { useFixture } from './site.fixture';

afterEach(() => vi.unstubAllGlobals());

describe('asking the compiler', () => {
  it('posts to the server beside the page when served', async () => {
    const site = useFixture();
    site.transport = 'server';
    const fetched = vi.fn(async () => new Response('// compiled'));
    vi.stubGlobal('fetch', fetched);
    const answer = await ask('/pattern', 'scale=minor', 'a poem');
    expect(await answer.text()).toBe('// compiled');
    const [url, init] = fetched.mock.calls[0] as unknown as [URL, RequestInit];
    expect(url.href).toBe(new URL('/pattern?scale=minor', document.baseURI).href);
    expect(init).toEqual({ method: 'POST', body: 'a poem' });
    site.transport = 'wasm';
  });

  it('hands a worker the bindings\' absolute address, since a blob resolves nothing relative', () => {
    const source = compilerThreadSource('https://prosody.rocks/prosody.js');
    expect(source).toContain('import("https://prosody.rocks/prosody.js")');
    expect(source).toContain('answer.free();');
  });
});
