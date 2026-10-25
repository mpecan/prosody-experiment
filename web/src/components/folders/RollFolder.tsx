// The Roll folder: the notes playing now, scrolling past a playhead, each row one sound coloured by
// its layer, faint lines marking the bars. Strudel's own piano roll draws it (watch.ts).
import { useEffect, useRef } from 'preact/hooks';

import { setRollBars } from '../../actions';
import { rollBars } from '../../store';
import { watch } from '../../watch';
import { Selector } from '../desk/parts';

export function RollFolder() {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    watch.canvas = canvas.current;
    return () => { watch.canvas = null; };
  }, []);
  return (
    <>
      <div class="rollbar">
        <Selector id="rollBars" label="Bars shown" inline value={rollBars.value} onChange={setRollBars}
          options={['2', '4', '8', '16'].map((n) => ({ value: n, label: n }))} />
        <div class="key" aria-hidden="true">
          <span style="--c:#c9a6ff"><i />pulse</span>
          <span style="--c:#6fd6c5"><i />ornament</span>
          <span style="--c:#ffb86b"><i />bass</span>
          <span style="--c:#ff7aa8"><i />rhyme note</span>
          <span style="--c:#8fb3ff"><i />chords</span>
          <span class="echo"><i />dimmer in the poem: a line looping, not new</span>
          <span class="voice"><i />behind the words: the line being read aloud</span>
        </div>
      </div>
      <canvas id="roll" ref={canvas} aria-label="The notes playing now, scrolling past a playhead: each row one sound, coloured by its layer; faint lines mark the bars" />
    </>
  );
}
