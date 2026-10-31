// Play and Stop, through whichever player is chosen (`docs/player.md` §6, "Listening"): Strudel's
// runtime playing the pattern text, or the Prosody player playing the Performance. The page keeps
// both until they sound close enough; Strudel stays the default meanwhile.
//
// The Prosody player is loaded only when it is first chosen: its worker and worklet are modules of
// their own, and a visit that never plays through it never fetches them.
import * as strudel from './strudel';
import { load, save } from './storage';
import { player, playing, type PlayerName } from './store';

type ProsodyPlayer = typeof import('./player');
let prosody: Promise<ProsodyPlayer> | null = null;
const prosodyPlayer = () => (prosody ??= import('./player'));

/** Plays the piece of the side heard, or carries on with its newest version. */
export async function play(): Promise<void> {
  if (player.value === 'prosody') return (await prosodyPlayer()).play();
  return strudel.play();
}

/** Stops whichever is playing. */
export function stop(): void {
  if (player.value === 'prosody') void prosodyPlayer().then((p) => p.stop());
  else strudel.stop();
}

/** After mute or solo: the Prosody player hears it as a stem gain, at once (Strudel reads the
 * switches as it schedules each event: mix.ts). */
export function mixChanged(): void {
  if (prosody) void prosody.then((p) => p.sendGains());
}

/** Chooses the player, remembered in this browser. While it plays, the other one takes over from
 * the top. */
export function choosePlayer(next: PlayerName): void {
  if (next === player.value) return;
  const wasPlaying = playing.value;
  if (wasPlaying) stop();
  player.value = next;
  save('prosody.player', next);
  if (wasPlaying) void play();
}

/** The player this browser chose last time, or Strudel. */
export function restorePlayer(): void {
  player.value = load('prosody.player') === 'prosody' ? 'prosody' : 'strudel';
}
