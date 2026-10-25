// The poem you write is the poem you watch: the text box lies exactly over the verse that lights
// up, its own letters transparent, so typing, the caret and selecting work as in any text box
// while the colour underneath shows what is sounding. The verse is drawn by watch.ts.
import { useEffect, useRef } from 'preact/hooks';

import { write } from '../actions';
import { playing, poem } from '../store';
import { buildLit, watch } from '../watch';

export function Editor() {
  const lit = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    watch.litBox = lit.current;
    watch.poemBox = box.current;
    buildLit();
    return () => { watch.litBox = null; watch.poemBox = null; };
  }, []);
  return (
    <main>
      <div id="editor" class={playing.value ? 'playing' : undefined}>
        <div id="lit" ref={lit} aria-hidden="true" />
        <textarea id="poem" ref={box} spellcheck={false} aria-label="The poem"
          placeholder="Write a poem, or pick one of the examples."
          value={poem.value}
          onInput={(event) => write(event.currentTarget.value)}
          onScroll={(event) => { if (lit.current) lit.current.scrollTop = event.currentTarget.scrollTop; }} />
      </div>
    </main>
  );
}
