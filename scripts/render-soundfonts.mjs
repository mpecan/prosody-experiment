#!/usr/bin/env node
// Converts the 28 soundfonts the page plays into web/public/soundfonts/: each zone's recording as a
// lossless FLAC of its own, and one JSON file a soundfont describing its zones (docs/player.md §8,
// decision 3). The Prosody player decodes FLAC at the file's own rate, so every browser and the CLI
// get the same samples, where the webaudiofont files' MP3 zones were decoded (and resampled) by each
// browser its own way.
//
//   node scripts/render-soundfonts.mjs
//
// The sources are the licensed webaudiofont files `style::SOUNDFONT_VARIANTS` names (FluidR3 GM,
// MIT; GeneralUser GS, free for any use), fetched from felixroos/webaudiofontdata at the commit
// below. Their zones are read as data — fields picked out by pattern, never run — as
// web/src/player/samples.ts reads them. Identical recordings (two instruments sharing a zone) are
// written once, named by their content. Needs ffmpeg and ffprobe.
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(fileURLToPath(import.meta.url), '../..');
const out = path.join(root, 'web/public/soundfonts');
const REPO = 'felixroos/webaudiofontdata';
const COMMIT = '23ca907d4370a04fd89ca483a92915e4d6159ab9';

// The files to convert: exactly the variants the compiler plays, read from its own table so the two
// cannot drift.
const style = fs.readFileSync(path.join(root, 'crates/prosody-emit/src/style.rs'), 'utf8');
const table = style.slice(style.indexOf('pub const SOUNDFONT_VARIANTS'));
const variants = [...table.slice(0, table.indexOf('];')).matchAll(/variant\(\s*"([^"]+)",\s*"[^"]+",\s*"([^"]+)",?\s*\)/g)]
  .map(([, name, file]) => ({ name, file }));
if (variants.length !== 28) throw new Error(`expected 28 soundfonts in style.rs, found ${variants.length}`);

/** Each bank's licence, by the name in its file. */
const LICENCES = {
  FluidR3_GM: 'FluidR3 GM by Frank Wen, MIT licence',
  GeneralUserGS: 'GeneralUser GS by S. Christian Collins, free for any use (its licence permits use and redistribution, commercial included)',
};
/** Each licence's full text, with its copyright line, kept beside the zones in `LICENSES/` (an MIT
 * notice must travel with what it covers; a one-line attribution is not the notice). Written by
 * hand, not by this script: it checks they are there. */
const LICENCE_FILES = {
  FluidR3_GM: 'LICENSES/FluidR3_GM.txt',
  GeneralUserGS: 'LICENSES/GeneralUser_GS.txt',
};
const SOURCE_LICENCE = 'LICENSES/webaudiofontdata.txt';
for (const file of [...Object.values(LICENCE_FILES), SOURCE_LICENCE]) {
  if (!fs.existsSync(path.join(out, file))) throw new Error(`${file} is missing from ${path.relative(root, out)}`);
}

