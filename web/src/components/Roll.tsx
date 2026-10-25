// The roll: the notes playing now, scrolling past a playhead, each row one sound coloured by its
// layer, faint lines marking the bars. Shown or hidden from the top bar, whichever folder is open;
// Strudel's own piano roll draws it (watch.ts).
import { useEffect, useRef } from 'preact/hooks';

import { setRollBars } from '../actions';
import { rollBars, rollShown } from '../store';
import { watch } from '../watch';
import { Segmented } from './desk/Segmented';

const KEY: [string, string][] = [
  ['var(--pulse)', 'pulse'], ['var(--ornament)', 'ornament'], ['var(--bass)', 'bass'],
  ['var(--rhyme)', 'rhyme note'], ['var(--chords)', 'chords'],
];

export function Roll() {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    watch.canvas = canvas.current;
    return () => { watch.canvas = null; };
  }, []);
  return (
    <section id="rollband" class="rollband" aria-label="The roll" hidden={!rollShown.value}>
      <div class="rollbar">
        <Segmented label="Bars shown" value={rollBars.value} onChange={setRollBars} colour="var(--pulse)"
          options={['2', '4', '8', '16'].map((n) => ({ value: n, label: n }))} />
        <div class="key" aria-hidden="true">
          {KEY.map(([colour, name]) => <span key={name} style={{ '--c': colour }}><i />{name}</span>)}
          <span class="echo" title="dimmer in the poem: a line looping, not new"><i />looping</span>
          <span class="voice" title="behind the words: the line being read aloud"><i />read aloud</span>
        </div>
      </div>
      <canvas id="roll" ref={canvas} aria-label="The notes playing now, scrolling past a playhead: each row one sound, coloured by its layer; faint lines mark the bars" />
    </section>
  );
}
