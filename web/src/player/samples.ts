// The samples the Prosody player plays, fetched and decoded on the page's thread with the browser's
// own decoder (`docs/player.md` §5: FLAC drums, the soundfonts' zones), then handed to the render
// worker once. Only the sounds a Performance uses are loaded, and of a soundfont only the zones its
// notes' keys fall in.
//
// The soundfonts are the webaudiofont files the page's Strudel already plays (the licensed variant
// `style::SOUNDFONT_VARIANTS` names), from the same host. Each is a script assigning one object:
// `zones`, each with its key range, its root (`originalPitch`, in cents), its tuning, its loop in
// the sample's frames, its sample rate, and the sample itself as a base64 MP3 (`file`). It is read
// as data here — its fields picked out by pattern — never run.

/** Where the webaudiofont files come from: the host the page's Strudel loads them from. */
export const SOUNDFONT_BASE = 'https://felixroos.github.io/webaudiofontdata/sound/';

/** A decoded sample: its channels and the rate they were decoded at. */
export interface Decoded {
  left: Float32Array;
  right: Float32Array;
  rate: number;
}

/** One soundfont zone as the file states it. */
export interface RawZone {
  low: number;
  high: number;
  /** The MIDI note the sample sounds at its own rate: `originalPitch` cents, less the tuning. */
  root: number;
  loopStart: number;
  loopEnd: number;
  sampleRate: number;
  /** The sample, base64. */
  file: string;
}

/** A soundfont zone, decoded, with its loop in the decoded sample's frames. */
export interface Zone extends Decoded {
  low: number;
  high: number;
  root: number;
  loopStart: number;
  loopEnd: number;
}

/** Every zone of a webaudiofont file that has a sample, in its order. Zones whose sample is
 * missing are left out; a file with none reads as no zones, and the notes needing it are then
 * counted missing by the synth. */
export function parseZones(text: string): RawZone[] {
  const zones: RawZone[] = [];
  for (const block of text.split(/\{\s*\n?\s*midi\s*:/).slice(1)) {
    const number = (name: string) => {
      const found = block.match(new RegExp(`[,{\\s]${name}\\s*:\\s*(-?[0-9.]+)`));
      return found ? Number(found[1]) : NaN;
    };
    const file = block.match(/[,\s]file\s*:\s*'([A-Za-z0-9+/=]*)'/)?.[1];
    if (!file) continue;
    const coarse = number('coarseTune'), fine = number('fineTune');
    zones.push({
      low: number('keyRangeLow'),
      high: number('keyRangeHigh'),
      // SoundFont 2 §8.1.3: tuning raises the pitch a sample sounds at, so the key it sounds as at
      // its own rate is the root less the tuning.
      root: number('originalPitch') / 100 - (Number.isFinite(coarse) ? coarse : 0) - (Number.isFinite(fine) ? fine / 100 : 0),
      loopStart: number('loopStart'),
      loopEnd: number('loopEnd'),
      sampleRate: number('sampleRate'),
      file,
    });
  }
  return zones.filter((zone) => Number.isFinite(zone.low) && Number.isFinite(zone.high) && Number.isFinite(zone.root));
}

/** The zones a set of keys needs: for each key, the first zone whose range holds it, or else the
 * nearest — the synth's own rule (prosody_synth `Instrument::zone`), so every zone it may pick is
 * loaded. */
export function zonesFor(zones: RawZone[], keys: Iterable<number>): RawZone[] {
  const chosen = new Set<RawZone>();
  for (const key of keys) {
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

/** Base64 to bytes. */
function bytesOf(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
}

/** An AudioBuffer's channels, copied out (a mono buffer gives an empty right channel). */
function channelsOf(buffer: AudioBuffer): Decoded {
  return {
    left: buffer.getChannelData(0).slice(),
    right: buffer.numberOfChannels > 1 ? buffer.getChannelData(1).slice() : new Float32Array(0),
    rate: buffer.sampleRate,
  };
}

/** A drum of a kit, from beside the page. */
export async function decodeDrum(context: BaseAudioContext, bank: string, piece: string): Promise<Decoded> {
  const response = await fetch(new URL(`./drums/${bank}/${piece}.flac`, document.baseURI));
  if (!response.ok) throw new Error(`${response.status} for drums/${bank}/${piece}.flac`);
  return channelsOf(await context.decodeAudioData(await response.arrayBuffer()));
}

/** A zone's name among its file's zones: its key range. */
export const zoneName = (zone: { low: number; high: number }) => `${zone.low}-${zone.high}`;

/** The zones of soundfont `file` that `keys` need, decoded, less those named in `have` (loaded
 * already, for other keys). The browser decodes at the context's rate, so a zone's loop, stated in
 * its own sample rate's frames, is scaled to the decoded ones. */
export async function decodeFont(context: BaseAudioContext, file: string, keys: Iterable<number>, have: ReadonlySet<string>): Promise<Zone[]> {
  const response = await fetch(`${SOUNDFONT_BASE}${file}.js`);
  if (!response.ok) throw new Error(`${response.status} for the soundfont ${file}`);
  const wanted = zonesFor(parseZones(await response.text()), keys).filter((zone) => !have.has(zoneName(zone)));
  return Promise.all(wanted.map(async (zone) => {
    const decoded = channelsOf(await context.decodeAudioData(bytesOf(zone.file)));
    const scale = zone.sampleRate > 0 ? decoded.rate / zone.sampleRate : 1;
    return {
      ...decoded,
      low: zone.low,
      high: zone.high,
      root: zone.root,
      loopStart: zone.loopStart * scale,
      loopEnd: zone.loopEnd * scale,
    };
  }));
}
