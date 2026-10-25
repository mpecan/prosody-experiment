// The top bar: the name, that it is an experiment, the theme, about and the source; then the poem
// picker, Play, Stop and which setting is heard.
import { choosePreset, nextTheme, openShare, showRoll } from '../actions';
import { site } from '../data';
import { hear } from '../piece';
import { play, stop } from '../strudel';
import { beat, hearing, playing, preset, rollShown, theme, welcomeOpen } from '../store';
import { Patch } from './desk/Patch';

const THEME_LABELS = { auto: '◐ auto', light: '☀ light', dark: '☾ dark' };

export function TopBar() {
  const beating = beat.value;
  return (
    <header class="top">
      <div class="brand">
        <h1 aria-label="prosody"><span>pro</span><span>so</span><span>dy</span></h1>
        <span class="tag">an experiment</span>
      </div>
      <nav class="links" aria-label="About this site">
        <button id="theme" type="button" title="Light or dark: as your system says, or always one"
          aria-label={`Theme: ${theme.value}. Change it`} onClick={nextTheme}>{THEME_LABELS[theme.value]}</button>
        <button id="about" type="button" onClick={() => { welcomeOpen.value = true; }}>about</button>
        <a href="https://github.com/mpecan/prosody-experiment" rel="noopener" target="_blank">source</a>
      </nav>
      <div class="controls">
        <div class="poems">
          <Patch id="preset" label="Example poems" hideLabel value={preset.value} onChange={choosePreset} colour="var(--ornament)"
            title="Load one of the example poems, with its palette"
            options={[{ value: '', label: 'Your own poem', disabled: true }, ...site.presets.map((p) => ({ value: p.name, label: p.label }))]} />
        </div>
        {/* While it plays, the button breathes on the beat. */}
        <button id="play" class={beating ? 'primary beating' : 'primary'} style={beating ? { '--beat': beating.toFixed(3) + 's' } : undefined}
          onClick={() => void play()}>{playing.value ? '▶ Playing' : '▶ Play'}</button>
        <button id="stop" disabled={!playing.value} onClick={stop}>■ Stop</button>
        <button id="share" type="button" title="A link to this poem and how it sounds, to send to anyone" onClick={() => void openShare()}>Share</button>
        <button id="rollSwitch" type="button" class="roll-switch" aria-pressed={rollShown.value}
          title="Show the roll under the poem, whichever folder is open" onClick={() => showRoll(!rollShown.value)}>
          <span class="lever" aria-hidden="true" />Roll
        </button>
        <span class="ab" title="Which setting is playing and shown — switch while it plays to compare">
          <span>hearing</span>
          <button id="hearA" aria-pressed={hearing.value === 'A'} onClick={() => hear('A')}>A</button>
          <button id="hearB" aria-pressed={hearing.value === 'B'} onClick={() => hear('B')}>B</button>
        </span>
      </div>
    </header>
  );
}
