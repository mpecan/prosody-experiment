// The top bar: the name, that it is an experiment, the theme, about and the source; then the poem
// picker, Play, Stop and which setting is heard.
import { choosePreset, nextTheme, openShare, showRoll } from '../actions';
import { site } from '../data';
import { hear } from '../piece';
import { play, stop } from '../strudel';
import { beat, hearing, playing, preset, rollShown, theme, welcomeOpen } from '../store';
import { Patch } from './desk/Patch';

const THEME_LABELS = { auto: '◐ auto', light: '☀ light', dark: '☾ dark' };

// The transport's icons. With their words beside them on a wide screen; alone on a phone, where the
// button's name (its aria-label) still says what it does.
const PLAY = <svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><path d="M4.5 2.6v10.8L13 8z" /></svg>;
const STOP = <svg class="icon" viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="3.5" width="9" height="9" rx="1" /></svg>;
const SHARE = (
  <svg class="icon" viewBox="0 0 16 16" aria-hidden="true">
    <circle cx="12" cy="3.5" r="2" /><circle cx="4" cy="8" r="2" /><circle cx="12" cy="12.5" r="2" />
    <path class="line" d="M10.2 4.5 5.8 7M5.8 9l4.4 2.5" />
  </svg>
);

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
          aria-label={playing.value ? 'Playing' : 'Play'} onClick={() => void play()}>
          {PLAY}<span class="button-text">{playing.value ? 'Playing' : 'Play'}</span>
        </button>
        <button id="stop" disabled={!playing.value} aria-label="Stop" onClick={stop}>{STOP}<span class="button-text">Stop</span></button>
        <button id="share" type="button" aria-label="Share" title="A link to this poem and how it sounds, to send to anyone"
          onClick={() => void openShare()}>
          {SHARE}<span class="button-text">Share</span>
        </button>
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
