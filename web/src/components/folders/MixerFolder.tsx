// The Mixer: every level in one place — the band's four layers and the voice reading over them,
// side by side, each with its mute and solo — and the effects on the whole mix. Mute and solo are
// heard at once and change nothing in the pattern (mix.ts); the meters move with what sounds.
import type { ComponentChildren } from 'preact';

import { toggleMute, toggleSolo, turnControl } from '../../actions';
import { audible } from '../../mix';
import { setLevel } from '../../reading';
import { controls, duck, muted, readingSettings, soloed, type Channel } from '../../store';
import { Fader, Knob, Switch } from '../desk/parts';

const value = (id: string) => controls.value[id] ?? '';
const turn = (id: string) => (next: string) => turnControl(id, next);

/** A channel strip: its fader, then mute and solo. Dimmed while it is not heard. */
function Strip({ channel, name, children, voice = false }: { channel: Channel; name: string; children: ComponentChildren; voice?: boolean }) {
  const isMuted = !!muted.value[channel];
  const isSoloed = !!soloed.value[channel];
  const heard = audible(channel, muted.value, soloed.value);
  const classes = ['channel', voice ? 'voice' : '', heard ? '' : 'quiet'].filter(Boolean).join(' ');
  return (
    <div class={classes} data-channel={channel}>
      {children}
      <div class="ms">
        <button type="button" class="mute" aria-pressed={isMuted} aria-label={`Mute ${name.toLowerCase()}`} title={`Mute ${name.toLowerCase()}`}
          onClick={() => toggleMute(channel)}>M</button>
        <button type="button" class="solo" aria-pressed={isSoloed} aria-label={`Hear only ${name.toLowerCase()}`} title={`Solo: hear only ${name.toLowerCase()} (and any other soloed)`}
          onClick={() => toggleSolo(channel)}>S</button>
      </div>
    </div>
  );
}

function Level({ id, channel, name, colour, title }: { id: string; channel: Channel; name: string; colour: string; title?: string }) {
  return (
    <Strip channel={channel} name={name}>
      <Fader id={id} channel={channel} label={name + ' level'} engraved={name} value={value(id)} min={0.1} max={2} step={0.01} colour={colour} title={title} onInput={turn(id)} />
    </Strip>
  );
}

function Effect({ id, label, min, max, step, colour, title }:
  { id: string; label: string; min: number; max: number; step: number; colour: string; title?: string }) {
  return <Knob id={id} label={label} value={value(id)} min={min} max={max} step={step} colour={colour} title={title} onInput={turn(id)} />;
}

export function MixerFolder() {
  const level = readingSettings.value.level;
  const anySolo = Object.values(soloed.value).some(Boolean);
  return (
    <div class="desk">
      <div class="desk-head">
        <h2>Mixer</h2>
        <span class="hint">{anySolo ? 'Soloed: only the lit channels are heard.' : 'The band and the voice, side by side.'}</span>
      </div>
      <div class="module mixer" aria-label="Levels">
        <Level id="drums_gain" channel="drums" name="Drums" colour="var(--pulse)" />
        <Level id="bass_gain" channel="bass" name="Bass" colour="var(--bass)" />
        <Level id="melody_gain" channel="melody" name="Melody" colour="var(--rhyme)" />
        <Level id="chord_gain" channel="chords" name="Chords" colour="var(--chords)" title="The chord lane’s level, shared between a chord’s tones; heard only with the harmony mapping on" />
        <Strip channel="voice" name="Voice" voice>
          <Fader id="readingLevel" channel="voice" label="Voice level" engraved="Voice" value={level} min={0} max={2} step={0.05} colour="var(--ornament)"
            shown={Number(level).toFixed(2)} title="The voice's level into the master, against the music" onInput={(next) => setLevel(Number(next))} />
        </Strip>
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
