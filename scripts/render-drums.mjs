#!/usr/bin/env node
// Renders the five drum kits the page plays into web/public/drums/, one FLAC per drum, from the
// recordings they were chosen from by ear (2026-10-31). Every source is CC0 or the Unlicense; the
// drum machines the page used to load from strudel.b-cdn.net carry no licence at all, so nothing
// of theirs may be shipped or cached. web/public/drums/SOURCES.md says where each file came from.
//
//   node scripts/render-drums.mjs <freesound dir>
//
// The GitHub kits are fetched at the commits below. Freesound's originals need a signed-in
// download, so <freesound dir> holds the three packs, unzipped: Terry93D's "FM Synth - Drum Kit"
// (19034), cabled_mess's "Modular Drums & One-Shots" (20450) and friedensreich's "Modular Kicks"
// (16692). Needs ffmpeg.
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(fileURLToPath(import.meta.url), '../..');
const out = path.join(root, 'web/public/drums');
const freesound = process.argv[2];
if (!freesound) throw new Error('usage: node scripts/render-drums.mjs <freesound dir>');

const REPOS = {
  uzu: ['tidalcycles/uzu-drumkit', '2f3e05c70ab4d73ad053a1467adec89bd27377a0'],
  virtuosity: ['sfzinstruments/virtuosity_drums', '9f04cf9a734527edfbb0a4eee1f674e45bbf71bc'],
  rusty: ['sfzinstruments/karoryfer.big-rusty-drums', 'f07ce00df34a46b6b08375be56fe116cf15782bc'],
};

// How long each drum may ring, in seconds, before it fades out.
const LENGTH = { bd: 1.2, sd: 1.0, hh: 0.5, oh: 1.6, rd: 3.0, cr: 3.0, lt: 1.2, mt: 1.2, ht: 1.2 };
// Each drum's peak, in dBFS: the kit's balance, baked into the files because the pattern gives
// the whole drum layer one gain. Struck drums at the top, the hats and ride that play on every
// subdivision well under them; the balance the kits were auditioned at.
const PEAK = { bd: -1, sd: -1, lt: -1, mt: -1, ht: -1, hh: -7, rd: -7, oh: -5, cr: -5 };

// A source is `<repo>:<path>`, or `fs:<id>` for a Freesound sound. A drum is one recording or
// several microphones of one hit, mixed, optionally retuned in semitones.
const hit = (...mix) => ({ mix, pitch: 0 });
const tuned = (pitch, ...mix) => ({ mix, pitch });
const mics = (stem, list) => list.map((mic) => stem.replace('{}', mic));
const R = 'rusty:Samples/';
const V = 'virtuosity:Samples/mid/';