/** Every zone of a webaudiofont file that has a sample, in its order (as samples.ts `parseZones`). */
function parseZones(text) {
  const zones = [];
  for (const block of text.split(/\{\s*\n?\s*midi\s*:/).slice(1)) {
    const number = (name) => {
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

const cache = fs.mkdtempSync(path.join(os.tmpdir(), 'prosody-soundfonts-'));
const probe = (file) => execFileSync('ffprobe', ['-v', 'error', '-select_streams', 'a:0', '-show_entries',
  'stream=sample_rate,channels', '-of', 'csv=p=0', file], { encoding: 'utf8' }).trim().split(',').map(Number);

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, 'zones'), { recursive: true });
const written = new Map();
const sources = [];
let bytes = 0;
for (const { name, file } of variants) {
  const url = `https://raw.githubusercontent.com/${REPO}/${COMMIT}/sound/${file}.js`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} for ${url}`);
  const zones = parseZones(await response.text());
  const described = zones.map((zone, position) => {
    const mp3 = Buffer.from(zone.file, 'base64');
    const hash = crypto.createHash('sha256').update(mp3).digest('hex').slice(0, 16);
    const flac = `zones/${hash}.flac`;
    if (!written.has(hash)) {
      const local = path.join(cache, `${hash}.mp3`);
      fs.writeFileSync(local, mp3);
      // 4096-frame blocks, stated: left to itself ffmpeg wrote these zones in 47-frame blocks, which
      // are valid FLAC but which Apple's decoder refuses outright — on iOS and in Safari every zone
      // failed to decode and its instrument was silent (docs/player.md §14).
      execFileSync('ffmpeg', ['-hide_banner', '-v', 'error', '-y', '-i', local, '-sample_fmt', 's16',
        '-compression_level', '12', '-frame_size', '4096', path.join(out, flac)]);
      const [rate] = probe(path.join(out, flac));
      written.set(hash, rate);
      bytes += fs.statSync(path.join(out, flac)).size;
    }
    const rate = written.get(hash);
    // The loop is stated in the original recording's frames; an MP3 holds only standard rates, so a
    // zone recorded at, say, 30 kHz was stored at 32 kHz, and its loop moves with it.
    const scale = zone.sampleRate > 0 ? rate / zone.sampleRate : 1;
    const looped = zone.loopEnd > zone.loopStart;
    return {
      position,
      low: zone.low,
      high: zone.high,
      root: Math.round(zone.root * 1e6) / 1e6,
      loopStart: looped ? Math.round(zone.loopStart * scale * 1000) / 1000 : null,
      loopEnd: looped ? Math.round(zone.loopEnd * scale * 1000) / 1000 : null,
      rate,
      file: flac,
    };
  });
  const bank = Object.keys(LICENCES).find((key) => file.includes(key));
  if (!bank) throw new Error(`no licence known for ${file}`);
  const manifest = {
    name, source: `https://github.com/${REPO}/blob/${COMMIT}/sound/${file}.js`, licence: LICENCES[bank],
    licenceFile: LICENCE_FILES[bank], zones: described,
  };
  fs.writeFileSync(path.join(out, `${file}.json`), `${JSON.stringify(manifest, null, 1)}\n`);
  sources.push({ name, file, url: manifest.source, bank, zones: described.length });
}
const lines = [
  '# The soundfonts',
  '',
  'Written by `scripts/render-soundfonts.mjs`; do not edit by hand. Each `<file>.json` describes one',
  'soundfont\'s zones — the keys each plays, the pitch its recording sounds at, its loop in the',
  'recording\'s frames, its rate — and names the zone\'s recording in `zones/`, a 16-bit FLAC decoded',
  'from the webaudiofont file\'s MP3 (lossless from there on). A recording two soundfonts share is',
  'stored once, named by the hash of its MP3.',
  '',
  `The webaudiofont files come from [${REPO}](https://github.com/${REPO}) (MIT, [its licence](${SOURCE_LICENCE})) at commit \`${COMMIT}\`.`,
  'The recordings keep their banks\' licences, whose full texts and copyright lines are in `LICENSES/`:',
  '',
  ...Object.entries(LICENCES).map(([bank, licence]) => `- ${licence}: [${LICENCE_FILES[bank]}](${LICENCE_FILES[bank]}).`),
  '',
  '| Instrument | File | Zones | Bank |',
  '|---|---|---|---|',
  ...sources.map((s) => `| \`${s.name}\` | [${s.file}](${s.url}) | ${s.zones} | ${LICENCES[s.bank]} |`),
];
fs.writeFileSync(path.join(out, 'SOURCES.md'), `${lines.join('\n')}\n`);
fs.rmSync(cache, { recursive: true });
console.log(`${sources.length} soundfonts, ${written.size} recordings, ${(bytes / 1e6).toFixed(1)} MB of FLAC in ${path.relative(root, out)}`);
