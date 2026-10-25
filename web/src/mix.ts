// Mute and solo. A pattern's events each carry their layer's colour (`site.layers`); the music
// plays through a filter that reads the mixer's switches as each event is scheduled, so muting a
// layer or soloing one is heard at once, with no recompile and no change to the pattern — the
// pattern shown, and pasted into strudel.cc, is the poem's, whatever the mixer says.
import { site } from './data';
import { muted, soloed, type Channel } from './store';

/** The mixer's channel each layer plays through: both drum layers through the drums. */
export const CHANNEL_OF_LAYER: Record<string, Exclude<Channel, 'voice'>> = {
  pulse: 'drums', ornament: 'drums', bass: 'bass', rhyme: 'melody', chords: 'chords',
};

/** The layer an event's colour names, or undefined for an event that carries none. */
export function layerOf(colour: unknown): string | undefined {
  return site.layers.find((layer) => layer.colour === colour)?.name;
}

/** Whether a channel is heard: with any channel soloed, only the soloed ones; otherwise every one
 * not muted. A soloed channel is heard even if it is also muted, as on a desk. */
export function audible(channel: Channel, mute = muted.value, solo = soloed.value): boolean {
  if (Object.values(solo).some(Boolean)) return !!solo[channel];
  return !mute[channel];
}

/** An event's value as it is played: silent, and marked so the roll can show it, if its channel
 * is not heard; as it was otherwise. */
export function played(value: Record<string, unknown>): Record<string, unknown> {
  const layer = layerOf(value?.color);
  const channel = layer ? CHANNEL_OF_LAYER[layer] : undefined;
  if (!channel || audible(channel)) return value;
  return { ...value, gain: 0, muted: true };
}
