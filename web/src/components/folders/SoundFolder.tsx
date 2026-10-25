// The Sound folder: how it sounds, never what happens. Modules on a grid that reflows on a phone —
// the key, the tempo window, the instruments, the levels and the mix.
import { pickPalette, turnControl } from '../../actions';
import { optionsOf, paletteOf, ROOTS } from '../../controls';
import { site } from '../../data';
import { controls } from '../../store';
import { Fader, Knob, plain, Selector } from '../desk/parts';

const value = (id: string) => controls.value[id] ?? '';
const turn = (id: string) => (next: string) => turnControl(id, next);

/** A knob for the control `id`. */
function ControlKnob({ id, label, min, max, step, colour, title }:
  { id: string; label: string; min: number; max: number; step: number; colour: string; title?: string }) {
  return <Knob id={id} label={label} value={value(id)} min={min} max={max} step={step} colour={colour} title={title} onInput={turn(id)} />;
}

/** A fader for the level `id`. */
function Level({ id, name, colour, title }: { id: string; name: string; colour: string; title?: string }) {
  return <Fader id={id} label={name + ' level'} engraved={name} value={value(id)} min={0.1} max={2} step={0.01} colour={colour} title={title} onInput={turn(id)} />;
}

/** A menu for the control `id`. */
function Menu({ id, label, title }: { id: string; label: string; title?: string }) {
  return <Selector id={id} label={label} title={title} value={value(id)} options={plain(optionsOf(id))} onChange={turn(id)} />;
}

// The key: twelve notes on a circle. A note is a button that sets the root; the root is lit and
// the scale's other notes tinted, from the scale's own steps.
function KeyCircle() {
  const root = ROOTS.indexOf(value('root'));
  const steps = site.scaleSteps[value('scale')] ?? [];
  return (
    <div class="circle" id="noteCircle" role="group" aria-label="Root note">
      {ROOTS.map((name, i) => {
        const angle = (i / 12) * 2 * Math.PI - Math.PI / 2;
        return (
          <button key={name} type="button" aria-label={'Root ' + name.replace('#', ' sharp')} aria-pressed={i === root}
            class={steps.includes((i - root + 12) % 12) ? 'in' : undefined}
            style={{
              left: `calc(50% + ${(Math.cos(angle) * 5.3).toFixed(3)}rem)`,
              top: `calc(50% + ${(Math.sin(angle) * 5.3).toFixed(3)}rem)`,
            }}
            onClick={() => turnControl('root', name)}>
            {name.replace('#', '♯')}
          </button>
        );
      })}
      <div class="hub">
        <span id="hubRoot">{value('root').replace('#', '♯')}</span>
        <span id="hubScale" class="engrave">{value('scale').replace(/_/g, ' ')}</span>
      </div>
    </div>
  );
}

export function SoundFolder() {
  // The palette menu names the palette the sound controls currently match, or "custom".
  const palette = paletteOf(controls.value);
  const palettes = Object.keys(site.palettes).map((name) => ({ value: name, label: name }))
    .concat([{ value: 'custom', label: 'custom', disabled: true } as { value: string; label: string; disabled?: boolean }]);
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Sound</h2>
        <span class="engrave">how it sounds · never what happens</span>
      </div>
      <div class="module key" aria-label="Key">
        <KeyCircle />
        <Menu id="scale" label="Scale" />
      </div>
      <div class="module tempo" aria-label="Tempo">
        <span class="engrave">Tempo window · bpm</span>
        <div class="leds">
          <input id="low" class="led" type="number" min="1" max="400" value={value('low')} aria-label="Slowest tempo, beats per minute"
            onInput={(event) => turnControl('low', event.currentTarget.value)} />
          <span class="led-dash">–</span>
          <input id="high" class="led" type="number" min="1" max="400" value={value('high')} aria-label="Fastest tempo, beats per minute"
            onInput={(event) => turnControl('high', event.currentTarget.value)} />
        </div>
        <span class="hint">the poem picks within it, by how densely its syllables pack</span>
        <ControlKnob id="swing" label="Swing" min={0} max={1} step={0.01} colour="var(--ornament)" />
      </div>
      <div class="module voices" aria-label="Instruments">
        <Selector id="palette" label="Sound" value={palette} options={palettes} onChange={pickPalette}
          title="A named palette: instruments and mix. Scale, root, tempo and bank stay as they are — except jazz, which also sets its scale, swing and bank." />
        <Menu id="bank" label="Bank" />
        <Menu id="kit" label="Kit" title="Which drum carries each layer: kick and hat, or ride and snare" />
        <Menu id="ride_fallback" label="Ride stand-in" title="What keeps time under the ride kit on a bank with no ride (tr808, tr707): its open hat or its crash" />
        <Menu id="bass" label="Bass" />
        <Menu id="melody" label="Melody" />
        <Menu id="chord_sound" label="Chords" title="The instrument of the chord lane, which plays only when a side’s harmony mapping is not off" />
        <Menu id="voicing" label="Voicing" title="Each rhyme word as one note, or as the seventh chord on its degree. With a side’s harmony mapping on, rhyme words are single notes over the chords and this voices only the chord lane" />
        <div class="octaves">
          <ControlKnob id="bass_octave" label="Bass oct" min={1} max={3} step={1} colour="var(--bass)" title="The octave the bass plays in: 2 is MIDI 36" />
          <ControlKnob id="melody_octave" label="Melody oct" min={3} max={5} step={1} colour="var(--rhyme)" title="The octave the rhyme melody plays in: 4 is MIDI 60, middle C" />
        </div>
      </div>
      <div class="module mixer" aria-label="Levels">
        <Level id="drums_gain" name="Drums" colour="var(--pulse)" />
        <Level id="bass_gain" name="Bass" colour="var(--bass)" />
        <Level id="melody_gain" name="Melody" colour="var(--rhyme)" />
        <Level id="chord_gain" name="Chords" colour="var(--chords)" title="The chord lane’s level, shared between a chord’s tones; heard only with the harmony mapping on" />
      </div>
      <div class="module fx" aria-label="Mix">
        <span class="engrave fx-title">Mix</span>
        <ControlKnob id="reverb" label="Reverb" min={0} max={1} step={0.01} colour="var(--pulse)" />
        <ControlKnob id="room_size" label="Room" min={0.1} max={10} step={0.1} colour="var(--ornament)" />
        <ControlKnob id="delay" label="Echo" min={0} max={1} step={0.01} colour="var(--rhyme)" />
        <ControlKnob id="high_cut" label="High cut" min={1000} max={20000} step={100} colour="var(--chords)" />
        <ControlKnob id="drive" label="Drive" min={0} max={1} step={0.01} colour="var(--bass)" />
        <ControlKnob id="compress" label="Compress" min={0} max={1} step={0.01} colour="var(--ornament)" />
        <ControlKnob id="humanise" label="Humanise" min={0} max={1} step={0.01} colour="var(--rhyme)" />
        <ControlKnob id="density_level" label="Density" min={0} max={1} step={0.01} colour="var(--pulse)"
          title="Levels follow density: each bar’s gains divided by n to this power, n = the lines sounding in it: 0 fixed levels, 0.5 by √n, 1 by n" />
      </div>
      <p class="hint desk-note">Everything here is colour: the same poem plays the same things,
        differently. Humanise strays each syllable a little late and soft, drawn from its own line,
        so the same poem always strays the same way.</p>
    </div>
  );
}
