// The Reading folder: a voice reading the poem over the music — who reads, in what language and
// voice, and at what pace. Its level and the duck are on the Mixer.
import { chooseEngine, setLanguage, setPace, setVoice } from '../../reading';
import { language, readingMenus, readingSettings, type EngineName, type Pace } from '../../store';
import { More } from '../desk/More';
import { Patch } from '../desk/Patch';
import { Segmented } from '../desk/Segmented';

export function ReadingFolder() {
  const menus = readingMenus.value;
  const settings = readingSettings.value;
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Reading</h2>
        <span class="hint">A voice reading the poem over the music.</span>
      </div>
      <div class="module reading" aria-label="The reading">
        <div class="wide" title="Who reads: your browser's own voice, a neural voice that runs in this page — downloaded once you choose it, then kept by the browser — or readings recorded ahead of time for the example poems, a small file a line">
          <Segmented id="engine" label="Who reads" value={settings.engine} options={menus.engines} wrap colour="var(--rhyme)"
            onChange={(name) => chooseEngine(name as EngineName)} />
        </div>
        <Patch id="language" label="Language" value={language.value} options={menus.languages} onChange={setLanguage} colour="var(--rhyme)"
          title="The language the poem is read in: an example poem sets its own; each engine lists only its voices for it" />
        <div title="natural: each line at the voice's own speed, a little faster or slower line by line, with pauses where the punctuation asks; fitted: each line squeezed or stretched to fill its bar">
          <Segmented id="pace" label="Pace" value={settings.pace} onChange={(pace) => setPace(pace as Pace)} colour="var(--rhyme)"
            options={[{ value: 'natural', label: 'natural' }, { value: 'fitted', label: 'fitted to the bar' }]} />
        </div>
        <div class="wide">
  <Patch id="voice" label="Voice" value={menus.voice} options={menus.voices} onChange={setVoice} colour="var(--rhyme)" disabled={menus.voiceDisabled} />
        </div>
      </div>
      <More label="How the reading works">
        <p>
          Whether and when lines are read is the <code>reading</code> mapping:
          {' '}<code>breath</code> (the default) reads each stanza through, with a silent bar before the
          next stanza and never before the music has begun it;
          {' '}<code>arrives</code> speaks each line once, where it is first heard;
          {' '}<code>stanza</code> reads a stanza through from where it is first heard;
          {' '}<code>off</code> reads nothing. Loops, lanes and the outro are never read.
        </p>
        <p>
          At the <em>natural</em> pace a line follows the one before after a pause from how it
          ends — a quarter of a second after a line that runs on, more after a comma, more after a
          full stop, a second and a half at a stanza's end; <em>fitted</em> fits each line to its bar.
          Strudel's clock places every line; a neural voice is rendered ahead and started exactly on
          it, the browser's voice as near as its speech allows. Kokoro and Kitten speak English;
          Piper speaks many languages, Slovenian among them. Kitten needs WebGPU.
        </p>
      </More>
    </div>
  );
}
