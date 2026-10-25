// The Reading folder: a voice reading the poem over the music — who reads, in what language and
// voice, at what pace and level, and whether the music ducks under it.
import { chooseEngine, setLanguage, setLevel, setPace, setVoice } from '../../reading';
import { duck, language, readingMenus, readingSettings, type EngineName, type Pace } from '../../store';
import { Knob, Selector, Switch } from '../desk/parts';

export function ReadingFolder() {
  const menus = readingMenus.value;
  const settings = readingSettings.value;
  return (
    <>
      <div class="desk-head"><h2>Reading</h2><span class="engrave">a voice reading the poem over the music</span></div>
      <div class="palette reading">
        <Selector id="engine" label="Engine" value={settings.engine} options={menus.engines}
          onChange={(name) => chooseEngine(name as EngineName)}
          title="Who reads: your browser's own voice, a neural voice that runs in this page — downloaded once you choose it, then kept by the browser — or readings recorded ahead of time for the example poems, a small file a line" />
        <Selector id="language" label="Language" value={language.value} options={menus.languages} onChange={setLanguage}
          title="The language the poem is read in: an example poem sets its own; each engine lists only its voices for it" />
        <label class="selector">
          <span class="engrave">Voice</span>
          <select id="voice" value={menus.voice} disabled={menus.voiceDisabled} onChange={(event) => setVoice(event.currentTarget.value)}>
            {menus.voices.map((v) => <option key={v.value} value={v.value}>{v.label}</option>)}
          </select>
        </label>
        <Selector id="pace" label="Pace" value={settings.pace} onChange={(pace) => setPace(pace as Pace)}
          options={[{ value: 'natural', label: 'natural' }, { value: 'fitted', label: 'fitted to the bar' }]}
          title="natural: each line at the voice's own speed, a little faster or slower line by line, with pauses where the punctuation asks; fitted: each line squeezed or stretched to fill its bar" />
        <Knob id="readingLevel" label="Voice level" value={settings.level} min={0} max={2} step={0.05} colour="var(--rhyme)"
          shown={Number(settings.level).toFixed(2)} onInput={(level) => setLevel(Number(level))}
          title="The voice's level into the master, against the music" />
        <Switch id="duck" checked={duck.value} onChange={(on) => { duck.value = on; }}>Duck the music while a stanza is read</Switch>
        <p>
          Whether and when lines are read is the <code>reading</code> mapping above:
          {' '}<code>breath</code> (the default) reads each stanza through, with a silent bar before the
          next stanza and never before the music has begun it;
          {' '}<code>arrives</code> speaks each line once, where it is first heard;
          {' '}<code>stanza</code> reads a stanza through from where it is first heard;
          {' '}<code>off</code> reads nothing. Loops, lanes and the outro are never read.
          At the <em>natural</em> pace a line follows the one before after a pause from how it
          ends — a quarter of a second after a line that runs on, more after a comma, more after a
          full stop, a second and a half at a stanza's end; <em>fitted</em> fits each line to its bar.
          Strudel's clock places every line; a neural voice is rendered ahead and started exactly on
          it, the browser's voice as near as its speech allows. Kokoro and Kitten speak English;
          Piper speaks many languages, Slovenian among them. Kitten needs WebGPU.
        </p>
      </div>
    </>
  );
}