const KITS = {
  // Switch Angel's own recordings from uzu-drumkit; it has no toms of hers, so its kick tuned up.
  uzu: {
    bd: hit('uzu:bd/10_bd_switchangel.wav'), sd: hit('uzu:sd/13_sd_switchangel_2.wav'),
    hh: hit('uzu:hh/12_hh_switchangel.wav'), oh: hit('uzu:oh/11_oh_switchangel.wav'),
    rd: hit('uzu:rd/10_rd_switchangel.wav'), cr: hit('uzu:cr/10_cr_switchangel.wav'),
    lt: tuned(4, 'uzu:bd/16_bd_switchangel.wav'), mt: tuned(8, 'uzu:bd/16_bd_switchangel.wav'),
    ht: tuned(12, 'uzu:bd/16_bd_switchangel.wav'),
  },
  // Terry93D's FM synthesis kit; its third crash keeps time as the ride.
  fm_drums: {
    bd: hit('fs:336433'), sd: hit('fs:336348'), hh: hit('fs:336430'), oh: hit('fs:336436'),
    rd: hit('fs:336427'), cr: hit('fs:336429'), lt: hit('fs:336432'), mt: hit('fs:336431'), ht: hit('fs:336434'),
  },
  // cabled_mess's modular hits and one of friedensreich's kicks; the toms are another of his
  // kicks tuned up, the crash is FM's.
  modular: {
    bd: hit('fs:270577'), sd: hit('fs:363212'), hh: hit('fs:363203'), oh: hit('fs:363130'),
    rd: hit('fs:363128'), cr: hit('fs:336429'),
    lt: tuned(4, 'fs:270570'), mt: tuned(8, 'fs:270570'), ht: tuned(12, 'fs:270570'),
  },
  // Big Rusty: every microphone of each hit, mixed.
  big_rusty: {
    bd: hit(...mics(`${R}kick_24/kick/{}/k_vl8_rr1.flac`, ['kick', 'oh', 'sn'])),
    sd: hit(...mics(`${R}snare_14/rimshot/{}/sn_rims_vl6_rr1.flac`, ['top', 'btm', 'oh'])),
    hh: hit(...mics(`${R}hihat_14/lc/{}/ht_lc_vl6_rr1.flac`, ['cl', 'oh'])),
    oh: hit(...mics(`${R}hihat_14/qo/{}/ht_qo_vl4_rr1.flac`, ['cl', 'oh'])),
    rd: hit(...mics(`${R}ride_22/rd/{}/rd_vl6_rr1.flac`, ['cl', 'oh'])),
    cr: hit(...mics(`${R}crash_17/cr/{}/cr_vl3_rr1.flac`, ['cl', 'oh'])),
    lt: hit(...mics(`${R}tom_18/center/{}/t18_vl8_rr1.flac`, ['cl', 'oh', 'sn'])),
    mt: hit(...mics(`${R}tom_15/center/{}/t15_vl7_rr1.flac`, ['cl', 'oh', 'sn'])),
    ht: hit(...mics(`${R}tom_14/center/{}/t14_vl6_rr1.flac`, ['cl', 'oh', 'sn'])),
  },
  // Virtuosity's mid (stereo room) microphones; it has two toms, so the mid is the high one tuned down.
  virtuosity: {
    bd: hit(`${V}kick/mid_kick_snoff_vl4_rr1.flac`), sd: hit(`${V}snare/mid_snare_center_vl24.flac`),
    hh: hit(`${V}hh/mid_hh_closed_vl2_rr1.flac`), oh: hit(`${V}hh/mid_hh_open_vl3_rr1.flac`),
    rd: hit(`${V}ride/mid_ride_ride_vl3_rr1.flac`), cr: hit(`${V}crash/mid_crash_crash_vl3_rr1.flac`),
    lt: hit(`${V}ltom/mid_ltom_center_vl14.flac`), mt: tuned(-3, `${V}htom/mid_htom_center_vl14.flac`),
    ht: hit(`${V}htom/mid_htom_center_vl14.flac`),
  },
};

const cache = fs.mkdtempSync(path.join(os.tmpdir(), 'prosody-drums-'));
const freesoundFiles = fs.readdirSync(freesound, { recursive: true }).map(String);

/** A local copy of a source, and where it came from. */
async function fetchSource(source) {
  const [repo, file] = [source.slice(0, source.indexOf(':')), source.slice(source.indexOf(':') + 1)];
  if (repo === 'fs') {
    const found = freesoundFiles.find((name) => path.basename(name).startsWith(`${file}__`));
    if (!found) throw new Error(`Freesound sound ${file} is not in ${freesound}`);
    return { local: path.join(freesound, found), url: `https://freesound.org/s/${file}/` };
  }
  const [name, commit] = REPOS[repo];
  const url = `https://raw.githubusercontent.com/${name}/${commit}/${file.split('/').map(encodeURIComponent).join('/')}`;
  const local = path.join(cache, `${repo}-${file.replaceAll('/', '_')}`);
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status} for ${url}`);
  fs.writeFileSync(local, Buffer.from(await response.arrayBuffer()));
  return { local, url: `https://github.com/${name}/blob/${commit}/${file}` };
}

