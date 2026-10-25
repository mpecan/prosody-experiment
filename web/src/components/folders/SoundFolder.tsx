// The Sound folder: how it sounds, never what happens. The key, the tempo window and the feel, and
// the instruments — each a patch display or a switch. The levels and effects are on the Mixer.
import { openFolder, pickStyle, turnControl } from '../../actions';
import { optionsOf, paletteOf, ROOTS } from '../../controls';
import { site } from '../../data';
import { nameOf } from '../../names';
import { controls, suggested } from '../../store';
import { Knob } from '../desk/parts';
import { Patch, type Choice } from '../desk/Patch';
import { Segmented } from '../desk/Segmented';

const value = (id: string) => controls.value[id] ?? '';
const turn = (id: string) => (next: string) => turnControl(id, next);
const named = (values: string[]): Choice[] => values.map((v) => ({ value: v, label: nameOf(v) }));

function ControlKnob({ id, label, min, max, step, colour, title }:
  { id: string; label: string; min: number; max: number; step: number; colour: string; title?: string }) {
  return <Knob id={id} label={label} value={value(id)} min={min} max={max} step={step} colour={colour} title={title} onInput={turn(id)} />;
}

/** A patch display for the control `id`. */
function ControlPatch({ id, label, colour, title }: { id: string; label: string; colour: string; title?: string }) {
  return <Patch id={id} label={label} value={value(id)} options={named(optionsOf(id))} onChange={turn(id)} colour={colour} title={title} />;
}

/** A switch for the control `id`, whose choices are few. */
function ControlSwitch({ id, label, colour, title }: { id: string; label: string; colour: string; title?: string }) {
  return (
    <div title={title}>
      <Segmented id={id} label={label} value={value(id)} options={named(optionsOf(id))} onChange={turn(id)} colour={colour} />
    </div>
  );
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
        <span id="hubScale" class="engrave">{nameOf(value('scale'))}</span>
      </div>
    </div>
  );
}

/** What a style's suggestions are, in words: "bass: syllables, harmony: cadence". */
const suggestions = (suggests: Record<string, string>) =>
  Object.entries(suggests).map(([mapping, variant]) => `${mapping}: ${variant}`).join(', ');

// The styles: a card each, the one the sound controls match lit — none once a control has been
// turned by hand. A style sets its palette, and on A the mappings its genre suggests.
function StylePicker() {
  const current = paletteOf(controls.value);
  const set = Object.entries(suggested.value).filter(([, change]) => change.style === current);
  return (
    <div class="module styles" aria-label="Style">
      <span class="label">Style</span>
      <div class="style-cards" id="styles" role="group" aria-label="Style">
        {site.styles.map((style) => (
          <button key={style.name} type="button" class="style-card" data-style={style.name} aria-pressed={current === style.name}
            onClick={() => pickStyle(style.name)}>
            <span class="style-name">{nameOf(style.name)}</span>
            <span class="style-what">{style.what}</span>
            {Object.keys(style.suggests).length > 0 && <span class="style-suggests">suggests {suggestions(style.suggests)}</span>}
          </button>
        ))}
      </div>
      {current === 'custom' && <p class="hint">Custom: a control has been turned since the last style.</p>}
      {set.length > 0 && (
        <p class="hint style-set">
          {nameOf(current)} also set A's {set.map(([mapping, { to }]) => `${mapping} to ${to}`).join(' and ')}.{' '}
          <button type="button" class="link" onClick={() => openFolder('mappings')}>See Mappings</button>
        </p>
      )}
    </div>
  );
}

export function SoundFolder() {
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Sound</h2>
        <span class="hint">How it sounds, never what happens.</span>
      </div>
      <StylePicker />
      <div class="module key" aria-label="Key">
        <KeyCircle />
        <ControlPatch id="scale" label="Scale" colour="var(--pulse)" />
      </div>
      <div class="module tempo" aria-label="Tempo and feel">
        <span class="label">Tempo window, in beats a minute</span>
        <div class="leds">
          <input id="low" class="led" type="number" min="1" max="400" value={value('low')} aria-label="Slowest tempo, beats per minute"
            onInput={(event) => turnControl('low', event.currentTarget.value)} />
          <span class="led-dash">–</span>
          <input id="high" class="led" type="number" min="1" max="400" value={value('high')} aria-label="Fastest tempo, beats per minute"
            onInput={(event) => turnControl('high', event.currentTarget.value)} />
        </div>
        <span class="hint">The poem picks within it, by how densely its syllables pack.</span>
        <div class="feel">
          <ControlKnob id="swing" label="Swing" min={0} max={1} step={0.01} colour="var(--ornament)" />
          <ControlKnob id="humanise" label="Humanise" min={0} max={1} step={0.01} colour="var(--rhyme)"
            title="Each syllable strays a little late and soft, drawn from its own line, so the same poem always strays the same way" />
        </div>
      </div>
      <div class="module voices" aria-label="Instruments">
        <ControlPatch id="bank" label="Drum machine" colour="var(--pulse)" />
        <ControlSwitch id="kit" label="Kit" colour="var(--pulse)" title="Which drum carries each layer: kick and hat, or ride and snare" />
        <ControlSwitch id="ride_fallback" label="Ride stand-in" colour="var(--pulse)"
          title="What keeps time under the ride kit on a bank with no ride (tr808, tr707): its open hat or its crash" />
        <ControlPatch id="bass" label="Bass" colour="var(--bass)" />
        <ControlPatch id="melody" label="Melody" colour="var(--rhyme)" />
        <ControlPatch id="chord_sound" label="Chords" colour="var(--chords)"
          title="The instrument of the chord lane, which plays only when a side’s harmony mapping is not off" />
        <ControlSwitch id="voicing" label="Rhyme voicing" colour="var(--rhyme)"
          title="Each rhyme word as one note, or as the seventh chord on its degree. With a side’s harmony mapping on, rhyme words are single notes over the chords and this voices only the chord lane" />
        <div class="octaves">
          <ControlKnob id="bass_octave" label="Bass oct" min={1} max={3} step={1} colour="var(--bass)" title="The octave the bass plays in: 2 is MIDI 36" />
          <ControlKnob id="melody_octave" label="Melody oct" min={3} max={5} step={1} colour="var(--rhyme)" title="The octave the rhyme melody plays in: 4 is MIDI 60, middle C" />
        </div>
      </div>
      <p class="hint desk-note">Everything here is colour: the same poem plays the same things, differently.</p>
    </div>
  );
}
