// The samples the Prosody player plays, fetched and decoded on the page's thread, then handed to the
// render worker once (`docs/player.md` §5). Only the sounds a Performance uses are loaded, and of a
// soundfont only the zones its notes' keys fall in.
//
// Everything is beside the page and lossless: the drums (`drums/`, scripts/render-drums.mjs) and the
// soundfonts (`soundfonts/`, scripts/render-soundfonts.mjs: a JSON file a soundfont describing its
// zones, each zone's recording a FLAC). Each file is decoded at its own rate, by an
// OfflineAudioContext made at that rate, so the browser decodes and never resamples: FLAC is
// lossless, so every browser hands the synth the same samples, and the synth moves them to the
// output's rate itself (`docs/player.md` §8, decision 3).

/** A decoded sample: its channels and the rate they were decoded at. */
export interface Decoded {
  left: Float32Array;
  right: Float32Array;
  rate: number;
}

/** One soundfont zone as its JSON states it (scripts/render-soundfonts.mjs). */
export interface RawZone {
  /** Its index among its soundfont's zones. */
  position: number;
  low: number;
  high: number;
  /** The MIDI note the recording sounds at its own rate. */
  root: number;
  /** Its loop, in the recording's frames, or null for none. */
  loopStart: number | null;
  loopEnd: number | null;
  /** The recording's rate. */
  rate: number;
  /** The recording, beside the JSON. */
  file: string;
}

/** A soundfont's JSON. */
export interface Manifest {
  name: string;
  source: string;
  licence: string;
  zones: RawZone[];
}

/** A soundfont zone, decoded, its loop in its recording's frames (NaN for none). */
export interface Zone extends Decoded {
  /** Its index among its soundfont's zones: the synth chooses between zones that could both play a
   * key by it, so the choice does not depend on which was loaded first. */
  position: number;
  low: number;
  high: number;
  root: number;
  loopStart: number;
  loopEnd: number;
}

/** The zones a set of keys needs: for each key, the first zone whose range holds the key below it
 * (Strudel's choice, measured), or else the nearest — the synth's own rule (prosody_synth
 * `Instrument::zone`, which says why), so every zone it may pick is loaded. */
export function zonesFor(zones: RawZone[], keys: Iterable<number>): RawZone[] {
  const chosen = new Set<RawZone>();
  for (const played of keys) {
    const key = Math.max(0, played - 1);
    const holding = zones.find((zone) => zone.low <= key && key <= zone.high);
    if (holding) { chosen.add(holding); continue; }
    let nearest: RawZone | null = null, distance = Infinity;
    for (const zone of zones) {
      const away = key < zone.low ? zone.low - key : key - zone.high;
      if (away < distance) { nearest = zone; distance = away; }
    }
    if (nearest) chosen.add(nearest);
  }
  return zones.filter((zone) => chosen.has(zone));
}

/** An AudioBuffer's channels, copied out (a mono buffer gives an empty right channel). */
function channelsOf(buffer: AudioBuffer): Decoded {
  return {
    left: buffer.getChannelData(0).slice(),
    right: buffer.numberOfChannels > 1 ? buffer.getChannelData(1).slice() : new Float32Array(0),
    rate: buffer.sampleRate,
  };
}

/** `bytes`, a file recorded at `rate`, decoded at that rate: an OfflineAudioContext decodes to its
 * own rate, so one made at the file's decodes without resampling. */
async function decodeAt(bytes: ArrayBuffer, rate: number): Promise<Decoded> {
  return channelsOf(await new OfflineAudioContext(1, 1, rate).decodeAudioData(bytes));
}

/** A drum of a kit, from beside the page, at its own rate (the drums are 44.1 kHz). */
export async function decodeDrum(bank: string, piece: string): Promise<Decoded> {
  const response = await fetch(new URL(`./drums/${bank}/${piece}.flac`, document.baseURI));
  if (!response.ok) throw new Error(`${response.status} for drums/${bank}/${piece}.flac`);
  return decodeAt(await response.arrayBuffer(), DRUM_RATE);
}

/** The rate scripts/render-drums.mjs writes every drum at. */
export const DRUM_RATE = 44_100;

/** A zone's name among its file's zones: its key range. */
export const zoneName = (zone: { low: number; high: number }) => `${zone.low}-${zone.high}`;

/** The zones of soundfont `file` that `keys` need, decoded at their own rates, less those named in
 * `have` (loaded already, for other keys). */
export async function decodeFont(file: string, keys: Iterable<number>, have: ReadonlySet<string>): Promise<Zone[]> {
  const base = new URL(`./soundfonts/`, document.baseURI);
  const response = await fetch(new URL(`${file}.json`, base));
  if (!response.ok) throw new Error(`${response.status} for the soundfont ${file}`);
  const manifest = await response.json() as Manifest;
  const wanted = zonesFor(manifest.zones, keys).filter((zone) => !have.has(zoneName(zone)));
  return Promise.all(wanted.map(async (zone) => {
    const recording = await fetch(new URL(zone.file, base));
    if (!recording.ok) throw new Error(`${recording.status} for soundfonts/${zone.file}`);
    const decoded = await decodeAt(await recording.arrayBuffer(), zone.rate);
    return {
      ...decoded,
      position: zone.position,
      low: zone.low,
      high: zone.high,
      root: zone.root,
      loopStart: zone.loopStart ?? NaN,
      loopEnd: zone.loopEnd ?? NaN,
    };
  }));
}