const ffmpeg = (...args) => execFileSync('ffmpeg', ['-hide_banner', ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
const channels = (file) => Number(execFileSync('ffprobe', ['-v', 'error', '-select_streams', 'a:0',
  '-show_entries', 'stream=channels', '-of', 'csv=p=0', file], { encoding: 'utf8' }).trim());

/** Mixes, trims, retunes, fades and levels one drum into `target`. */
function render(files, { pitch }, piece, target) {
  const stereo = files.some((file) => channels(file) > 1);
  const layout = stereo ? 'stereo' : 'mono';
  const inputs = files.flatMap((file) => ['-i', file]);
  let chain = files.map((_, i) => `[${i}:a]aresample=44100,aformat=channel_layouts=${layout}[s${i}];`).join('');
  chain += files.length > 1
    ? `${files.map((_, i) => `[s${i}]`).join('')}amix=inputs=${files.length}:normalize=0[m];`
    : '[s0]anull[m];';
  const length = LENGTH[piece];
  const fade = Math.min(length * 0.4, 1.2);
  // Retuning by resampling, as a sampler does: a tom an octave up is half as long.
  chain += `[m]silenceremove=start_periods=1:start_threshold=-55dB,asetrate=${Math.round(44100 * 2 ** (pitch / 12))},`
    + `aresample=44100,atrim=0:${length},afade=t=out:st=${(length - fade).toFixed(3)}:d=${fade.toFixed(3)}[o]`;
  const raw = path.join(cache, 'raw.wav');
  ffmpeg('-y', '-v', 'error', ...inputs, '-filter_complex', chain, '-map', '[o]', raw);
  // volumedetect reports on stderr.
  const report = spawnSync('ffmpeg', ['-hide_banner', '-i', raw, '-af', 'volumedetect', '-f', 'null', '-'], { encoding: 'utf8' }).stderr;
  const peak = Number(/max_volume: (-?[\d.]+) dB/.exec(report)[1]);
  ffmpeg('-y', '-v', 'error', '-i', raw, '-af', `volume=${(PEAK[piece] - peak).toFixed(2)}dB`,
    '-sample_fmt', 's16', '-compression_level', '12', target);
}

const sources = [];
for (const [kit, drums] of Object.entries(KITS)) {
  fs.mkdirSync(path.join(out, kit), { recursive: true });
  for (const [piece, recipe] of Object.entries(drums)) {
    const fetched = await Promise.all(recipe.mix.map(fetchSource));
    render(fetched.map((f) => f.local), recipe, piece, path.join(out, kit, `${piece}.flac`));
    sources.push({ file: `${kit}/${piece}.flac`, pitch: recipe.pitch, from: fetched.map((f) => f.url) });
  }
}
const CREDITS = {
  uzu: 'Switch Angel\'s recordings in [uzu-drumkit](https://github.com/tidalcycles/uzu-drumkit), released under the Unlicense.',
  fm_drums: 'Terry93D, [FM Synth - Drum Kit](https://freesound.org/people/Terry93D/packs/19034/) on Freesound, CC0.',
  modular: 'cabled_mess, [Modular Drums & One-Shots](https://freesound.org/people/cabled_mess/packs/20450/), and '
    + 'friedensreich, [Modular Kicks](https://freesound.org/people/friedensreich/packs/16692/), on Freesound, CC0. '
    + 'The crash is Terry93D\'s.',
  big_rusty: 'Karoryfer Samples, [Big Rusty Drums](https://github.com/sfzinstruments/karoryfer.big-rusty-drums), CC0.',
  virtuosity: '[Virtuosity Drums](https://github.com/sfzinstruments/virtuosity_drums), sfzinstruments, CC0.',
};
const lines = [
  '# The drums',
  '',
  'Written by `scripts/render-drums.mjs`; do not edit by hand. Each file is one hit, every microphone',
  'of it mixed, its leading silence trimmed, faded out, and levelled to its drum\'s peak. A tuned file',
  'was resampled by the semitones shown, as a sampler retunes. Every source is in the public domain',
  '(CC0 or the Unlicense); the credits are a courtesy, not a condition.',
];
for (const kit of Object.keys(KITS)) {
  lines.push('', `## ${kit}`, '', CREDITS[kit], '');
  for (const source of sources.filter((s) => s.file.startsWith(`${kit}/`))) {
    const tuning = source.pitch ? `, tuned ${source.pitch > 0 ? '+' : ''}${source.pitch}` : '';
    lines.push(`- \`${source.file}\`${tuning}: ${source.from.map((url) => `<${url}>`).join(', ')}`);
  }
}
fs.writeFileSync(path.join(out, 'SOURCES.md'), `${lines.join('\n')}\n`);
fs.rmSync(cache, { recursive: true });
console.log(`${sources.length} drums rendered into ${path.relative(root, out)}, sources in SOURCES.md`);
