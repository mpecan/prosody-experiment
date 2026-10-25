// The Mixer: every level in one place — the band's four layers and the voice reading over them,
// side by side — and the effects on the whole mix.
import { turnControl } from '../../actions';
import { setLevel } from '../../reading';
import { controls, duck, readingSettings } from '../../store';
import { Fader, Knob, Switch } from '../desk/parts';

const value = (id: string) => controls.value[id] ?? '';
const turn = (id: string) => (next: string) => turnControl(id, next);

function Level({ id, name, colour, title }: { id: string; name: string; colour: string; title?: string }) {
  return <Fader id={id} label={name + ' level'} engraved={name} value={value(id)} min={0.1} max={2} step={0.01} colour={colour} title={title} onInput={turn(id)} />;
}

function Effect({ id, label, min, max, step, colour, title }:
  { id: string; label: string; min: number; max: number; step: number; colour: string; title?: string }) {
  return <Knob id={id} label={label} value={value(id)} min={min} max={max} step={step} colour={colour} title={title} onInput={turn(id)} />;
}

export function MixerFolder() {
  const level = readingSettings.value.level;
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Mixer</h2>
        <span class="hint">The band and the voice, side by side.</span>
      </div>
      <div class="module mixer" aria-label="Levels">
        <Level id="drums_gain" name="Drums" colour="var(--pulse)" />
        <Level id="bass_gain" name="Bass" colour="var(--bass)" />
        <Level id="melody_gain" name="Melody" colour="var(--rhyme)" />
        <Level id="chord_gain" name="Chords" colour="var(--chords)" title="The chord lane’s level, shared between a chord’s tones; heard only with the harmony mapping on" />
        <div class="channel voice">
          <Fader id="readingLevel" label="Voice level" engraved="Voice" value={level} min={0} max={2} step={0.05} colour="var(--ornament)"
            shown={Number(level).toFixed(2)} title="The voice's level into the master, against the music" onInput={(next) => setLevel(Number(next))} />
        </div>
      </div>
      <div class="module duck">
        <Switch id="duck" checked={duck.value} onChange={(on) => { duck.value = on; }}>Duck the band while a stanza is read</Switch>
      </div>
      <div class="module fx" aria-label="Effects">
        <span class="label fx-title">Effects</span>
        <Effect id="reverb" label="Reverb" min={0} max={1} step={0.01} colour="var(--pulse)" />
        <Effect id="room_size" label="Room" min={0.1} max={10} step={0.1} colour="var(--ornament)" />
        <Effect id="delay" label="Echo" min={0} max={1} step={0.01} colour="var(--rhyme)" />
        <Effect id="high_cut" label="High cut" min={1000} max={20000} step={100} colour="var(--chords)" />
        <Effect id="drive" label="Drive" min={0} max={1} step={0.01} colour="var(--bass)" />
        <Effect id="compress" label="Compress" min={0} max={1} step={0.01} colour="var(--ornament)" />
        <Effect id="density_level" label="Density" min={0} max={1} step={0.01} colour="var(--pulse)"
          title="Levels follow density: each bar’s gains divided by n to this power, n = the lines sounding in it: 0 fixed levels, 0.5 by √n, 1 by n" />
      </div>
    </div>
  );
}
