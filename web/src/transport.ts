// Play and Stop, through the Prosody player (`docs/player.md`): the piece's Performance rendered by
// our own synth. Strudel's runtime is not on the page; the pattern it would play is still the text
// the page shows, copies, shares and links to strudel.cc.
import * as player from './player';
import { duckChanged } from './player/speech';

/** Plays the piece of the side heard, or carries on with its newest version. */
export function play(): Promise<void> {
  return player.play();
}

/** Stops it. */
export function stop(): void {
  player.stop();
}

/** After mute or solo, or the reading's level: the player hears it as a stem gain, within a few ms. */
export function mixChanged(): void {
  player.sendGains();
}

/** After the duck is switched: the music under the reading follows from the next block rendered. */
export function duckSwitched(): void {
  duckChanged();
}
