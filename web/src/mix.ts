// Mute and solo. Each layer of the piece plays through one of the mixer's channels, and each channel
// is a stem of the Prosody player (`docs/player.md` §4): muting a channel or soloing one is a stem
// gain in the worklet, heard at once, with no recompile and no change to the pattern — the pattern
// shown, and pasted into strudel.cc, is the poem's, whatever the mixer says.
import { site } from './data';
import { muted, soloed, type Channel } from './store';

/** The mixer's channel each layer plays through: both drum layers through the drums. */
export const CHANNEL_OF_LAYER: Record<string, Exclude<Channel, 'voice'>> = {
  pulse: 'drums', ornament: 'drums', bass: 'bass', rhyme: 'melody', chords: 'chords',
};

/** The layer a colour names, or undefined for none (`site.layers`). */
export function layerOf(colour: unknown): string | undefined {
  return site.layers.find((layer) => layer.colour === colour)?.name;
}

/** Whether a channel is heard: with any channel soloed, only the soloed ones; otherwise every one
 * not muted. A soloed channel is heard even if it is also muted, as on a desk. */
export function audible(channel: Channel, mute = muted.value, solo = soloed.value): boolean {
  if (Object.values(solo).some(Boolean)) return !!solo[channel];
  return !mute[channel];
}
